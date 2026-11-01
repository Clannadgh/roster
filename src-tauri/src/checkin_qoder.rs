use std::sync::OnceLock;
use std::time::Duration;

use serde_json::{json, Value};

use crate::account::{self, Account};
use crate::checkin_workbuddy::{CheckinOutcome, CheckinStatus};
use crate::scope::{Region, Scope};
use crate::store::now_ms;

const PATH_CAMPAIGNS: &str = "/sash/api/v1/me/campaigns";
const PATH_CLAIM: &str = "/sash/api/v1/me/campaigns/{id}/claim";
const CLIENT_TYPE: &str = "10";
const CLIENT_VERSION: &str = "0.4.3";

fn openapi_base(region: Region) -> &'static str {
    match region {
        Region::Cn => "https://openapi.qoder.com.cn",
        Region::Global => "https://openapi.qoder.sh",
    }
}

fn website(region: Region) -> &'static str {
    match region {
        Region::Cn => "https://qoder.com.cn",
        Region::Global => "https://qoder.com",
    }
}

fn derive_id(uid: &str, salt: &str) -> String {
    use md5::{Digest, Md5};
    let mut hasher = Md5::new();
    hasher.update(format!("{salt}:{uid}").as_bytes());
    format!("{:x}", hasher.finalize())
}

static HTTP_CLIENT: OnceLock<reqwest::Client> = OnceLock::new();

pub(crate) fn client() -> &'static reqwest::Client {
    HTTP_CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .expect("build http client")
    })
}

