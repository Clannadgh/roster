use std::sync::OnceLock;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::account::{self, Account};
use crate::scope::{Region, Scope};
use crate::store::{now_ms, read_secure_json, store_dir, write_secure_json};

const API_PREFIX: &str = "/v2/billing/meter";
const REFRESH_PREFIX: &str = "/v2/plugin/auth/token/refresh";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckinStatus {
    pub ok: bool,
    pub today_checked_in: bool,
    pub daily_credit: Option<f64>,
    pub today_credit: Option<f64>,
    pub streak_days: Option<i64>,
    pub error: Option<String>,
}

impl CheckinStatus {
    fn error(message: impl Into<String>) -> Self {
        Self {
            ok: false,
            today_checked_in: false,
            daily_credit: None,
            today_credit: None,
            streak_days: None,
            error: Some(message.into()),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckinOutcome {
    pub account_id: String,
    pub result: String,
    pub amount: Option<f64>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CheckinLog {
    ts: i64,
    account_id: String,
    name: String,
    result: String,
    #[serde(default)]
    amount: Option<f64>,
    #[serde(default)]
    error: Option<String>,
}

static HTTP_CLIENT: OnceLock<reqwest::Client> = OnceLock::new();

fn client() -> &'static reqwest::Client {
    HTTP_CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .expect("build http client")
    })
}

pub(crate) fn auth_headers(account: &Account) -> Vec<(&'static str, String)> {
    let mut headers = vec![
        ("Authorization", format!("Bearer {}", account.access_token)),
        ("Accept", "application/json".to_string()),
        ("Content-Type", "application/json".to_string()),
    ];
    if let Some(uid) = account.uid.as_deref().filter(|value| !value.is_empty()) {
        headers.push(("X-User-Id", uid.to_string()));
    }
    if !account.domain.is_empty() {
        headers.push(("X-Domain", account.domain.clone()));
    }
    headers
}

async fn post_json(url: &str, headers: Vec<(&'static str, String)>) -> Result<Value, String> {
    let mut request = client()
        .post(url)
        .header("Content-Type", "application/json");
    for (name, value) in headers {
        request = request.header(name, value);
    }
    let response = request
        .json(&json!({}))
        .send()
        .await
        .map_err(|error| format!("网络错误：{error}"))?;
    let status = response.status();
    let text = response.text().await.unwrap_or_default();
    serde_json::from_str(&text).map_err(|_| {
        let snippet: String = text.chars().take(300).collect();
        format!("HTTP {status}：{snippet}")
    })
}

pub(crate) fn response_message(response: &Value) -> String {
    response
        .get("message")
        .or_else(|| response.get("msg"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string()
}

pub(crate) fn is_unauthorized(response: &Value) -> bool {
    let code = response.get("code").and_then(Value::as_i64).unwrap_or(-1);
    if code == 401 || code == 403 {
        return true;
    }
    let message = response_message(response).to_lowercase();
    ["unauthorized", "401", "登录", "失效", "过期", "token"]
        .iter()
        .any(|keyword| message.contains(keyword))
}

fn normalize_status(response: &Value) -> Option<CheckinStatus> {
    let code = response.get("code").and_then(Value::as_i64).unwrap_or(-1);
    if code != 0 && code != 200 {
        return None;
    }
    let data = response.get("data").cloned().unwrap_or_else(|| json!({}));
    Some(CheckinStatus {
        ok: true,
        today_checked_in: data
            .get("today_checked_in")
            .or_else(|| data.get("todayCheckedIn"))
            .and_then(Value::as_bool)
            .unwrap_or(false),
        daily_credit: data.get("daily_credit").and_then(Value::as_f64),
        today_credit: data.get("today_credit").and_then(Value::as_f64),
        streak_days: data.get("streak_days").and_then(Value::as_i64),
        error: None,
    })
}

pub(crate) async fn refresh_account(scope: Scope, account: &Account) -> Result<Account, String> {
    let Some(refresh_token) = account.refresh_token.clone().filter(|value| !value.is_empty()) else {
        return Err("缺少 refresh token，需重新登录".to_string());
    };
    let url = format!("{}{REFRESH_PREFIX}", scope.billing_base());
    let response = post_json(
        &url,
        vec![
            ("Authorization", format!("Bearer {}", account.access_token)),
            ("X-Refresh-Token", refresh_token),
            ("Accept", "application/json".to_string()),
            ("Content-Type", "application/json".to_string()),
        ],
    )
    .await?;
    let code = response.get("code").and_then(Value::as_i64).unwrap_or(-1);
    if code != 0 && code != 200 {
        return Err(format!("刷新失败（code={code}）：{}", response_message(&response)));
    }
    let data = response.get("data").cloned().unwrap_or_else(|| json!({}));
    let access_token = data
        .get("accessToken")
        .and_then(Value::as_str)
        .or_else(|| data.get("access_token").and_then(Value::as_str))
        .map(str::to_string)
        .ok_or_else(|| "刷新响应缺少 accessToken".to_string())?;

    let mut updated = account.clone();
    updated.access_token = access_token;
    if let Some(new_refresh) = data
        .get("refreshToken")
        .and_then(Value::as_str)
        .or_else(|| data.get("refresh_token").and_then(Value::as_str))
    {
        updated.refresh_token = Some(new_refresh.to_string());
    }
    if let Some(expires_in) = data.get("expiresIn").and_then(Value::as_i64) {
        updated.expires_at = Some(now_ms() + expires_in * 1000);
    }
    updated.updated_at = now_ms();
    account::upsert(scope.as_str(), updated.clone())?;
    Ok(updated)
}

async fn status_for(scope: Scope, account_id: &str) -> Result<(CheckinStatus, Account), String> {
    if scope.region == Region::Global {
        return Ok((
            CheckinStatus::error("WorkBuddy 国际版没有签到接口"),
            account::find(scope.as_str(), account_id)
                .ok_or_else(|| "账号不存在".to_string())?,
        ));
    }
    let mut account = account::find(scope.as_str(), account_id)
        .ok_or_else(|| "账号不存在".to_string())?;
    if let Some(status) = crate::checkin_marks::cached_status(scope, account_id) {
        return Ok((status, account));
    }

    let url = format!("{}{API_PREFIX}/checkin-activity-status", scope.billing_base());
    let mut response = post_json(&url, auth_headers(&account)).await?;
    if is_unauthorized(&response) && account.refresh_token.is_some() {
        account = refresh_account(scope, &account).await?;
        response = post_json(&url, auth_headers(&account)).await?;
    }

    if let Some(status) = normalize_status(&response) {
        if status.today_checked_in {
            crate::checkin_marks::mark_signed(scope, account_id);
        }
        return Ok((status, account));
    }

    let fallback = format!("{}{API_PREFIX}/checkin-status", scope.billing_base());
    let mut second = post_json(&fallback, auth_headers(&account)).await?;
    if is_unauthorized(&second) && account.refresh_token.is_some() {
        account = refresh_account(scope, &account).await?;
        second = post_json(&fallback, auth_headers(&account)).await?;
    }
    if let Some(status) = normalize_status(&second) {
        if status.today_checked_in {
            crate::checkin_marks::mark_signed(scope, account_id);
        }
        return Ok((status, account));
    }

    let code = second.get("code").and_then(Value::as_i64).unwrap_or(-1);
    let message = response_message(&second);
    let error = if message.is_empty() {
        format!("code={code}")
    } else {
        message
    };
    Ok((CheckinStatus::error(error), account))
}

pub async fn status(scope: Scope, account_id: &str) -> Result<CheckinStatus, String> {
    status_for(scope, account_id).await.map(|(status, _)| status)
}

async fn perform(scope: Scope, account: &Account) -> Result<Value, String> {
    let url = format!("{}{API_PREFIX}/daily-checkin", scope.billing_base());
    let mut response = post_json(&url, auth_headers(account)).await?;
    if is_unauthorized(&response) && account.refresh_token.is_some() {
        let refreshed = refresh_account(scope, account).await?;
        response = post_json(&url, auth_headers(&refreshed)).await?;
    }
    Ok(response)
}

pub(crate) fn append_log(scope: Scope, account: &Account, outcome: &CheckinOutcome) {
    let path = store_dir().join(format!("checkin-logs.{}.json", scope.as_str()));
    let mut logs: Vec<CheckinLog> = read_secure_json(&path).unwrap_or_default();
    logs.push(CheckinLog {
        ts: now_ms(),
        account_id: outcome.account_id.clone(),
        name: account
            .email
            .clone()
            .or_else(|| account.nickname.clone())
            .or_else(|| account.uid.clone())
            .unwrap_or_else(|| "未知账号".to_string()),
        result: outcome.result.clone(),
        amount: outcome.amount,
        error: outcome.error.clone(),
    });
    if logs.len() > 500 {
        let drain = logs.len() - 500;
        logs.drain(0..drain);
    }
    let _ = write_secure_json(&path, &logs);
}

pub async fn checkin(scope: Scope, account_id: &str) -> CheckinOutcome {
    let outcome = |result: &str, amount: Option<f64>, error: Option<String>| CheckinOutcome {
        account_id: account_id.to_string(),
        result: result.to_string(),
        amount,
        error,
    };

    if scope.region == Region::Global {
        return outcome(
            "unsupported",
            None,
            Some("WorkBuddy 国际版没有签到接口".to_string()),
        );
    }
    if account::find(scope.as_str(), account_id).is_none() {
        return outcome("error", None, Some("账号不存在".to_string()));
    }

    let (status, account) = match status_for(scope, account_id).await {
        Ok(value) => value,
        Err(error) => return outcome("error", None, Some(error)),
    };
    if !status.ok {
        return outcome(
            "error",
            None,
            Some(status.error.unwrap_or_else(|| "查询签到状态失败".to_string())),
        );
    }
    if status.today_checked_in {
        crate::checkin_marks::mark_signed(scope, account_id);
        return outcome("already", None, None);
    }

    let reward = [status.daily_credit, status.today_credit]
        .into_iter()
        .flatten()
        .find(|amount| *amount > 0.0);

    let response = match perform(scope, &account).await {
        Ok(response) => response,
        Err(error) => return outcome("error", None, Some(error)),
    };
    let code = response.get("code").and_then(Value::as_i64).unwrap_or(-1);
    let message = response_message(&response);
    let (result, error) = if code == 0 || code == 200 {
        ("success", None)
    } else if message.contains("已签到") || message.to_lowercase().contains("repeat") {
        ("already", None)
    } else if message.contains("企业账号") && message.contains("不支持") {
        ("unsupported", Some(message))
    } else {
        let error = if message.is_empty() {
            format!("code={code}")
        } else {
            message
        };
        ("error", Some(error))
    };

    if matches!(result, "success" | "already") {
        crate::checkin_marks::mark_signed(scope, account_id);
    }

    let outcome = outcome(
        result,
        if result == "success" { reward } else { None },
        error,
    );
    append_log(scope, &account, &outcome);
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
