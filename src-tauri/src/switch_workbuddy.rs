use std::path::PathBuf;

use serde_json::{json, Map, Value};

use crate::account::{self, Account};
use crate::scope::{Region, Scope};
use crate::store::{atomic_write, home_dir, now_ms, store_dir};

fn auth_filename(region: Region) -> &'static str {
    match region {
        Region::Cn => "workbuddy-desktop.info",
        Region::Global => "workbuddy-desktop-ai.info",
    }
}

fn auth_candidates(region: Region) -> Vec<PathBuf> {
    let home = home_dir();
    let filename = auth_filename(region);
    vec![
        home.join("AppData/Local/CodeBuddyExtension/Data/Public/auth")
            .join(filename),
        home.join("AppData/Roaming/CodeBuddyExtension/Data/Public/auth")
            .join(filename),
    ]
}

fn auth_file(region: Region) -> PathBuf {
    auth_candidates(region)
        .into_iter()
        .find(|path| path.exists())
        .unwrap_or_else(|| auth_candidates(region)[0].clone())
}

fn read_auth_file(region: Region) -> Option<Value> {
    let text = std::fs::read_to_string(auth_file(region)).ok()?;
    serde_json::from_str(&text).ok()
}

pub fn current_uid(region: Region) -> Option<String> {
    let root = read_auth_file(region)?;
    root.get("account")
        .and_then(|account| account.get("uid"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|uid| !uid.is_empty())
        .map(str::to_string)
}

fn region_of_domain(domain: &str) -> Region {
    let lowered = domain.trim().to_ascii_lowercase();
    if lowered == "workbuddy.ai" || lowered.ends_with(".workbuddy.ai") {
        Region::Global
    } else {
        Region::Cn
    }
}

fn client_process_name(region: Region) -> &'static str {
    match region {
        Region::Cn => "WorkBuddy.exe",
        Region::Global => "WorkBuddyAI.exe",
    }
}

#[cfg(windows)]
fn client_running(region: Region) -> bool {
    crate::process::image_running(client_process_name(region))
}

#[cfg(not(windows))]
fn client_running(_region: Region) -> bool {
    false
}

fn set_default(object: &mut Map<String, Value>, key: &str, value: Value) {
    object.entry(key.to_string()).or_insert(value);
}

fn text(value: Option<&Value>) -> String {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or("")
        .to_string()
}

fn build_account_obj(account: &Account) -> Value {
    let mut object: Map<String, Value> = match &account.profile_raw {
        Value::Object(map) => map.clone(),
        _ => Map::new(),
    };
    object.insert(
        "uid".to_string(),
        json!(account.uid.clone().unwrap_or_default()),
    );
    object.insert(
        "nickname".to_string(),
        json!(account.nickname.clone().unwrap_or_default()),
    );
    set_default(&mut object, "type", json!("personal"));
    set_default(&mut object, "accountType", json!(""));
    set_default(&mut object, "idp", json!(""));
    set_default(&mut object, "oneidAccountId", json!(""));
    set_default(&mut object, "areaInfoComplete", json!(false));
    set_default(&mut object, "isCurrentOneIdEnterprise", json!(false));
    set_default(&mut object, "isCurrentOneIdPersonal", json!(false));
    set_default(&mut object, "isFirstLogin", json!(false));
    set_default(&mut object, "isCreator", json!(false));
    set_default(&mut object, "isAdmin", json!(false));
    set_default(&mut object, "uin", json!(""));
    set_default(&mut object, "phoneNumber", json!(""));
    set_default(&mut object, "lastLogin", json!(true));
    set_default(&mut object, "pluginEnabled", json!(true));
    set_default(
        &mut object,
        "deployStatus",
        json!({"statusCode": 0, "statusMsg": "", "detailMsg": ""}),
    );
    set_default(
        &mut object,
        "sso",
        json!({"domain": "", "domainModifiedTimes": 0}),
    );
    Value::Object(object)
}

