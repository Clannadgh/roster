use std::path::PathBuf;

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use base64::Engine as _;
use rand::RngCore;
use serde_json::{json, Value};

use crate::account::{self, Account};
use crate::scope::{Platform, Region, Scope};
use crate::store::{atomic_write_bytes, home_dir, now_ms, store_dir};

const V10_PREFIX: &[u8; 3] = b"v10";

fn roaming_dir() -> PathBuf {
    std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| home_dir().join("AppData/Roaming"))
}

fn app_dir(region: Region) -> PathBuf {
    roaming_dir().join(match region {
        Region::Cn => "com.qodercn.app.stable",
        Region::Global => "com.qoder.app.stable",
    })
}

fn process_name(region: Region) -> &'static str {
    match region {
        Region::Cn => "Qoder CN.exe",
        Region::Global => "Qoder.exe",
    }
}

fn encryption_key(region: Region) -> Result<Vec<u8>, String> {
    let local_state = app_dir(region).join("Local State");
    let text = std::fs::read_to_string(&local_state)
        .map_err(|error| format!("读取 {} 失败：{error}", local_state.display()))?;
    let json: Value =
        serde_json::from_str(&text).map_err(|error| format!("解析 Local State 失败：{error}"))?;
    let encrypted_key = json
        .get("os_crypt")
        .and_then(|value| value.get("encrypted_key"))
        .and_then(Value::as_str)
        .ok_or_else(|| "Local State 缺少 os_crypt.encrypted_key".to_string())?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(encrypted_key)
        .map_err(|error| format!("encrypted_key 不是合法 base64：{error}"))?;
    if bytes.len() < 6 || &bytes[..5] != b"DPAPI" {
        return Err("encrypted_key 前缀不是 DPAPI".to_string());
    }
    let key = crate::secrets::unprotect(&bytes[5..])?;
    if key.len() != 32 {
        return Err(format!("解密出的 AES 密钥长度异常：{}", key.len()));
    }
    Ok(key)
}

fn decrypt(key: &[u8], blob: &[u8]) -> Result<Vec<u8>, String> {
    if blob.len() < 31 || &blob[..3] != V10_PREFIX {
        return Err("登录态密文不是受支持的 v10 格式".to_string());
    }
    let cipher = Aes256Gcm::new_from_slice(key).map_err(|error| format!("初始化 AES 失败：{error}"))?;
    cipher
        .decrypt(Nonce::from_slice(&blob[3..15]), &blob[15..])
        .map_err(|_| "AES-GCM 解密失败".to_string())
}

fn encrypt(key: &[u8], plaintext: &[u8]) -> Result<Vec<u8>, String> {
    let cipher = Aes256Gcm::new_from_slice(key).map_err(|error| format!("初始化 AES 失败：{error}"))?;
    let mut nonce = [0u8; 12];
    rand::rngs::OsRng.fill_bytes(&mut nonce);
    let ciphertext = cipher
        .encrypt(Nonce::from_slice(&nonce), plaintext)
        .map_err(|_| "AES-GCM 加密失败".to_string())?;
    let mut output = Vec::with_capacity(3 + 12 + ciphertext.len());
    output.extend_from_slice(V10_PREFIX);
    output.extend_from_slice(&nonce);
    output.extend_from_slice(&ciphertext);
    Ok(output)
}

fn token_of(json: &Value) -> Option<String> {
    ["token", "accessToken", "access_token"]
        .iter()
        .find_map(|key| {
            json.get(*key)
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
        })
}

fn apply_overrides(json: &mut Value, account: &Account) -> Result<(), String> {
    let object = json
        .as_object_mut()
        .ok_or_else(|| "现有登录态不是 JSON 对象".to_string())?;
    object.insert("token".to_string(), json!(account.access_token));
    if let Some(refresh) = account.refresh_token.as_deref().filter(|value| !value.is_empty()) {
        object.insert("refreshToken".to_string(), json!(refresh));
    }
    if let Some(millis) = account.expires_at {
        if let Some(when) = chrono::DateTime::from_timestamp_millis(millis) {
            object.insert("expiresAt".to_string(), json!(when.to_rfc3339()));
        }
    }
    let user = object
        .entry("user".to_string())
        .or_insert_with(|| Value::Object(Default::default()));
    if let Some(user) = user.as_object_mut() {
        if let Some(uid) = account.uid.as_deref().filter(|value| !value.is_empty()) {
            user.insert("id".to_string(), json!(uid));
        }
        if let Some(name) = account.nickname.as_deref().filter(|value| !value.is_empty()) {
            user.insert("name".to_string(), json!(name));
        }
    }
    Ok(())
}

