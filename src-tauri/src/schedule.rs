use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Mutex;

use chrono::{Local, Timelike};
use serde::{Deserialize, Serialize};

use crate::checkin_qoder;
use crate::checkin_trae;
use crate::checkin_workbuddy;
use crate::scope::{Platform, Region, Scope};
use crate::store::{atomic_write, now_ms, store_dir};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScheduleConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub times: Vec<String>,
    #[serde(default)]
    pub jitter_minutes: i64,
    #[serde(default)]
    pub last_success_date: Option<String>,
    #[serde(default)]
    pub last_run_at: Option<i64>,
    #[serde(default)]
    pub last_result: Option<String>,
}

impl Default for ScheduleConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            times: vec!["09:30".to_string()],
            jitter_minutes: 0,
            last_success_date: None,
            last_run_at: None,
            last_result: None,
        }
    }
}

type ScheduleMap = BTreeMap<String, ScheduleConfig>;

// 设置页会自动保存，后台排程也会写同一个文件；写入必须串行，避免旧快照覆盖新配置。
static STORE_LOCK: Mutex<()> = Mutex::new(());

fn file() -> PathBuf {
    store_dir().join("schedules.json")
}

fn load_all() -> ScheduleMap {
    let Ok(text) = std::fs::read_to_string(file()) else {
        return ScheduleMap::new();
    };
    serde_json::from_str(&text).unwrap_or_default()
}

fn save_all(map: &ScheduleMap) -> Result<(), String> {
    let content =
        serde_json::to_string_pretty(map).map_err(|error| format!("序列化排程失败：{error}"))?;
    atomic_write(&file(), &content).map_err(|error| format!("保存排程失败：{error}"))
}

fn parse_hhmm(value: &str) -> Option<(i64, i64)> {
    let (hour, minute) = value.trim().split_once(':')?;
    let hour: i64 = hour.parse().ok()?;
    let minute: i64 = minute.parse().ok()?;
    (0..=23).contains(&hour).then_some(())?;
    (0..=59).contains(&minute).then_some(())?;
    Some((hour, minute))
}

fn normalize_times(times: &[String]) -> Result<Vec<String>, String> {
    let mut values: Vec<String> = Vec::new();
    for time in times {
        let Some((hour, minute)) = parse_hhmm(time) else {
            return Err(format!("时间格式不对：{time}（应为 HH:MM）"));
        };
        let normalized = format!("{hour:02}:{minute:02}");
        if !values.contains(&normalized) {
            values.push(normalized);
        }
    }
    values.sort();
    if values.is_empty() {
        return Err("至少需要一个签到时点".to_string());
    }
    Ok(values)
}

pub fn get(scope: Scope) -> ScheduleConfig {
    load_all()
        .remove(scope.as_str())
        .unwrap_or_else(ScheduleConfig::default)
}

pub fn save(scope: Scope, mut config: ScheduleConfig) -> Result<ScheduleConfig, String> {
    config.times = normalize_times(&config.times)?;
    if !(0..=120).contains(&config.jitter_minutes) {
        return Err("抖动幅度需在 0–120 分钟之间".to_string());
    }
    let _guard = STORE_LOCK.lock().unwrap_or_else(|error| error.into_inner());
    let mut map = load_all();
    if let Some(existing) = map.get(scope.as_str()) {
        config.last_success_date = existing.last_success_date.clone();
        config.last_run_at = existing.last_run_at;
        config.last_result = existing.last_result.clone();
    }
    map.insert(scope.as_str().to_string(), config.clone());
    save_all(&map)?;
    Ok(config)
}

fn deterministic_jitter(scope: &str, date: &str, time: &str, jitter: i64) -> i64 {
    if jitter <= 0 {
        return 0;
    }
    use md5::{Digest, Md5};
    let mut hasher = Md5::new();
    hasher.update(format!("{scope}|{date}|{time}").as_bytes());
    let digest = hasher.finalize();
    let value = u32::from_be_bytes([digest[0], digest[1], digest[2], digest[3]]) as i64;
    value % (2 * jitter + 1) - jitter
}

fn due_now(config: &ScheduleConfig, scope: &str, now: &chrono::DateTime<Local>) -> bool {
    let today = now.format("%Y-%m-%d").to_string();
    let now_minutes = now.hour() as i64 * 60 + now.minute() as i64;
    config.times.iter().any(|time| {
        let Some((hour, minute)) = parse_hhmm(time) else {
            return false;
        };
        let offset = deterministic_jitter(scope, &today, time, config.jitter_minutes);
        let target = hour * 60 + minute + offset;
        now_minutes >= target
    })
}

