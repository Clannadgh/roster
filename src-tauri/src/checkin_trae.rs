use std::sync::OnceLock;
use std::time::Duration;

use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::account::{self, Account};
use crate::checkin_workbuddy::{CheckinOutcome, CheckinStatus};
use crate::scope::{Region, Scope};

const PATH_STATUS: &str = "/trae/api/v2/ug/checkin_credits/status";
const PATH_CLAIM: &str = "/trae/api/v2/ug/checkin_credits/claim";
const APP_VERSION: &str = "0.1.45";
const VSCODE_VERSION: &str = "1.107.1";
const UA_PRODUCT: &str = "TRAE SOLO CN";
const MARKET_AID: &str = "787976";
const SDK_TYPE: &str = "3";
const PACKAGE_TYPE: &str = "stable_cn";
const USER_REGION: &str = "CN";

fn account_base(region: Region) -> &'static str {
    match region {
        Region::Cn => "https://api.trae.cn",
        Region::Global => "https://grow-normal.trae.ai",
    }
}

static HTTP_CLIENT: OnceLock<reqwest::Client> = OnceLock::new();

fn client() -> &'static reqwest::Client {
    HTTP_CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(60))
            .build()
            .expect("build http client")
    })
}

fn seeded_stream(seed: &str, salt: &str, nbytes: usize) -> Vec<u8> {
    let prefix = format!("{salt}:{seed}");
    let mut out: Vec<u8> = Vec::with_capacity(nbytes + 32);
    let mut counter: u32 = 0;
    while out.len() < nbytes {
        let mut hasher = Sha256::new();
        hasher.update(prefix.as_bytes());
        hasher.update(counter.to_be_bytes());
        out.extend_from_slice(&hasher.finalize());
        counter = counter.wrapping_add(1);
    }
    out.truncate(nbytes);
    out
}

fn device_id(uid: &str) -> String {
    seeded_stream(uid, "devid", 15)
        .iter()
        .map(|byte| char::from(b'0' + (byte % 10)))
        .collect()
}

fn session_id(uid: &str) -> String {
    let mut hex = String::with_capacity(64);
    for byte in seeded_stream(uid, "sess", 32) {
        hex.push_str(&format!("{byte:02x}"));
    }
    hex.truncate(64);
    hex
}

fn market_user_id(uid: &str) -> String {
    let mut bytes = [0u8; 16];
    bytes.copy_from_slice(&seeded_stream(uid, "market", 16));
    bytes[6] = (bytes[6] & 0x0F) | 0x40;
    bytes[8] = (bytes[8] & 0x3F) | 0x80;
    uuid::Uuid::from_bytes(bytes).to_string()
}

fn authorization(token: &str) -> String {
    let trimmed = token.trim();
    if trimmed.starts_with("Cloud-IDE-JWT ") {
        trimmed.to_string()
    } else {
        format!("Cloud-IDE-JWT {}", trimmed.strip_prefix("Bearer ").unwrap_or(trimmed).trim())
    }
}

pub(crate) fn headers(account: &Account) -> Vec<(&'static str, String)> {
    let uid = account.uid.as_deref().unwrap_or("");
    let trace = uuid::Uuid::new_v4().simple().to_string();
    vec![
        ("accept", "*/*".to_string()),
        ("accept-language", "zh-CN".to_string()),
        ("authorization", authorization(&account.access_token)),
        ("content-type", "application/json".to_string()),
        ("user-agent", format!("VSCode {VSCODE_VERSION} ({UA_PRODUCT})")),
        ("x-market-client-id", format!("VSCode {VSCODE_VERSION}")),
        ("x-market-user-id", market_user_id(uid)),
        ("x-user-region", USER_REGION.to_string()),
        ("x-device-id", device_id(uid)),
        ("x-lgw-req-sdk-type", SDK_TYPE.to_string()),
        ("package-type", PACKAGE_TYPE.to_string()),
        ("x-request-id", uuid::Uuid::new_v4().to_string()),
        ("x-lscbd-aid", MARKET_AID.to_string()),
        (
            "x-lscbd-platform",
            if cfg!(target_os = "windows") {
                "windows".to_string()
            } else if cfg!(target_os = "macos") {
                "macos".to_string()
            } else {
                "linux".to_string()
            },
        ),
        ("app-version", APP_VERSION.to_string()),
        ("x-tt-trace-id", format!("00-{}-01", &trace[..16])),
        ("vscode-sessionid", session_id(uid)),
        ("sec-fetch-dest", "empty".to_string()),
        ("sec-fetch-mode", "no-cors".to_string()),
        ("sec-fetch-site", "none".to_string()),
    ]
}

