use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::account::{self, Account};
use crate::checkin_workbuddy;
use crate::scope::{Platform, Region, Scope};
use crate::store::{atomic_write, store_dir};

const TRAVEL_PREFIX: &str = "/activity/growth/buddy/travel";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TravelStatus {
    pub state: String,
    pub location_name: Option<String>,
    pub reward_credit: Option<f64>,
    pub arrive_at: Option<i64>,
    pub daily_limit_reached: bool,
}

impl TravelStatus {
    fn plain(state: &str) -> Self {
        Self {
            state: state.to_string(),
            location_name: None,
            reward_credit: None,
            arrive_at: None,
            daily_limit_reached: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct TravelConfig {
    #[serde(default)]
    enabled: bool,
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

fn ensure_supported(scope: Scope) -> Result<(), String> {
    if scope.platform != Platform::Workbuddy || scope.region != Region::Cn {
        return Err("WorkBuddy 旅行仅国内版支持".to_string());
    }
    Ok(())
}

fn config_path(scope: Scope) -> PathBuf {
    store_dir().join(format!("travel-{}.json", scope.as_str()))
}

pub fn auto_enabled(scope: Scope) -> bool {
    std::fs::read_to_string(config_path(scope))
        .ok()
        .and_then(|text| serde_json::from_str::<TravelConfig>(&text).ok())
        .map(|config| config.enabled)
        .unwrap_or(false)
}

pub fn set_auto(scope: Scope, enabled: bool) -> Result<bool, String> {
    ensure_supported(scope)?;
    let content = serde_json::to_string_pretty(&TravelConfig { enabled })
        .map_err(|error| format!("序列化旅行配置失败：{error}"))?;
    atomic_write(&config_path(scope), &content).map_err(|error| format!("保存旅行配置失败：{error}"))?;
    Ok(enabled)
}

fn headers(scope: Scope, account: &Account) -> Vec<(&'static str, String)> {
    let base = scope.billing_base();
    let mut headers = checkin_workbuddy::auth_headers(account);
    headers.push(("x-client-platform", "web".to_string()));
    headers.push(("origin", base.to_string()));
    headers.push(("referer", format!("{base}/profile/growth-center")));
    if let Some(enterprise) = account
        .profile_raw
        .get("enterpriseId")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
    {
        headers.push(("X-Enterprise-Id", enterprise.to_string()));
        headers.push(("X-Tenant-Id", enterprise.to_string()));
    }
    headers
}

async fn call(
    scope: Scope,
    account: &Account,
    path: &str,
    method: reqwest::Method,
    body: Option<Value>,
) -> Result<Value, String> {
    let url = format!("{}{TRAVEL_PREFIX}{path}", scope.billing_base());
    let mut request = client().request(method, &url);
    for (name, value) in headers(scope, account) {
        request = request.header(name, value);
    }
    if let Some(body) = body {
        request = request.json(&body);
    }
    let response = request.send().await.map_err(|error| format!("网络错误：{error}"))?;
    let status = response.status();
    let text = response.text().await.unwrap_or_default();
    serde_json::from_str(&text).map_err(|_| {
        let snippet: String = text.chars().take(200).collect();
        format!("HTTP {status}：{snippet}")
    })
}

async fn authed(
    scope: Scope,
    account_id: &str,
    path: &str,
    method: reqwest::Method,
    body: Option<Value>,
) -> Result<Value, String> {
    ensure_supported(scope)?;
    let mut account =
        account::find(scope.as_str(), account_id).ok_or_else(|| "账号不存在".to_string())?;
    let mut response = call(scope, &account, path, method.clone(), body.clone()).await?;
    if checkin_workbuddy::is_unauthorized(&response) && account.refresh_token.is_some() {
        account = checkin_workbuddy::refresh_account(scope, &account).await?;
        response = call(scope, &account, path, method, body).await?;
    }
    Ok(response)
}

fn code_of(value: &Value) -> i64 {
    value.get("code").and_then(Value::as_i64).unwrap_or(-1)
}

fn number_of(value: Option<&Value>) -> Option<f64> {
    let value = value?;
    value
        .as_f64()
        .or_else(|| value.as_i64().map(|number| number as f64))
        .or_else(|| value.as_str().and_then(|text| text.trim().parse().ok()))
}

fn timestamp_ms(value: Option<&Value>) -> Option<i64> {
    let raw = value.and_then(Value::as_i64)?;
    if raw <= 0 {
        return None;
    }
    Some(if raw > 1_000_000_000_000 { raw } else { raw * 1000 })
}

fn no_buddy(message: &str) -> bool {
    let lowered = message.to_lowercase();
    lowered.contains("no active buddy") || lowered.contains("buddy")
}

fn growth_unsupported(message: &str) -> bool {
    let lowered = message.to_lowercase();
    lowered.contains("only available for personal") || lowered.contains("growth system")
}

fn parse_status(response: &Value) -> Result<(TravelStatus, i64), String> {
    let code = code_of(response);
    if code != 0 && code != 200 {
        let message = checkin_workbuddy::response_message(response);
        if growth_unsupported(&message) {
            return Ok((TravelStatus::plain("unsupported"), 0));
        }
        if no_buddy(&message) {
            return Ok((TravelStatus::plain("no_buddy"), 0));
        }
        return Err(if message.is_empty() {
            format!("code={code}")
        } else {
            message
        });
    }
    let data = response.get("data").cloned().unwrap_or_else(|| json!({}));
    let raw_state = data
        .get("state")
        .and_then(Value::as_str)
        .map(|value| value.trim().to_ascii_lowercase())
        .unwrap_or_default();
    if !matches!(raw_state.as_str(), "idle" | "traveling" | "arrived") {
        return Err(if raw_state.is_empty() {
            "旅行状态缺失".to_string()
        } else {
            format!("未知旅行状态：{raw_state}")
        });
    }
    let location_name = data
        .get("location")
        .and_then(|location| location.get("name"))
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let record_id = data.get("record_id").and_then(Value::as_i64).unwrap_or(0);
    Ok((
        TravelStatus {
            state: raw_state,
            location_name,
            reward_credit: number_of(data.get("reward_credit")),
            arrive_at: timestamp_ms(data.get("arrive_at")),
            daily_limit_reached: data
                .get("daily_limit_reached")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        },
        record_id,
    ))
}

async fn fetch_status(scope: Scope, account_id: &str) -> Result<(TravelStatus, i64), String> {
    let response = authed(
        scope,
        account_id,
        "/status",
        reqwest::Method::GET,
        None,
    )
    .await?;
    parse_status(&response)
}

pub async fn status(scope: Scope, account_id: &str) -> Result<TravelStatus, String> {
    fetch_status(scope, account_id)
        .await
        .map(|(status, _)| status)
}

pub async fn depart(scope: Scope, account_id: &str) -> Result<TravelStatus, String> {
    ensure_supported(scope)?;
    let config = authed(
        scope,
        account_id,
        "/config",
        reqwest::Method::GET,
        None,
    )
    .await?;
    let code = code_of(&config);
    if code != 0 && code != 200 {
        let message = checkin_workbuddy::response_message(&config);
        return Err(if message.is_empty() {
            format!("读取旅行配置失败（code={code}）")
        } else {
            message
        });
    }
    let locations = config
        .get("data")
        .and_then(|data| data.get("locations"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if locations.is_empty() {
        return Err("当前没有可用的旅行地点".to_string());
    }

    let mut unavailable = false;
    for location in locations {
        let location_id = location.get("id").cloned().unwrap_or(Value::Null);
        let name = location
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let response = authed(
            scope,
            account_id,
            "/depart",
            reqwest::Method::POST,
            Some(json!({ "location_id": location_id })),
        )
        .await?;
        let code = code_of(&response);
        if code == 0 || code == 200 {
            let mut current = status(scope, account_id)
                .await
                .unwrap_or_else(|_| TravelStatus::plain("traveling"));
            if current.location_name.is_none() && !name.is_empty() {
                current.location_name = Some(name);
            }
            return Ok(current);
        }
        let raw = checkin_workbuddy::response_message(&response);
        let lowered = raw.to_lowercase();
        if lowered.contains("already traveling") {
            return status(scope, account_id).await;
        }
        if lowered.contains("daily limit") || lowered.contains("daily_limit") {
            return status(scope, account_id).await;
        }
        if growth_unsupported(&raw) {
            return Ok(TravelStatus::plain("unsupported"));
        }
        if no_buddy(&raw) {
            return Ok(TravelStatus::plain("no_buddy"));
        }
        if lowered.contains("location not available") {
            unavailable = true;
            continue;
        }
        return Err(if raw.is_empty() {
            format!("派发失败（code={code}）")
        } else {
            raw
        });
    }

    if unavailable {
        Err("所有旅行地点都不可用".to_string())
    } else {
        Err("派发失败".to_string())
    }
}

pub async fn claim(scope: Scope, account_id: &str) -> Result<TravelStatus, String> {
    ensure_supported(scope)?;
    let (current, record_id) = fetch_status(scope, account_id).await?;
    if current.state != "arrived" {
        return Err("当前没有可领取的旅行奖励".to_string());
    }
    let body = if record_id > 0 {
        json!({ "record_id": record_id })
    } else {
        json!({})
    };
    let response = authed(
        scope,
        account_id,
        "/claim",
        reqwest::Method::POST,
        Some(body),
    )
    .await?;
    let code = code_of(&response);
    if code == 0 || code == 200 {
        let data = response.get("data").cloned().unwrap_or_else(|| json!({}));
        let mut next = current;
        next.state = "idle".to_string();
        next.arrive_at = None;
        next.location_name = None;
        if let Some(reward) = number_of(data.get("reward_credit")) {
            next.reward_credit = Some(reward);
        }
        return Ok(next);
    }
    let raw = checkin_workbuddy::response_message(&response);
    let lowered = raw.to_lowercase();
    if lowered.contains("no unclaimed") || lowered.contains("daily_limit") {
        return status(scope, account_id).await;
    }
    Err(if raw.is_empty() {
        format!("领取失败（code={code}）")
    } else {
        raw
    })
}

pub async fn run_auto_cycle() {
    let scope = Scope {
        platform: Platform::Workbuddy,
        region: Region::Cn,
    };
    if !auto_enabled(scope) {
        return;
    }
    for account in account::load(scope.as_str()) {
        let Ok(current) = status(scope, &account.id).await else {
            continue;
        };
        match current.state.as_str() {
            "arrived" => {
                let _ = claim(scope, &account.id).await;
            }
            "idle" if !current.daily_limit_reached => {
                let _ = depart(scope, &account.id).await;
            }
            _ => {}
        }
    }
}

pub fn spawn() {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_secs(20)).await;
        loop {
            run_auto_cycle().await;
            tokio::time::sleep(Duration::from_secs(15 * 60)).await;
        }
    });
}

