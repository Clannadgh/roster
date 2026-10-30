use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::store::{accounts_file, now_ms, read_secure_json, write_secure_json};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Account {
    pub id: String,
    #[serde(default)]
    pub uid: Option<String>,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub nickname: Option<String>,
    #[serde(default)]
    pub enterprise_name: Option<String>,
    #[serde(default)]
    pub access_token: String,
    #[serde(default)]
    pub refresh_token: Option<String>,
    #[serde(default = "default_token_type")]
    pub token_type: String,
    #[serde(default)]
    pub domain: String,
    #[serde(default)]
    pub expires_at: Option<i64>,
    #[serde(default)]
    pub refresh_expires_at: Option<i64>,
    #[serde(default)]
    pub created_at: i64,
    #[serde(default)]
    pub updated_at: i64,
    #[serde(default)]
    pub auth_raw: Value,
    #[serde(default)]
    pub profile_raw: Value,
}

fn default_token_type() -> String {
    "Bearer".to_string()
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountMeta {
    pub id: String,
    pub uid: Option<String>,
    pub email: Option<String>,
    pub nickname: Option<String>,
    pub enterprise_name: Option<String>,
    pub domain: String,
    pub expires_at: Option<i64>,
    pub refresh_expires_at: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
    pub needs_relogin: bool,
    pub is_current: bool,
}

impl Account {
    pub fn meta(&self) -> AccountMeta {
        AccountMeta {
            id: self.id.clone(),
            uid: self.uid.clone(),
            email: self.email.clone(),
            nickname: self.nickname.clone(),
            enterprise_name: self.enterprise_name.clone(),
            domain: self.domain.clone(),
            expires_at: self.expires_at,
            refresh_expires_at: self.refresh_expires_at,
            created_at: self.created_at,
            updated_at: self.updated_at,
            needs_relogin: self
                .expires_at
                .map(|expires| expires <= now_ms())
                .unwrap_or(false),
            is_current: false,
        }
    }
}

pub fn load(scope: &str) -> Vec<Account> {
    read_secure_json(&accounts_file(scope)).unwrap_or_default()
}

pub fn list_meta(scope: &str) -> Vec<AccountMeta> {
    load(scope).iter().map(Account::meta).collect()
}

pub fn find(scope: &str, id: &str) -> Option<Account> {
    load(scope)
        .into_iter()
        .find(|account| account.id == id || account.uid.as_deref() == Some(id))
}

pub fn save(scope: &str, accounts: &[Account]) -> Result<(), String> {
    write_secure_json(&accounts_file(scope), &accounts)
        .map_err(|error| format!("保存账号失败：{error}"))
}

/// 按 uid（优先）或 email 合并：命中已有账号时保留原 id 与创建时间。
pub fn upsert(scope: &str, mut account: Account) -> Result<Account, String> {
    let mut accounts = load(scope);
    let matching = accounts.iter().position(|existing| match (&account.uid, &existing.uid) {
        (Some(left), Some(right)) => left == right,
        _ => account.email.is_some() && account.email == existing.email,
    });

    match matching {
        Some(index) => {
            account.id = accounts[index].id.clone();
            account.created_at = accounts[index].created_at;
            accounts[index] = account.clone();
        }
        None => accounts.push(account.clone()),
    }

    save(scope, &accounts)?;
    Ok(account)
}

pub fn delete(scope: &str, id: &str) -> Result<(), String> {
    let mut accounts = load(scope);
    let before = accounts.len();
    accounts.retain(|account| account.id != id && account.uid.as_deref() != Some(id));
    if accounts.len() == before {
        return Err("账号不存在".to_string());
    }
    save(scope, &accounts)
}