pub(crate) async fn post(
    scope: Scope,
    path: &str,
    account: &Account,
) -> Result<(u16, String), String> {
    let url = format!("{}{path}", account_base(scope.region));
    let mut request = client()
        .post(&url)
        .header("content-type", "application/json")
        .body("{}");
    for (name, value) in headers(account) {
        if let Ok(value) = reqwest::header::HeaderValue::from_str(&value) {
            request = request.header(name, value);
        }
    }
    let response = request.send().await.map_err(|error| format!("网络错误：{error}"))?;
    let status = response.status().as_u16();
    let body = response.text().await.unwrap_or_default();
    Ok((status, body))
}

pub(crate) fn parse_json(status: u16, body: &str) -> Result<Value, String> {
    serde_json::from_str(body).map_err(|_| {
        if body.trim_start().starts_with('<') {
            return format!("HTTP {status}：接口返回了网页而不是数据，通常说明国内/国际版选错了");
        }
        let snippet: String = body.chars().take(200).collect();
        format!("HTTP {status}：{snippet}")
    })
}

/// 导入前校验 JWT：当前区域能通过状态接口才允许保存账号。
pub(crate) async fn validate_import(scope: Scope, account: &Account) -> Result<(), String> {
    match probe(scope, account).await {
        Ok(_) => Ok(()),
        Err(error) => {
            let other_region = match scope.region {
                Region::Cn => Region::Global,
                Region::Global => Region::Cn,
            };
            let other = Scope {
                platform: scope.platform,
                region: other_region,
            };
            if probe(other, account).await.is_ok() {
                let label = if other_region == Region::Cn { "国内版" } else { "国际版" };
                return Err(format!("这个 JWT 属于 Trae {label}，请切换到「{label}」后再导入"));
            }
            Err(format!("JWT 未通过 Trae 服务端校验：{error}"))
        }
    }
}

struct Probe {
    checked_in: bool,
    credits: Option<f64>,
}

async fn probe(scope: Scope, account: &Account) -> Result<Probe, String> {
    let (status, body) = post(scope, PATH_STATUS, account).await?;
    let payload = parse_json(status, &body)?;
    let code = payload.get("code").and_then(Value::as_i64).unwrap_or(-1);
    if code != 0 {
        let message = payload
            .get("message")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| format!("HTTP {status}"));
        return Err(message);
    }
    Ok(Probe {
        checked_in: payload
            .get("checked_in")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        credits: payload
            .get("credits")
            .and_then(|value| value.as_f64().or_else(|| value.as_i64().map(|n| n as f64))),
    })
}

pub async fn status(scope: Scope, account_id: &str) -> Result<CheckinStatus, String> {
    let account = account::find(scope.as_str(), account_id)
        .ok_or_else(|| "账号不存在".to_string())?;
    if let Some(status) = crate::checkin_marks::cached_status(scope, account_id) {
        return Ok(status);
    }
    let probe = probe(scope, &account).await?;
    if probe.checked_in {
        crate::checkin_marks::mark_signed(scope, account_id);
    }
    Ok(CheckinStatus {
        ok: true,
        today_checked_in: probe.checked_in,
        daily_credit: if probe.checked_in { None } else { probe.credits },
        today_credit: if probe.checked_in { probe.credits } else { None },
        streak_days: None,
        error: None,
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

    let probe = match probe(scope, &account).await {
        Ok(probe) => probe,
        Err(error) => return outcome("error", None, Some(error)),
    };
    if probe.checked_in {
        crate::checkin_marks::mark_signed(scope, account_id);
        return outcome("already", None, None);
    }

    let (status_code, body) = match post(scope, PATH_CLAIM, &account).await {
        Ok(value) => value,
        Err(error) => return outcome("error", None, Some(error)),
    };
    let payload = match parse_json(status_code, &body) {
        Ok(payload) => payload,
        Err(error) => return outcome("error", None, Some(error)),
    };
    let code = payload.get("code").and_then(Value::as_i64).unwrap_or(-1);
    if code != 0 {
        let message = payload
            .get("message")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| format!("HTTP {status_code}"));
        let outcome = outcome("error", None, Some(message));
        crate::checkin_workbuddy::append_log(scope, &account, &outcome);
        return outcome;
    }

    crate::checkin_marks::mark_signed(scope, account_id);
    let outcome = outcome("success", probe.credits, None);
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

