use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use serde_json::{json, Value};

use crate::account::{self, Account, AccountMeta};
use crate::login::LoginStart;
use crate::scope::{Region, Scope};
use crate::store::now_ms;

const API_PREFIX: &str = "/v2/plugin";
const OAUTH_TIMEOUT_SECONDS: i64 = 600;

struct OAuthInfo {
    region: Region,
    state: String,
    expires_at: i64,
    done: bool,
    result: Option<AccountMeta>,
    error: Option<String>,
}

static OAUTH_STATES: OnceLock<Mutex<HashMap<String, OAuthInfo>>> = OnceLock::new();

fn states() -> &'static Mutex<HashMap<String, OAuthInfo>> {
    OAUTH_STATES.get_or_init(|| Mutex::new(HashMap::new()))
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

async fn request(
    method: &str,
    url: &str,
    body: Option<Value>,
    headers: &[(&'static str, String)],
) -> Result<Value, String> {
    let method = reqwest::Method::from_bytes(method.as_bytes())
        .map_err(|error| format!("非法 HTTP 方法：{error}"))?;
    let mut builder = client()
        .request(method, url)
        .header("Content-Type", "application/json");
    for (name, value) in headers {
        builder = builder.header(*name, value);
    }
    if let Some(body) = body {
        builder = builder.json(&body);
    }
    let response = builder
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

fn now_secs() -> i64 {
    now_ms() / 1000
}

fn norm_ts(value: Option<&Value>) -> Option<i64> {
    let mut value = match value {
        Some(Value::String(text)) => text.trim().parse::<f64>().ok()? as i64,
        Some(Value::Number(number)) => number
            .as_i64()
            .or_else(|| number.as_f64().map(|value| value as i64))?,
        _ => return None,
    };
    if value < 10_000_000_000 {
        value *= 1000;
    }
    Some(value)
}

pub async fn start(scope: Scope) -> Result<LoginStart, String> {
    let base = scope.billing_base();
    let login_id = format!("wb_{}", uuid::Uuid::new_v4().simple());
    let url = format!("{base}{API_PREFIX}/auth/state?platform=workbuddy");
    let response = request("POST", &url, Some(json!({})), &[]).await?;
    let data = response
        .get("data")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let state = data
        .get("state")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string();
    if state.is_empty() {
        let snippet: String = response.to_string().chars().take(300).collect();
        return Err(format!("auth/state 响应缺少 state：{snippet}"));
    }
    let verification_uri = data
        .get("authUrl")
        .or_else(|| data.get("auth_url"))
        .or_else(|| data.get("url"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| format!("{base}/login?state={state}"));

    states().lock().unwrap().insert(
        login_id.clone(),
        OAuthInfo {
            region: scope.region,
            state,
            expires_at: now_secs() + OAUTH_TIMEOUT_SECONDS,
            done: false,
            result: None,
            error: None,
        },
    );

    Ok(LoginStart {
        login_id,
        verification_uri,
        expires_in: OAUTH_TIMEOUT_SECONDS,
    })
}

pub async fn poll(scope: Scope, login_id: &str) -> Value {
    let base = scope.billing_base();
    let state = {
        let mut map = states().lock().unwrap();
        let Some(info) = map.get_mut(login_id) else {
            return json!({"done": true, "error": "登录请求不存在"});
        };
        if info.region != scope.region {
            return json!({"done": true, "error": "登录请求与目标版本不匹配"});
        }
        if info.done {
            return json!({"done": true, "result": info.result, "error": info.error});
        }
        if now_secs() > info.expires_at {
            info.done = true;
            info.error = Some("登录超时".to_string());
            return json!({"done": true, "error": "登录超时"});
        }
        info.state.clone()
    };

    let url = format!("{base}{API_PREFIX}/auth/token?state={state}");
    let response = match request("GET", &url, None, &[]).await {
        Ok(value) => value,
        Err(_) => return json!({"done": false}),
    };
    let code = response.get("code").and_then(Value::as_i64).unwrap_or(-1);
    if code != 0 && code != 200 {
        return json!({"done": false});
    }
    let data = response
        .get("data")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let access_token = data
        .get("accessToken")
        .and_then(Value::as_str)
        .or_else(|| data.get("access_token").and_then(Value::as_str))
        .unwrap_or("")
        .to_string();
    if access_token.is_empty() {
        return json!({"done": false});
    }

    let account_url = format!("{base}{API_PREFIX}/login/account?state={state}");
    let domain = data
        .get("domain")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let mut headers: Vec<(&'static str, String)> =
        vec![("Authorization", format!("Bearer {access_token}"))];
    if !domain.is_empty() {
        headers.push(("X-Domain", domain.clone()));
    }
    let profile = request("GET", &account_url, None, &headers)
        .await
        .unwrap_or(Value::Null);
    let profile_data = profile
        .get("data")
        .cloned()
        .unwrap_or_else(|| json!({}));

    let expires_at = norm_ts(data.get("expiresAt").or_else(|| data.get("expires_at"))).or_else(|| {
        data.get("expiresIn")
            .and_then(Value::as_i64)
            .map(|seconds| now_ms() + seconds * 1000)
    });
    let refresh_expires_at = norm_ts(
        data.get("refreshExpiresAt")
            .or_else(|| data.get("refresh_expires_at")),
    )
    .or_else(|| {
        data.get("refreshExpiresIn")
            .and_then(Value::as_i64)
            .map(|seconds| now_ms() + seconds * 1000)
    });

    let uid = profile_data
        .get("uid")
        .and_then(Value::as_str)
        .map(str::to_string);
    let email = profile_data
        .get("email")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    if uid.is_none() && email.is_none() {
        let message = "登录成功但未能获取账号信息（uid / email 均为空）".to_string();
        let mut map = states().lock().unwrap();
        if let Some(info) = map.get_mut(login_id) {
            info.done = true;
            info.error = Some(message.clone());
        }
        return json!({"done": true, "error": message});
    }

    let account = Account {
        id: uuid::Uuid::new_v4().to_string(),
        uid,
        email,
        nickname: profile_data
            .get("nickname")
            .and_then(Value::as_str)
            .map(str::to_string),
        enterprise_name: profile_data
            .get("enterpriseName")
            .and_then(Value::as_str)
            .map(str::to_string),
        access_token,
        refresh_token: data
            .get("refreshToken")
            .and_then(Value::as_str)
            .or_else(|| data.get("refresh_token").and_then(Value::as_str))
            .map(str::to_string),
        token_type: data
            .get("tokenType")
            .and_then(Value::as_str)
            .unwrap_or("Bearer")
            .to_string(),
        domain,
        expires_at,
        refresh_expires_at,
        created_at: now_ms(),
        updated_at: now_ms(),
        auth_raw: data,
        profile_raw: profile_data,
    };

    let saved = match account::upsert(scope.as_str(), account) {
        Ok(saved) => saved,
        Err(error) => {
            let message = format!("保存账号失败：{error}");
            let mut map = states().lock().unwrap();
            if let Some(info) = map.get_mut(login_id) {
                info.done = true;
                info.error = Some(message.clone());
            }
            return json!({"done": true, "error": message});
        }
    };

    let meta = saved.meta();
    let mut map = states().lock().unwrap();
    if let Some(info) = map.get_mut(login_id) {
        info.done = true;
        info.result = Some(meta.clone());
    }
    drop(map);

    json!({"done": true, "result": meta})
}