fn backup_auth(scope: Scope, source: &std::path::Path) -> Result<Option<PathBuf>, String> {
    if !source.exists() {
        return Ok(None);
    }
    let dir = store_dir().join("backups").join(scope.as_str());
    std::fs::create_dir_all(&dir).map_err(|error| format!("创建备份目录失败：{error}"))?;
    let target = dir.join(format!("qoder-auth-{}.dat", now_ms()));
    std::fs::copy(source, &target).map_err(|error| format!("备份 Qoder 登录态失败：{error}"))?;
    Ok(Some(target))
}

pub fn switch(scope: Scope, account_id: &str) -> Result<(), String> {
    if scope.platform != Platform::Qoder {
        return Err("该命令仅用于 Qoder".to_string());
    }
    let name = process_name(scope.region);
    if crate::process::image_running(name) {
        return Err(format!("{name} 正在运行，请先退出 Qoder 桌面版后再切换"));
    }
    let account =
        account::find(scope.as_str(), account_id).ok_or_else(|| "账号不存在".to_string())?;
    if account.access_token.trim().is_empty() {
        return Err("该账号没有访问令牌，无法写回登录态".to_string());
    }

    let auth_path = app_dir(scope.region).join("auth.v1.dat");
    if !auth_path.exists() {
        return Err(format!(
            "未找到 Qoder 桌面版登录态 {}。请先打开 Qoder 并登录一次，再切换账号。",
            auth_path.display()
        ));
    }
    let key = encryption_key(scope.region)?;
    let existing = std::fs::read(&auth_path).map_err(|error| format!("读取登录态失败：{error}"))?;
    let plaintext = decrypt(&key, &existing).map_err(|error| {
        format!("无法解密现有登录态：{error}（若刚升级过客户端，请先在客户端里重新登录一次）")
    })?;
    let mut json: Value =
        serde_json::from_slice(&plaintext).map_err(|error| format!("登录态不是 JSON：{error}"))?;
    apply_overrides(&mut json, &account)?;

    let new_plaintext =
        serde_json::to_vec(&json).map_err(|error| format!("序列化登录态失败：{error}"))?;
    let encrypted = encrypt(&key, &new_plaintext)?;
    let backup = backup_auth(scope, &auth_path)?;

    atomic_write_bytes(&auth_path, &encrypted).map_err(|error| format!("写入登录态失败：{error}"))?;

    let actual = std::fs::read(&auth_path)
        .map_err(|error| format!("写回后读取失败：{error}"))
        .and_then(|blob| decrypt(&key, &blob).map_err(|error| format!("写回后解密失败：{error}")))
        .and_then(|plain| {
            serde_json::from_slice::<Value>(&plain)
                .map_err(|error| format!("写回后不是 JSON：{error}"))
        })
        .and_then(|value| token_of(&value).ok_or_else(|| "写回后登录态缺少 token".to_string()));

    match actual {
        Ok(token) if token == account.access_token => Ok(()),
        other => {
            let reason = match other {
                Ok(token) => format!("写回后令牌与预期不一致（读到 {}…）", &token.chars().take(6).collect::<String>()),
                Err(error) => format!("写回校验失败：{error}"),
            };
            if let Some(backup) = backup {
                if let Ok(bytes) = std::fs::read(&backup) {
                    let _ = atomic_write_bytes(&auth_path, &bytes);
                    return Err(format!("{reason}，已还原为切换前的登录态"));
                }
            }
            Err(format!("{reason}，且没有可用备份"))
        }
    }
}