fn build_auth_obj(account: &Account) -> Value {
    let mut object: Map<String, Value> = Map::new();
    if let Value::Object(raw) = &account.auth_raw {
        let inner = match raw.get("auth") {
            Some(Value::Object(auth)) => auth.clone(),
            _ => raw.clone(),
        };
        object.extend(inner);
    }

    let now = now_ms();
    object.insert("accessToken".to_string(), json!(account.access_token));
    object.insert(
        "refreshToken".to_string(),
        json!(account.refresh_token.clone().unwrap_or_default()),
    );
    object.insert("tokenType".to_string(), json!(account.token_type));
    object.insert("domain".to_string(), json!(account.domain));
    object.insert("lastRefreshTime".to_string(), json!(now));
    set_default(
        &mut object,
        "scope",
        json!("openid profile offline_access email"),
    );

    if let Some(expires_at) = account.expires_at {
        object.insert("expiresAt".to_string(), json!(expires_at));
        object.insert(
            "expiresIn".to_string(),
            json!(((expires_at - now) / 1000).max(0)),
        );
        let refresh_expires = account.refresh_expires_at.unwrap_or(expires_at);
        if !object.contains_key("refreshExpiresAt") {
            object.insert("refreshExpiresAt".to_string(), json!(refresh_expires));
        }
        object.insert(
            "refreshExpiresIn".to_string(),
            json!(((refresh_expires - now) / 1000).max(0)),
        );
    } else {
        set_default(&mut object, "expiresIn", json!(0));
        set_default(&mut object, "refreshExpiresIn", json!(0));
    }
    set_default(&mut object, "notBeforePolicy", json!(0));
    set_default(&mut object, "sessionState", json!(""));
    Value::Object(object)
}

fn backup_auth_file(region: Region, scope: Scope) -> Result<(), String> {
    let source = auth_file(region);
    if !source.exists() {
        return Ok(());
    }
    let dir = store_dir().join("backups").join(scope.as_str());
    std::fs::create_dir_all(&dir).map_err(|error| format!("创建备份目录失败：{error}"))?;
    let target = dir.join(format!("auth-{}.json", now_ms()));
    std::fs::copy(&source, &target).map_err(|error| format!("备份认证文件失败：{error}"))?;
    Ok(())
}

pub fn switch(scope: Scope, account_id: &str) -> Result<(), String> {
    if scope.platform != crate::scope::Platform::Workbuddy {
        return Err("目前只支持 WorkBuddy 桌面端切换".to_string());
    }
    let account = account::find(scope.as_str(), account_id).ok_or_else(|| "账号不存在".to_string())?;
    let domain_region = region_of_domain(&account.domain);
    if domain_region != scope.region {
        return Err("账号所属区域与当前作用域不一致，已拒绝写入".to_string());
    }
    if client_running(scope.region) {
        return Err(format!(
            "{} 正在运行，请先退出客户端后再切换",
            client_process_name(scope.region)
        ));
    }

    let path = auth_file(scope.region);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| format!("创建认证目录失败：{error}"))?;
    }
    let existing = read_auth_file(scope.region).unwrap_or_else(|| json!({}));
    let existing_accounts = existing
        .get("allAccounts")
        .cloned()
        .or_else(|| existing.get("accounts").cloned())
        .filter(Value::is_array)
        .unwrap_or_else(|| json!([]));

    let account_obj = build_account_obj(&account);
    let auth_obj = build_auth_obj(&account);
    let target_uid = account.uid.clone().unwrap_or_default();

    let mut all: Vec<Value> = existing_accounts.as_array().cloned().unwrap_or_default();
    all.retain(|entry| text(entry.get("uid")) != target_uid);
    all.push(account_obj.clone());

    let session = json!({
        "account": account_obj,
        "auth": auth_obj,
        "accounts": all,
        "allAccounts": all,
    });

    backup_auth_file(scope.region, scope)?;

    let content = serde_json::to_string_pretty(&session)
        .map_err(|error| format!("序列化认证文件失败：{error}"))?;
    atomic_write(&path, &content)
        .map_err(|error| format!("写入认证文件失败：{error}"))?;

    let written: Value = serde_json::from_str(
        &std::fs::read_to_string(&path).map_err(|error| format!("回读认证文件失败：{error}"))?,
    )
    .map_err(|error| format!("解析写后认证文件失败：{error}"))?;
    let written_token = written
        .get("auth")
        .and_then(|auth| auth.get("accessToken"))
        .cloned()
        .unwrap_or(Value::Null);
    let expected_token = Value::String(account.access_token.clone());
    if written_token != expected_token {
        return Err("认证文件写后校验失败，未切换到目标账号".to_string());
    }

    let mut updated = account.clone();
    updated.updated_at = now_ms();
    let _ = account::upsert(scope.as_str(), updated);
    Ok(())
}
