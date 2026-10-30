use std::collections::HashMap;
use std::path::Path;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use base64::Engine as _;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::account::{self, Account, AccountMeta};
use crate::login::LoginStart;
use crate::scope::{Region, Scope};
use crate::store::{atomic_write, home_dir, now_ms, store_dir};

const OAUTH_TIMEOUT_SECONDS: i64 = 600;
const PATH_TOKEN_POLL: &str = "/api/v1/deviceToken/poll";
const PATH_USERINFO: &str = "/api/v1/userinfo";

struct Realm {
    website: &'static str,
    openapi_base: &'static str,
    client_id: &'static str,
    redirect_uri: Option<&'static str>,
    domain: &'static str,
    user_agent: &'static str,
    nonce_dashed: bool,
}

const CN: Realm = Realm {
    website: "https://qoder.com.cn",
    openapi_base: "https://openapi.qoder.com.cn",
    client_id: "1c5e33e1-364d-4ce6-b02c-acaa81274a5c",
    redirect_uri: Some("qoder-work-cn://"),
    domain: "qoder.com.cn",
    user_agent: "QoderWork/1.1.64",
    nonce_dashed: true,
};

const GLOBAL: Realm = Realm {
    website: "https://qoder.com",
    openapi_base: "https://openapi.qoder.sh",
    client_id: "e883ade2-e6e3-4d6d-adf7-f92ceff5fdcb",
    redirect_uri: None,
    domain: "qoder.com",
    user_agent: "Qoder/1.1.64",
    nonce_dashed: false,
};

fn realm(region: Region) -> &'static Realm {
    match region {
        Region::Cn => &CN,
        Region::Global => &GLOBAL,
    }
}

struct LoginSession {
    region: Region,
    verifier: String,
    nonce: String,
    expires_at: i64,
    done: bool,
    result: Option<AccountMeta>,
    error: Option<String>,
}

static SESSIONS: OnceLock<Mutex<HashMap<String, LoginSession>>> = OnceLock::new();

fn sessions() -> &'static Mutex<HashMap<String, LoginSession>> {
    SESSIONS.get_or_init(|| Mutex::new(HashMap::new()))
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

fn now_secs() -> i64 {
    now_ms() / 1000
}

fn read_trimmed(path: &Path) -> Option<String> {
    let text = std::fs::read_to_string(path).ok()?;
    let trimmed = text.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn machine_id(region: Region) -> Result<String, String> {
    let cli_home = home_dir().join(match region {
        Region::Cn => ".qoder-cn",
        Region::Global => ".qoder",
    });
    if let Some(value) = read_trimmed(&cli_home.join(".auth").join("machine_id")) {
        return Ok(value);
    }

    let store_path = store_dir().join(format!(
        "qoder-machine-id.{}.txt",
        match region {
            Region::Cn => "cn",
            Region::Global => "global",
        }
    ));
    if let Some(value) = read_trimmed(&store_path) {
        return Ok(value);
    }

    let generated = uuid::Uuid::new_v4().to_string();
    atomic_write(&store_path, &generated)
        .map_err(|error| format!("写入本机设备标识失败：{error}"))?;
    Ok(generated)
}

fn pkce_verifier() -> String {
    format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    )
}

fn pkce_challenge(verifier: &str) -> String {
    let digest = Sha256::digest(verifier.as_bytes());
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(digest)
}