async fn run_scope(scope: Scope) -> (usize, usize) {
    let outcomes = match scope.platform {
        Platform::Workbuddy => checkin_workbuddy::checkin_all(scope).await,
        Platform::Qoder => checkin_qoder::checkin_all(scope).await,
        Platform::Trae => checkin_trae::checkin_all(scope).await,
    };
    let succeeded = outcomes
        .iter()
        .filter(|outcome| matches!(outcome.result.as_str(), "success" | "already"))
        .count();
    let failed = outcomes
        .iter()
        .filter(|outcome| outcome.result == "error")
        .count();
    (succeeded, failed)
}

async fn tick() {
    let map = load_all();
    if map.is_empty() {
        return;
    }
    let now = Local::now();
    let today = now.format("%Y-%m-%d").to_string();
    let mut updates: Vec<(String, i64, String, Option<String>)> = Vec::new();

    for (scope_key, config) in map.iter() {
        if !config.enabled {
            continue;
        }
        let Ok(scope) = Scope::parse(scope_key) else {
            continue;
        };
        if scope.platform == Platform::Workbuddy && scope.region == Region::Global {
            continue;
        }
        if config.last_success_date.as_deref() == Some(today.as_str()) {
            continue;
        }
        if !due_now(config, scope_key, &now) {
            continue;
        }

        let (succeeded, failed) = run_scope(scope).await;
        updates.push((
            scope_key.clone(),
            now_ms(),
            format!("成功 {succeeded}，失败 {failed}"),
            (succeeded > 0).then(|| today.clone()),
        ));
    }

    if updates.is_empty() {
        return;
    }

    let _guard = STORE_LOCK.lock().unwrap_or_else(|error| error.into_inner());
    let mut latest = load_all();
    for (key, run_at, result, success_date) in updates {
        let Some(config) = latest.get_mut(&key) else {
            continue;
        };
        config.last_run_at = Some(run_at);
        config.last_result = Some(result);
        if success_date.is_some() {
            config.last_success_date = success_date;
        }
    }
    let _ = save_all(&latest);
}

pub fn spawn() {
    tauri::async_runtime::spawn(async move {
        loop {
            tick().await;
            tokio::time::sleep(std::time::Duration::from_secs(30)).await;
        }
    });
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScopeCheckinSummary {
    pub scope: String,
    pub success: usize,
    pub already: usize,
    pub failed: usize,
    pub skipped: usize,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GlobalCheckinSummary {
    pub success: usize,
    pub already: usize,
    pub failed: usize,
    pub skipped: usize,
    pub scopes: Vec<ScopeCheckinSummary>,
}

pub async fn run_all_scopes_summary() -> GlobalCheckinSummary {
    let scopes = [
        "workbuddy-cn",
        "workbuddy-global",
        "trae-cn",
        "trae-global",
        "qoder-cn",
        "qoder-global",
    ];
    let mut summary = GlobalCheckinSummary::default();
    for key in scopes {
        let Ok(scope) = Scope::parse(key) else {
            continue;
        };
        let account_count = crate::account::load(key).len();
        if account_count == 0 {
            continue;
        }
        let mut row = ScopeCheckinSummary {
            scope: key.to_string(),
            ..ScopeCheckinSummary::default()
        };
        if scope.platform == Platform::Workbuddy && scope.region == Region::Global {
            row.skipped = account_count;
        } else {
            let outcomes = match scope.platform {
                Platform::Workbuddy => checkin_workbuddy::checkin_all(scope).await,
                Platform::Qoder => checkin_qoder::checkin_all(scope).await,
                Platform::Trae => checkin_trae::checkin_all(scope).await,
            };
            for outcome in outcomes {
                match outcome.result.as_str() {
                    "success" => row.success += 1,
                    "already" => row.already += 1,
                    "unsupported" => row.skipped += 1,
                    _ => row.failed += 1,
                }
            }
        }
        summary.success += row.success;
        summary.already += row.already;
        summary.failed += row.failed;
        summary.skipped += row.skipped;
        summary.scopes.push(row);
    }
    summary
}

pub async fn run_all_scopes() -> (usize, usize) {
    let summary = run_all_scopes_summary().await;
    (summary.success + summary.already, summary.failed)
}
