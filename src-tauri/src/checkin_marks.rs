use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::Mutex;

use chrono::{Local, TimeZone};
use serde::{Deserialize, Serialize};

use crate::checkin_workbuddy::{CheckinOutcome, CheckinStatus};
use crate::scope::Scope;
use crate::store::{atomic_write, read_secure_json, store_dir};

const SCOPES: [&str; 6] = [
    "workbuddy-cn",
    "workbuddy-global",
    "trae-cn",
    "trae-global",
    "qoder-cn",
    "qoder-global",
];

#[derive(Debug, Default, Serialize, Deserialize)]
struct MarksFile {
    #[serde(default)]
    date: String,
    #[serde(default)]
    marks: BTreeMap<String, BTreeSet<String>>,
}

// 手动签到、自动排程和状态查询都会读写这份标记，写入串行避免互相覆盖。
static LOCK: Mutex<()> = Mutex::new(());

fn file() -> PathBuf {
    store_dir().join("checkin-marks.json")
}

fn today() -> String {
    Local::now().format("%Y-%m-%d").to_string()
}

fn write_marks(data: &MarksFile) {
    if let Ok(text) = serde_json::to_string_pretty(data) {
        let _ = atomic_write(&file(), &text);
    }
}

// 升级后第一次使用时，用当天已有的成功签到日志补齐标记，避免重复请求。
fn seed_from_logs() -> BTreeMap<String, BTreeSet<String>> {
    let today = Local::now().date_naive();
    let mut marks: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for scope in SCOPES {
        let path = store_dir().join(format!("checkin-logs.{scope}.json"));
        let Some(logs) = read_secure_json::<Vec<serde_json::Value>>(&path) else {
            continue;
        };
        for entry in logs {
            if entry.get("result").and_then(|value| value.as_str()) != Some("success") {
                continue;
            }
            let Some(timestamp) = entry.get("ts").and_then(|value| value.as_i64()) else {
                continue;
            };
            let Some(time) = Local.timestamp_millis_opt(timestamp).single() else {
                continue;
            };
            if time.date_naive() != today {
                continue;
            }
            let Some(account_id) = entry.get("accountId").and_then(|value| value.as_str()) else {
                continue;
            };
            marks
                .entry(scope.to_string())
                .or_default()
                .insert(account_id.to_string());
        }
    }
    marks
}

/// 读取当天标记；跨天时先重置，并从当天签到日志恢复。
fn current() -> MarksFile {
    let data = load();
    if data.date == today() {
        return data;
    }
    let data = MarksFile {
        date: today(),
        marks: seed_from_logs(),
    };
    write_marks(&data);
    data
}

fn load() -> MarksFile {
    let Ok(text) = std::fs::read_to_string(file()) else {
        return MarksFile::default();
    };
    serde_json::from_str(&text).unwrap_or_default()
}

pub fn mark_signed(scope: Scope, account_id: &str) {
    let _guard = LOCK.lock().unwrap_or_else(|error| error.into_inner());
    let mut data = current();
    data.marks
        .entry(scope.as_str().to_string())
        .or_default()
        .insert(account_id.to_string());
    write_marks(&data);
}

pub fn signed_today(scope: Scope, account_id: &str) -> bool {
    let _guard = LOCK.lock().unwrap_or_else(|error| error.into_inner());
    let data = current();
    data.marks
        .get(scope.as_str())
        .is_some_and(|marks| marks.contains(account_id))
}

pub fn cached_status(scope: Scope, account_id: &str) -> Option<CheckinStatus> {
    signed_today(scope, account_id).then(|| CheckinStatus {
        ok: true,
        today_checked_in: true,
        daily_credit: None,
        today_credit: None,
        streak_days: None,
        error: None,
    })
}

pub fn skipped_outcome(scope: Scope, account_id: &str) -> Option<CheckinOutcome> {
    signed_today(scope, account_id).then(|| CheckinOutcome {
        account_id: account_id.to_string(),
        result: "already".to_string(),
        amount: None,
        error: None,
    })
}