pub async fn start(scope: Scope) -> Result<LoginStart, String> {
    let spec = realm(scope.region);
    let machine_id = machine_id(scope.region)?;
    let verifier = pkce_verifier();
    let challenge = pkce_challenge(&verifier);
    let nonce = if spec.nonce_dashed {
        uuid::Uuid::new_v4().to_string()
    } else {
        uuid::Uuid::new_v4().simple().to_string()
    };

    let mut verification_uri = format!(
        "{}/device/selectAccounts?challenge={challenge}&challenge_method=S256&nonce={nonce}&client_id={}&machine_id={machine_id}",
        spec.website, spec.client_id
    );
    if let Some(redirect_uri) = spec.redirect_uri {
        verification_uri.push_str(&format!("&redirect_uri={redirect_uri}"));
    }

    let login_id = format!("qoder_{}", uuid::Uuid::new_v4().simple());
    sessions().lock().unwrap().insert(
        login_id.clone(),
        LoginSession {
            region: scope.region,
            verifier,
            nonce,
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

fn finish(login_id: &str, result: Option<AccountMeta>, error: Option<String>) {
    let mut map = sessions().lock().unwrap();
    if let Some(session) = map.get_mut(login_id) {
        session.done = true;
        session.result = result;
        session.error = error;
    }
}

fn response_field<'a>(body: &'a Value, key: &str) -> Option<&'a str> {
    body.get(key).and_then(Value::as_str).or_else(|| {
        body.get("data")
            .and_then(|data| data.get(key))
            .and_then(Value::as_str)
    })
}

async fn fetch_userinfo(spec: &Realm, token: &str) -> Result<Value, String> {
    let response = client()
        .get(format!("{}{PATH_USERINFO}", spec.openapi_base))
        .header("Accept", "application/json, text/plain, */*")
        .header("User-Agent", spec.user_agent)
        .header("Authorization", format!("Bearer {token}"))
        .timeout(Duration::from_secs(15))
        .send()
        .await
        .map_err(|error| format!("拉取账号身份失败：{error}"))?;
    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    if !status.is_success() {
        let detail: String = body.chars().take(160).collect();
        return Err(format!(
            "拉取账号身份失败：HTTP {} {detail}",
            status.as_u16()
        ));
    }
    serde_json::from_str(&body).map_err(|error| format!("账号身份响应不是 JSON：{error}"))
}

fn expires_at_ms(data: &Value) -> Option<i64> {
    let expires_in = data.get("expires_in").and_then(|value| {
        value
            .as_i64()
            .or_else(|| value.as_str().and_then(|text| text.trim().parse().ok()))
    })?;
    Some(now_ms() + expires_in)
}

pub async fn poll(scope: Scope, login_id: &str) -> Value {
    let spec = realm(scope.region);

    let (nonce, verifier) = {
        let mut map = sessions().lock().unwrap();
        let Some(session) = map.get_mut(login_id) else {
            return json!({"done": true, "error": "登录请求不存在"});
        };
        if session.region != scope.region {
            return json!({"done": true, "error": "登录请求与目标版本不匹配"});
        }
        if session.done {
            return json!({"done": true, "result": session.result, "error": session.error});
        }
        if now_secs() > session.expires_at {
            session.done = true;
            session.error = Some("登录超时".to_string());
            return json!({"done": true, "error": "登录超时"});
        }
        (session.nonce.clone(), session.verifier.clone())
    };

    let url = format!(
        "{}{PATH_TOKEN_POLL}?nonce={nonce}&verifier={verifier}&challenge_method=S256",
        spec.openapi_base
    );
    let response = client()
        .get(&url)
        .header("Accept", "application/json, text/plain, */*")
        .header("User-Agent", spec.user_agent)
        .timeout(Duration::from_secs(15))
        .send()
        .await;
    let response = match response {
        Ok(response) => response,
        Err(_) => return json!({"done": false}),
    };
    let status = response.status();
    let body = response.text().await.unwrap_or_default();

    if matches!(status.as_u16(), 202 | 404) {
        return json!({"done": false});
    }
    if !status.is_success() {
        let detail: String = body.chars().take(160).collect();
        let error = format!("轮询失败：HTTP {} {detail}", status.as_u16());
        finish(login_id, None, Some(error.clone()));
        return json!({"done": true, "error": error});
    }
    let data: Value = match serde_json::from_str(&body) {
        Ok(data) => data,
        Err(error) => {
            let message = format!("轮询响应不是 JSON：{error}");
            finish(login_id, None, Some(message.clone()));
            return json!({"done": true, "error": message});
        }
    };

    let token = response_field(&data, "token")
        .or_else(|| response_field(&data, "device_token"))
        .unwrap_or("")
        .to_string();
    if token.is_empty() {
        return json!({"done": false});
    }
    let refresh_token = response_field(&data, "refresh_token")
        .filter(|value| !value.is_empty())
        .map(str::to_string);

    let userinfo = match fetch_userinfo(spec, &token).await {
        Ok(userinfo) => userinfo,
        Err(error) => {
            finish(login_id, None, Some(error.clone()));
            return json!({"done": true, "error": error});
        }
    };
    let uid = match userinfo.get("id") {
        Some(Value::String(text)) if !text.trim().is_empty() => text.clone(),
        Some(Value::Number(number)) => number.to_string(),
        _ => String::new(),
    };
    if uid.is_empty() {
        let error = "userinfo 缺少 id —— 已拒绝写入".to_string();
        finish(login_id, None, Some(error.clone()));
        return json!({"done": true, "error": error});
    }
    let nickname = userinfo
        .get("name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_string);

    let account = Account {
        id: uuid::Uuid::new_v4().to_string(),
        uid: Some(uid),
        email: None,
        nickname,
        enterprise_name: None,
        access_token: token,
        refresh_token,
        token_type: "Bearer".to_string(),
        domain: spec.domain.to_string(),
        expires_at: expires_at_ms(&data),
        refresh_expires_at: None,
        created_at: now_ms(),
        updated_at: now_ms(),
        auth_raw: data,
        profile_raw: userinfo,
    };

    let saved = match account::upsert(scope.as_str(), account) {
        Ok(saved) => saved,
        Err(error) => {
            let message = format!("保存账号失败：{error}");
            finish(login_id, None, Some(message.clone()));
            return json!({"done": true, "error": message});
        }
    };
    let meta = saved.meta();
    finish(login_id, Some(meta.clone()), None);
    json!({"done": true, "result": meta})
}
