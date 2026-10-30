use crate::account::{self, AccountMeta};
use crate::checkin_qoder;
use crate::checkin_trae;
use crate::checkin_workbuddy::{self, CheckinOutcome, CheckinStatus};
use crate::login::LoginStart;
use crate::login_qoder;
use crate::login_trae;
use crate::login_workbuddy;
use crate::scope::{Platform, Scope};

#[tauri::command]
pub fn list_accounts(scope: String) -> Result<Vec<AccountMeta>, String> {
    let scope = Scope::parse(&scope)?;
    let mut metas = account::list_meta(scope.as_str());
    if scope.platform == Platform::Workbuddy {
        if let Some(uid) = crate::switch_workbuddy::current_uid(scope.region) {
            for meta in &mut metas {
                meta.is_current = meta.uid.as_deref() == Some(uid.as_str()) || meta.id == uid;
            }
        }
    }
    Ok(metas)
}

#[tauri::command]
pub async fn login_start(scope: String) -> Result<LoginStart, String> {
    let scope = Scope::parse(&scope)?;
    match scope.platform {
        Platform::Workbuddy => login_workbuddy::start(scope).await,
        Platform::Qoder => login_qoder::start(scope).await,
        Platform::Trae => Err("Trae 请使用「粘贴 JWT」导入账号".to_string()),
    }
}

#[tauri::command]
pub async fn login_poll(scope: String, login_id: String) -> Result<serde_json::Value, String> {
    let scope = Scope::parse(&scope)?;
    match scope.platform {
        Platform::Workbuddy => Ok(login_workbuddy::poll(scope, &login_id).await),
        Platform::Qoder => Ok(login_qoder::poll(scope, &login_id).await),
        Platform::Trae => Err("Trae 请使用「粘贴 JWT」导入账号".to_string()),
    }
}

#[tauri::command]
pub fn import_trae_jwt(scope: String, jwt: String) -> Result<AccountMeta, String> {
    let scope = Scope::parse(&scope)?;
    if scope.platform != Platform::Trae {
        return Err("该命令仅用于 Trae".to_string());
    }
    login_trae::import_jwt(scope, &jwt)
}

#[tauri::command]
pub fn delete_account(scope: String, id: String) -> Result<(), String> {
    let scope = Scope::parse(&scope)?;
    account::delete(scope.as_str(), &id)
}

#[tauri::command]
pub fn switch_account(scope: String, account_id: String) -> Result<(), String> {
    let scope = Scope::parse(&scope)?;
    match scope.platform {
        Platform::Workbuddy => crate::switch_workbuddy::switch(scope, &account_id),
        Platform::Qoder => crate::qoder_switch::switch(scope, &account_id),
        Platform::Trae => crate::trae_switch::switch(scope, &account_id),
    }
}

#[tauri::command]
pub fn save_trae_profile(scope: String, account_id: String) -> Result<u64, String> {
    let scope = Scope::parse(&scope)?;
    crate::trae_switch::save_profile(scope, &account_id)
}

#[tauri::command]
pub fn get_schedule(scope: String) -> Result<crate::schedule::ScheduleConfig, String> {
    let scope = Scope::parse(&scope)?;
    Ok(crate::schedule::get(scope))
}

#[tauri::command]
pub fn save_schedule(
    scope: String,
    config: crate::schedule::ScheduleConfig,
) -> Result<crate::schedule::ScheduleConfig, String> {
    let scope = Scope::parse(&scope)?;
    crate::schedule::save(scope, config)
}

#[tauri::command]
pub fn get_autostart(app: tauri::AppHandle) -> Result<bool, String> {
    use tauri_plugin_autostart::ManagerExt;
    app.autolaunch().is_enabled().map_err(|error| error.to_string())
}

#[tauri::command]
pub fn set_autostart(app: tauri::AppHandle, enabled: bool) -> Result<bool, String> {
    use tauri_plugin_autostart::ManagerExt;
    let manager = app.autolaunch();
    if enabled {
        manager.enable()
    } else {
        manager.disable()
    }
    .map_err(|error| error.to_string())?;
    manager.is_enabled().map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn checkin_status(scope: String, account_id: String) -> Result<CheckinStatus, String> {
    let scope = Scope::parse(&scope)?;
    match scope.platform {
        Platform::Workbuddy => checkin_workbuddy::status(scope, &account_id).await,
        Platform::Qoder => checkin_qoder::status(scope, &account_id).await,
        Platform::Trae => checkin_trae::status(scope, &account_id).await,
    }
}

#[tauri::command]
pub async fn checkin_account(scope: String, account_id: String) -> Result<CheckinOutcome, String> {
    let scope = Scope::parse(&scope)?;
    match scope.platform {
        Platform::Workbuddy => Ok(checkin_workbuddy::checkin(scope, &account_id).await),
        Platform::Qoder => Ok(checkin_qoder::checkin(scope, &account_id).await),
        Platform::Trae => Ok(checkin_trae::checkin(scope, &account_id).await),
    }
}

#[tauri::command]
pub async fn checkin_all(scope: String) -> Result<Vec<CheckinOutcome>, String> {
    let scope = Scope::parse(&scope)?;
    match scope.platform {
        Platform::Workbuddy => Ok(checkin_workbuddy::checkin_all(scope).await),
        Platform::Qoder => Ok(checkin_qoder::checkin_all(scope).await),
        Platform::Trae => Ok(checkin_trae::checkin_all(scope).await),
    }
}

#[tauri::command]
pub async fn get_credits_summary(scope: String) -> Result<crate::credits::CreditsSummary, String> {
    let scope = Scope::parse(&scope)?;
    crate::credits::summary(scope).await
}

