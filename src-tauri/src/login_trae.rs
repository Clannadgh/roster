use base64::Engine as _;
use serde_json::{json, Value};

use crate::account::{self, Account, AccountMeta};
use crate::scope::{Region, Scope};
use crate::store::now_ms;

pub async fn import_jwt(scope: Scope, raw: &str) -> Result<AccountMeta, String> {
    let trimmed = raw.trim();
    let token = trimmed
        .strip_prefix("Cloud-IDE-JWT ")
        .or_else(|| trimmed.strip_prefix("Bearer "))
        .unwrap_or(trimmed)
        .trim();
    if token.is_empty() {
        return Err("JWT 为空".to_string());
    }

    let payload = token
        .split('.')
        .nth(1)
        .ok_or_else(|| "不是合法的 JWT（缺少 payload 段）".to_string())?;
    let decoded = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload)
        .or_else(|_| base64::engine::general_purpose::URL_SAFE.decode(payload))
        .map_err(|_| "JWT payload 不是合法的 base64url".to_string())?;
    let data: Value =
        serde_json::from_slice(&decoded).map_err(|_| "JWT payload 不是 JSON".to_string())?;

    let uid = data
        .get("data")
        .and_then(|value| value.get("id"))
        .and_then(|value| {
            value
                .as_str()
                .map(str::to_string)
                .or_else(|| value.as_i64().map(|number| number.to_string()))
        })
        .or_else(|| data.get("auth_id").and_then(Value::as_str).map(str::to_string))
        .or_else(|| data.get("sub").and_then(Value::as_str).map(str::to_string))
        .ok_or_else(|| "JWT 里没有用户 ID（data.id / auth_id / sub）".to_string())?;

    let expires_at = data
        .get("exp")
        .and_then(Value::as_i64)
        .map(|seconds| seconds * 1000);
    let tail: String = uid
        .chars()
        .rev()
        .take(6)
        .collect::<String>()
        .chars()
        .rev()
        .collect();
    let nickname = format!("Trae {tail}");

    let account = Account {
        id: uuid::Uuid::new_v4().to_string(),
        uid: Some(uid),
        email: None,
        nickname: Some(nickname),
        enterprise_name: None,
        access_token: format!("Cloud-IDE-JWT {token}"),
        refresh_token: None,
        token_type: "Cloud-IDE-JWT".to_string(),
        domain: match scope.region {
            Region::Cn => "trae.com.cn",
            Region::Global => "trae.ai",
        }
        .to_string(),
        expires_at,
        refresh_expires_at: None,
        created_at: now_ms(),
        updated_at: now_ms(),
        auth_raw: json!({ "source": "jwt_paste" }),
        profile_raw: data,
    };

    crate::checkin_trae::validate_import(scope, &account).await?;
    account::upsert(scope.as_str(), account).map(|saved| saved.meta())
}