fn apply_headers(
    mut request: reqwest::RequestBuilder,
    headers: Vec<(&'static str, String)>,
) -> reqwest::RequestBuilder {
    for (name, value) in headers {
        if let Ok(value) = reqwest::header::HeaderValue::from_str(&value) {
            request = request.header(name, value);
        }
    }
    request
}

pub(crate) fn headers(scope: Scope, account: &Account) -> Vec<(&'static str, String)> {
    let uid = account.uid.as_deref().unwrap_or("");
    let request_id = format!("{}-{:06}", derive_id(uid, "req"), now_ms() % 1_000_000);
    vec![
        ("Content-Type", "application/json".to_string()),
        ("Accept", "application/json, text/plain, */*".to_string()),
        ("User-Agent", "Qoder".to_string()),
        (
            "Authorization",
            format!("Bearer {}", account.access_token),
        ),
        ("X-Request-ID", request_id),
        ("X-Machine-ID", derive_id(uid, "machine")),
        ("X-Session-ID", derive_id(uid, "session")),
        ("Origin", website(scope.region).to_string()),
        ("Referer", format!("{}/", website(scope.region))),
        ("cosy-clienttype", CLIENT_TYPE.to_string()),
        ("cosy-version", CLIENT_VERSION.to_string()),
    ]
}

async fn fetch_campaigns(scope: Scope, account: &Account) -> Result<Value, String> {
    let url = format!("{}{PATH_CAMPAIGNS}", openapi_base(scope.region));
    let request = apply_headers(
        client().get(&url).timeout(Duration::from_secs(15)),
        headers(scope, account),
    );
    let response = request.send().await.map_err(|error| format!("活动列表请求失败：{error}"))?;
    let status = response.status().as_u16();
    let body = response.text().await.unwrap_or_default();
    if matches!(status, 404 | 405 | 410) {
        return Err("本区域没有活动平台接口".to_string());
    }
    if !(200..300).contains(&status) {
        let detail: String = body.chars().take(160).collect();
        return Err(format!("活动列表失败：HTTP {status} {detail}"));
    }
    serde_json::from_str(&body).map_err(|error| format!("活动列表响应不是 JSON：{error}"))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Class {
    Claimable,
    Already,
    View,
}

#[derive(Debug, Clone)]
struct CampaignItem {
    id: String,
    title: String,
    action_type: String,
    claim_status: String,
    benefit_kind: String,
    benefit_amount: f64,
}

impl CampaignItem {
    fn is_credit_kind(&self) -> bool {
        let kind = self.benefit_kind.trim().to_ascii_uppercase();
        kind.is_empty() || kind == "CREDITS"
    }

    fn class(&self) -> Class {
        let actionable = self.action_type.trim().is_empty()
            || self.action_type.eq_ignore_ascii_case("CLAIM_BENEFIT");
        if self.claim_status == "CLAIMED" {
            return if self.benefit_amount > 0.0 || actionable {
                Class::Already
            } else {
                Class::View
            };
        }
        if !actionable {
            return Class::View;
        }
        if self.claim_status != "CLAIMABLE" {
            return Class::View;
        }
        Class::Claimable
    }
}

fn string_of(value: Option<&Value>) -> String {
    match value {
        Some(Value::String(text)) => text.trim().to_string(),
        Some(Value::Number(number)) => number.to_string(),
        _ => String::new(),
    }
}

fn number_of(value: Option<&Value>) -> f64 {
    match value {
        Some(Value::Number(number)) => number.as_f64().unwrap_or(0.0),
        Some(Value::String(text)) => text.trim().parse().unwrap_or(0.0),
        _ => 0.0,
    }
}

fn parse_item(raw: &Value) -> Option<CampaignItem> {
    let object = raw.as_object()?;
    let id = string_of(
        object
            .get("campaignId")
            .or_else(|| object.get("campaign_id")),
    );
    if id.is_empty() {
        return None;
    }
    let placements = object.get("placements").and_then(Value::as_array);
    let mut title = String::new();
    for placement in placements.into_iter().flatten() {
        let Some(content) = placement.get("content").and_then(Value::as_object) else {
            continue;
        };
        for language in ["zh", "en"] {
            if !title.is_empty() {
                break;
            }
            title = content
                .get(language)
                .and_then(Value::as_object)
                .and_then(|value| value.get("title"))
                .and_then(Value::as_str)
                .unwrap_or("")
                .trim()
                .to_string();
        }
        if !title.is_empty() {
            break;
        }
    }
    let benefit = object.get("benefit").and_then(Value::as_object);
    Some(CampaignItem {
        id,
        title,
        action_type: string_of(object.get("actionType").or_else(|| object.get("action_type"))),
        claim_status: string_of(
            object
                .get("claimStatus")
                .or_else(|| object.get("claim_status")),
        )
        .to_ascii_uppercase(),
        benefit_kind: benefit
            .and_then(|value| value.get("kind"))
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim()
            .to_string(),
        benefit_amount: benefit
            .and_then(|value| value.get("amount"))
            .map(|value| number_of(Some(value)))
            .unwrap_or(0.0),
    })
}

fn parse_items(payload: &Value) -> Vec<CampaignItem> {
    payload
        .get("campaigns")
        .and_then(Value::as_array)
        .map(|rows| rows.iter().filter_map(parse_item).collect())
        .unwrap_or_default()
}

fn credit_items(items: Vec<CampaignItem>) -> Vec<CampaignItem> {
    items.into_iter().filter(CampaignItem::is_credit_kind).collect()
}

pub async fn status(scope: Scope, account_id: &str) -> Result<CheckinStatus, String> {
    let account = account::find(scope.as_str(), account_id)
        .ok_or_else(|| "账号不存在".to_string())?;
    if let Some(status) = crate::checkin_marks::cached_status(scope, account_id) {
        return Ok(status);
    }
    let payload = fetch_campaigns(scope, &account).await?;
    let credits = credit_items(parse_items(&payload));
    if credits.is_empty() {
        return Ok(CheckinStatus {
            ok: false,
            today_checked_in: false,
            daily_credit: None,
            today_credit: None,
            streak_days: None,
            error: Some("没有可领取的积分类活动".to_string()),
        });
    }

    let mut claimable_reward = 0.0;
    let mut claimed_reward = 0.0;
    let mut claimable_count = 0;
    for item in &credits {
        match item.class() {
            Class::Claimable => {
                claimable_count += 1;
                claimable_reward += item.benefit_amount;
            }
            Class::Already => claimed_reward += item.benefit_amount,
            Class::View => {}
        }
    }

    let today_checked_in = claimable_count == 0;
    if today_checked_in {
        crate::checkin_marks::mark_signed(scope, account_id);
    }

    Ok(CheckinStatus {
        ok: true,
        today_checked_in,
        daily_credit: (claimable_reward > 0.0).then_some(claimable_reward),
        today_credit: (claimable_count == 0 && claimed_reward > 0.0).then_some(claimed_reward),
        streak_days: None,
        error: None,
    })
}

struct ClaimResult {
    ok: bool,
    replayed: bool,
    amount: f64,
    message: String,
}

async fn claim_one(scope: Scope, account: &Account, campaign_id: &str) -> Result<ClaimResult, String> {
    let url = format!(
        "{}{}",
        openapi_base(scope.region),
        PATH_CLAIM.replace("{id}", campaign_id)
    );
    let request = apply_headers(
        client()
            .post(&url)
            .timeout(Duration::from_secs(20))
            .json(&json!({})),
        headers(scope, account),
    );
    let response = request.send().await.map_err(|error| format!("领取请求失败：{error}"))?;
    let status = response.status().as_u16();
    let body = response.text().await.unwrap_or_default();
    if status == 409 {
        return Ok(ClaimResult {
            ok: true,
            replayed: true,
            amount: 0.0,
            message: "今日已领取".to_string(),
        });
    }
    if !(200..300).contains(&status) {
        let detail: String = body.chars().take(160).collect();
        return Err(format!("领取失败：HTTP {status} {detail}"));
    }
    let payload: Value = serde_json::from_str(&body)
        .map_err(|error| format!("领取响应不是 JSON：{error}"))?;
    let failure_code = string_of(
        payload
            .get("failureCode")
            .or_else(|| payload.get("failure_code")),
    )
    .to_ascii_uppercase();
    if failure_code == "SAME_PERSON_ALREADY_CLAIMED" {
        return Ok(ClaimResult {
            ok: true,
            replayed: true,
            amount: 0.0,
            message: "同人已领取".to_string(),
        });
    }
    if payload.get("success") == Some(&Value::Bool(false)) {
        let error = payload
            .get("error")
            .map(|value| value.to_string())
            .unwrap_or_else(|| body.chars().take(160).collect());
        return Err(format!("领取失败：{error}"));
    }
    let claim_status = string_of(payload.get("status")).to_ascii_uppercase();
    let amount = payload
        .get("benefit")
        .and_then(Value::as_object)
        .and_then(|benefit| benefit.get("amount"))
        .map(|value| number_of(Some(value)))
        .or_else(|| payload.get("amount").map(|value| number_of(Some(value))))
        .unwrap_or(0.0);
    let replayed = payload
        .get("replayed")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let ok = replayed
        || matches!(claim_status.as_str(), "CLAIMED" | "GRANTED" | "SUCCESS");
    Ok(ClaimResult {
        ok,
        replayed,
        amount,
        message: claim_status,
    })
}

pub async fn checkin(scope: Scope, account_id: &str) -> CheckinOutcome {
    let outcome = |result: &str, amount: Option<f64>, error: Option<String>| CheckinOutcome {
        account_id: account_id.to_string(),
        result: result.to_string(),
        amount,
        error,
    };

    let account = match account::find(scope.as_str(), account_id) {
        Some(account) => account,
        None => return outcome("error", None, Some("账号不存在".to_string())),
    };
    let payload = match fetch_campaigns(scope, &account).await {
        Ok(payload) => payload,
        Err(error) => return outcome("error", None, Some(error)),
    };
    let credits = credit_items(parse_items(&payload));
    if credits.is_empty() {
        return outcome(
            "error",
            None,
            Some("没有可领取的积分类活动".to_string()),
        );
    }

    let claimable: Vec<CampaignItem> = credits
        .iter()
        .filter(|item| item.class() == Class::Claimable)
        .cloned()
        .collect();
    if claimable.is_empty() {
        crate::checkin_marks::mark_signed(scope, account_id);
        return outcome("already", None, None);
    }

    let mut earned = 0.0;
    let mut claimed = 0;
    let mut replayed = 0;
    let mut errors: Vec<String> = Vec::new();
    for item in &claimable {
        match claim_one(scope, &account, &item.id).await {
            Ok(result) => {
                if result.replayed {
                    replayed += 1;
                } else if result.ok {
                    claimed += 1;
                    earned += result.amount;
                } else {
                    errors.push(format!("{}：{}", item.title, result.message));
                }
            }
            Err(error) => errors.push(format!("{}：{error}", item.title)),
        }
    }

    let (result, amount, error) = if claimed > 0 {
        ("success", Some(earned), None)
    } else if replayed > 0 && errors.is_empty() {
        ("already", None, None)
    } else {
        (
            "error",
            None,
            Some(
                errors
                    .first()
                    .cloned()
                    .unwrap_or_else(|| "领取失败".to_string()),
            ),
        )
    };

    if matches!(result, "success" | "already") {
        crate::checkin_marks::mark_signed(scope, account_id);
    }

    let outcome = outcome(result, amount, error);
    crate::checkin_workbuddy::append_log(scope, &account, &outcome);
    outcome
}

pub async fn checkin_all(scope: Scope) -> Vec<CheckinOutcome> {
    let mut results = Vec::new();
    for account in account::load(scope.as_str()) {
        if let Some(outcome) = crate::checkin_marks::skipped_outcome(scope, &account.id) {
            results.push(outcome);
            continue;
        }
        results.push(checkin(scope, &account.id).await);
    }
    results
}
