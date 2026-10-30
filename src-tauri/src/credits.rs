use std::collections::BTreeMap;
use std::path::PathBuf;

use chrono::{Datelike, Local, NaiveDate, TimeZone};
use serde::Serialize;
use serde_json::{json, Value};

use crate::account::{self, Account};
use crate::checkin_qoder;
use crate::checkin_trae;
use crate::checkin_workbuddy;
use crate::scope::{Platform, Region, Scope};
use crate::store::{atomic_write, store_dir};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreditAccount {
    pub account_id: String,
    pub uid: String,
    pub name: String,
    pub ok: bool,
    pub remaining: Option<f64>,
    pub total: Option<f64>,
    pub today: Option<f64>,
    pub expires_at: Option<i64>,
    pub expiring: Option<f64>,
    pub plan: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DailyPoint {
    pub date: String,
    pub amount: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreditsSummary {
    pub platform: String,
    pub accounts: Vec<CreditAccount>,
    pub total_remaining: f64,
    pub today: f64,
    pub week: f64,
    pub month: f64,
    pub daily: Vec<DailyPoint>,
    pub ok_accounts: usize,
    pub failed_accounts: usize,
}

impl CreditAccount {
    fn new(account: &Account) -> Self {
        Self {
            account_id: account.id.clone(),
            uid: account.uid.clone().unwrap_or_default(),
            name: account
                .email
                .clone()
                .or_else(|| account.nickname.clone())
                .or_else(|| account.uid.clone())
                .unwrap_or_else(|| "未知账号".to_string()),
            ok: false,
            remaining: None,
            total: None,
            today: None,
            expires_at: None,
            expiring: None,
            plan: None,
            error: None,
        }
    }
}

/// 按到期日合并剩余积分，返回最近一天 `(毫秒时间戳, 该天到期的积分)`。
fn nearest_expiring(
    entries: impl Iterator<Item = (f64, Option<i64>)>,
) -> Option<(i64, f64)> {
    let mut grouped: BTreeMap<NaiveDate, (i64, f64)> = BTreeMap::new();
    for (amount, expire_at) in entries {
        if amount <= 0.0 {
            continue;
        }
        let Some(ms) = expire_at else {
            continue;
        };
        let Some(date) = Local
            .timestamp_millis_opt(ms)
            .single()
            .map(|when| when.date_naive())
        else {
            continue;
        };
        let slot = grouped.entry(date).or_insert((ms, 0.0));
        slot.0 = slot.0.min(ms);
        slot.1 += amount;
    }
    grouped
        .into_iter()
        .next()
        .map(|(_, (ms, amount))| (ms, amount))
}

fn display_message(response: &Value) -> String {
    response
        .get("message")
        .or_else(|| response.get("msg"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string()
}

fn as_f64(value: &Value) -> Option<f64> {
    value
        .as_f64()
        .or_else(|| value.as_i64().map(|number| number as f64))
        .or_else(|| value.as_str().and_then(|text| text.trim().parse().ok()))
}

fn first_number(value: &Value, keys: &[&str]) -> Option<f64> {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(as_f64))
}

fn first_timestamp(value: &Value, keys: &[&str]) -> Option<i64> {
    for key in keys {
        let Some(raw) = value.get(*key) else {
            continue;
        };
        let mut millis = match raw {
            Value::Number(number) => number.as_i64().or_else(|| number.as_f64().map(|v| v as i64)),
            Value::String(text) => text.trim().parse::<i64>().ok(),
            _ => None,
        }?;
        if millis < 10_000_000_000 {
            millis *= 1000;
        }
        return Some(millis);
    }
    None
}

fn value_at<'a>(value: &'a Value, path: &[&str]) -> Option<&'a Value> {
    path.iter().try_fold(value, |current, key| current.get(*key))
}

fn resource_accounts(response: &Value) -> Vec<&Value> {
    let paths: &[&[&str]] = &[
        &["data", "Accounts"],
        &["data", "data", "Accounts"],
        &["data", "data", "accounts"],
        &["data", "accounts"],
        &["data", "Response", "Data", "Accounts"],
    ];
    for path in paths {
        if let Some(items) = value_at(response, path).and_then(Value::as_array) {
            return items.iter().collect();
        }
    }
    Vec::new()
}

const TOTAL_KEYS: &[&str] = &[
    "CycleCapacitySizePrecise",
    "CycleCapacitySize",
    "CycleTotalCapacity",
    "CapacitySizePrecise",
    "CapacitySize",
    "SlicePeriodCapacitySizePrecise",
    "SlicePeriodCapacitySize",
];

const REMAIN_KEYS: &[&str] = &[
    "CycleCapacityRemainPrecise",
    "CycleCapacityRemain",
    "CycleRemainCapacity",
    "CapacityRemainPrecise",
    "CapacityRemain",
    "SlicePeriodCapacityRemainPrecise",
    "SlicePeriodCapacityRemain",
];

const EXPIRE_KEYS: &[&str] = &[
    "DeductionEndTime",
    "deductionEndTime",
    "ExpiredTime",
    "expiredTime",
    "CycleEndTime",
    "cycleEndTime",
    "ExpireTime",
    "ExpireAt",
    "PackageEndTime",
    "SlicePeriodEndTime",
];

const PAID_PACKAGE_CODES: &[&str] = &[
    "TCACA_code_002_AkiJS3ZHF5",
    "TCACA_code_023_4xbGhMrE6q",
    "TCACA_code_026_BaESVICNoi",
    "TCACA_code_027_0FCGVA6vSa",
    "TCACA_code_009_0XmEQc2xOf",
    "TCACA_code_038_OhvqZtiPKr",
];

const FREE_PACKAGE_CODES: &[&str] = &[
    "TCACA_code_008_cfWoLwvjU4",
    "TCACA_code_007_nzdH5h4Nl0",
    "TCACA_code_028_NtpWi0jzXs",
    "TCACA_code_029_6wCGEWquYy",
    "TCACA_code_030_BjSt89qTvr",
];

fn resource_packages(response: &Value) -> Vec<&Value> {
    let paths: &[&[&str]] = &[
        &["data", "Packages"],
        &["data", "data", "Packages"],
        &["data", "data", "packages"],
        &["data", "packages"],
    ];
    for path in paths {
        if let Some(items) = value_at(response, path).and_then(Value::as_array) {
            return items.iter().collect();
        }
    }
    Vec::new()
}

fn is_unauthorized(response: &Value) -> bool {
    let code = response.get("code").and_then(Value::as_i64).unwrap_or(-1);
    if code == 401 || code == 403 {
        return true;
    }
    let message = display_message(response).to_lowercase();
    ["unauthorized", "401", "登录", "失效", "过期", "token"]
        .iter()
        .any(|keyword| message.contains(keyword))
}

async fn wb_send(
    account: &Account,
    url: &str,
    body: &Value,
    web_headers: bool,
) -> Result<Value, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|error| format!("创建 HTTP 客户端失败：{error}"))?;
    let mut request = client.post(url).json(body);
    for (name, value) in checkin_workbuddy::auth_headers(account) {
        request = request.header(name, value);
    }
    if web_headers {
        let origin = if url.starts_with("https://www.workbuddy.cn") {
            "https://www.workbuddy.cn"
        } else if url.starts_with("https://www.workbuddy.ai") {
            "https://www.workbuddy.ai"
        } else {
            "https://www.codebuddy.cn"
        };
        request = request
            .header("X-Client-Platform", "web")
            .header("Accept", "application/json, text/plain, */*")
            .header("Origin", origin)
            .header("Referer", format!("{origin}/profile/plans-usage"));
    }
    let response = request.send().await.map_err(|error| format!("网络错误：{error}"))?;
    let status = response.status();
    let text = response.text().await.unwrap_or_default();
    serde_json::from_str(&text).map_err(|_| {
        let snippet: String = text.chars().take(200).collect();
        format!("HTTP {status}：{snippet}")
    })
}

async fn wb_post(
    scope: Scope,
    account: &Account,
    url: &str,
    body: Value,
    web_headers: bool,
) -> Result<Value, String> {
    let response = wb_send(account, url, &body, web_headers).await?;
    if is_unauthorized(&response) && account.refresh_token.is_some() {
        let refreshed = checkin_workbuddy::refresh_account(scope, account).await?;
        return wb_send(&refreshed, url, &body, web_headers).await;
    }
    Ok(response)
}

fn resource_endpoint(scope: Scope, account: &Account) -> &'static str {
    match scope.region {
        Region::Global => "https://www.workbuddy.ai",
        Region::Cn => {
            let domain = account.domain.trim().to_ascii_lowercase();
            if domain == "workbuddy.cn" || domain == "www.workbuddy.cn" {
                "https://www.workbuddy.cn"
            } else {
                "https://www.codebuddy.cn"
            }
        }
    }
}

fn package_entry(item: &Value, now: i64) -> Option<(Option<String>, f64, f64, Option<i64>)> {
    let slice = item
        .get("SlicePeriodUsageDetails")
        .and_then(Value::as_array)
        .and_then(|items| items.first());
    let number = |keys: &[&str]| {
        first_number(item, keys).or_else(|| slice.and_then(|value| first_number(value, keys)))
    };
    let total = number(TOTAL_KEYS).unwrap_or(0.0).max(0.0);
    let remaining = number(REMAIN_KEYS).unwrap_or(0.0).max(0.0);
    if total <= 0.0 && remaining <= 0.0 {
        return None;
    }
    let expire_at = first_timestamp(item, EXPIRE_KEYS)
        .or_else(|| slice.and_then(|value| first_timestamp(value, EXPIRE_KEYS)));
    if expire_at.is_some_and(|timestamp| timestamp <= now) {
        return None;
    }
    let code = item
        .get("PackageCode")
        .or_else(|| item.get("packageCode"))
        .and_then(Value::as_str)
        .map(str::to_string);
    Some((code, total, remaining, expire_at))
}

async fn wb_balance(
    scope: Scope,
    account: &Account,
) -> Result<(f64, f64, Option<i64>, Option<f64>), String> {
    let endpoint = resource_endpoint(scope, account);
    let summary_url = format!("{endpoint}/billing/meter/get-user-resource-summary");
    let paid_url = format!("{endpoint}/billing/meter/get-user-resource-paid-packages");
    let free_url = format!("{endpoint}/billing/meter/get-user-resource-free-packages");

    let now = Local::now();
    let start = now
        .date_naive()
        .and_hms_opt(0, 0, 0)
        .unwrap_or(now.naive_local());
    let end = now
        .date_naive()
        .and_hms_opt(23, 59, 59)
        .unwrap_or(now.naive_local());

    let summary = wb_post(scope, account, &summary_url, json!({}), true).await?;
    let paid = wb_post(
        scope,
        account,
        &paid_url,
        json!({
            "PageNumber": 1,
            "PageSize": 200,
            "Status": [0, 3],
            "PackageCodes": PAID_PACKAGE_CODES,
            "NeedRenewInfo": true,
        }),
        true,
    )
    .await?;
    let free = wb_post(
        scope,
        account,
        &free_url,
        json!({
            "PageNumber": 1,
            "PageSize": 200,
            "Status": [0, 3],
            "SlicePeriodStartTime": start.format("%Y-%m-%d %H:%M:%S").to_string(),
            "SlicePeriodEndTime": end.format("%Y-%m-%d %H:%M:%S").to_string(),
            "PackageCodes": FREE_PACKAGE_CODES,
        }),
        true,
    )
    .await?;

    let now_ms_value = crate::store::now_ms();
    let mut entries: Vec<(Option<String>, f64, f64, Option<i64>)> = Vec::new();
    let mut errors: Vec<String> = Vec::new();
    let mut any_ok = false;

    for (label, response, packages) in [
        ("summary", &summary, true),
        ("paid", &paid, false),
        ("free", &free, false),
    ] {
        let code = response.get("code").and_then(Value::as_i64).unwrap_or(-1);
        if code != 0 && code != 200 {
            let message = display_message(response);
            if !message.is_empty() {
                errors.push(format!("{label}：{message}"));
            }
            continue;
        }
        any_ok = true;
        let items = if packages {
            resource_packages(response)
        } else {
            resource_accounts(response)
        };
        for item in items {
            let Some(entry) = package_entry(item, now_ms_value) else {
                continue;
            };
            if let Some(code) = &entry.0 {
                entries.retain(|existing| existing.0.as_deref() != Some(code.as_str()));
            }
            entries.push(entry);
        }
    }

    if !any_ok {
        return Err(if errors.is_empty() {
            "资源接口全部不可用".to_string()
        } else {
            errors.join("；")
        });
    }
    if entries.is_empty() {
        return Err("没有可用的积分资源包".to_string());
    }

    let remaining: f64 = entries.iter().map(|entry| entry.2).sum();
    let total: f64 = entries.iter().map(|entry| entry.1).sum();
    let expires_at = entries.iter().filter_map(|entry| entry.3).min();
    let expiring = nearest_expiring(entries.iter().map(|entry| (entry.2, entry.3)))
        .map(|(_, amount)| amount);
    Ok((remaining, total, expires_at, expiring))
}

fn official_usage_url(scope: Scope) -> String {
    match scope.region {
        Region::Cn => "https://www.workbuddy.cn/billing/meter/get-user-request-usage".to_string(),
        Region::Global => format!(
            "{}/billing/meter/get-user-request-usage",
            scope.billing_base()
        ),
    }
}

fn row_date(row: &Value) -> Option<String> {
    let value = row
        .get("requestTime")
        .or_else(|| row.get("request_time"))
        .or_else(|| row.get("time"))?;
    if let Some(text) = value.as_str() {
        let date = text.trim().get(..10)?;
        NaiveDate::parse_from_str(date, "%Y-%m-%d").ok()?;
        return Some(date.to_string());
    }
    let mut millis = value.as_i64()?;
    if millis < 10_000_000_000 {
        millis *= 1000;
    }
    chrono::DateTime::from_timestamp_millis(millis)
        .map(|date| date.with_timezone(&Local).format("%Y-%m-%d").to_string())
}

async fn wb_usage(scope: Scope, account: &Account) -> Result<BTreeMap<String, f64>, String> {
    let today = Local::now().date_naive();
    let start = today - chrono::Duration::days(30);
    let start_time = format!("{start} 00:00:00");
    let end_time = format!("{today} 23:59:59");
    let url = official_usage_url(scope);

    let mut rows: Vec<Value> = Vec::new();
    for page in 1..=5 {
        let response = wb_post(
            scope,
            account,
            &url,
            json!({
                "startTime": start_time,
                "endTime": end_time,
                "pageNum": page,
                "pageSize": 3000,
            }),
            false,
        )
        .await?;
        let code = response.get("code").and_then(Value::as_i64).unwrap_or(-1);
        if code != 0 && code != 200 {
            return Err(display_message(&response));
        }
        let data = response.get("data").cloned().unwrap_or_else(|| json!({}));
        let items = data
            .get("data")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let total = data
            .get("total")
            .and_then(|value| {
                value
                    .as_u64()
                    .or_else(|| value.as_str().and_then(|text| text.trim().parse().ok()))
            })
            .unwrap_or(items.len() as u64) as usize;
        let length = items.len();
        rows.extend(items);
        if length == 0 || rows.len() >= total {
            break;
        }
    }

    let mut daily: BTreeMap<String, f64> = BTreeMap::new();
    for row in rows {
        let Some(credit) = row.get("credit").and_then(as_f64) else {
            continue;
        };
        let Some(date) = row_date(&row) else {
            continue;
        };
        *daily.entry(date).or_insert(0.0) += credit;
    }
    Ok(daily)
}

fn window_sums(daily: &BTreeMap<String, f64>, today: &str) -> (f64, f64, f64) {
    let today = NaiveDate::parse_from_str(today, "%Y-%m-%d").ok();
    let mut day = 0.0;
    let mut week = 0.0;
    let mut month = 0.0;
    if let Some(today) = today {
        for (date, amount) in daily {
            let Ok(date) = NaiveDate::parse_from_str(date, "%Y-%m-%d") else {
                continue;
            };
            let offset = (today - date).num_days();
            if offset == 0 {
                day += amount;
            }
            if (0..7).contains(&offset) {
                week += amount;
            }
            if date.year() == today.year() && date.month() == today.month() {
                month += amount;
            }
        }
    }
    (day, week, month)
}

fn last_days(daily: &BTreeMap<String, f64>, today: &str, days: i64) -> Vec<DailyPoint> {
    let Ok(today) = NaiveDate::parse_from_str(today, "%Y-%m-%d") else {
        return Vec::new();
    };
    (0..days)
        .rev()
        .map(|offset| {
            let date = (today - chrono::Duration::days(offset))
                .format("%Y-%m-%d")
                .to_string();
            DailyPoint {
                amount: *daily.get(&date).unwrap_or(&0.0),
                date,
            }
        })
        .collect()
}

async fn wb_summary(scope: Scope) -> Result<CreditsSummary, String> {
    let accounts = account::load(scope.as_str());
    let today = Local::now().format("%Y-%m-%d").to_string();
    let mut rows = Vec::new();
    let mut daily: BTreeMap<String, f64> = BTreeMap::new();
    let mut total_remaining = 0.0;
    let mut ok_accounts = 0usize;
    let mut failed_accounts = 0usize;

    for account in accounts {
        let mut row = CreditAccount::new(&account);
        let mut errors = Vec::new();
        match wb_balance(scope, &account).await {
            Ok((remaining, total, expires_at, expiring)) => {
                row.ok = true;
                row.remaining = Some(remaining);
                row.total = Some(total);
                row.expires_at = expires_at;
                row.expiring = expiring;
                total_remaining += remaining;
            }
            Err(error) => errors.push(error),
        }
        match wb_usage(scope, &account).await {
            Ok(usage) => {
                let (day, _, _) = window_sums(&usage, &today);
                row.today = Some(day);
                for (date, amount) in usage {
                    *daily.entry(date).or_insert(0.0) += amount;
                }
            }
            Err(error) => errors.push(error),
        }
        if !errors.is_empty() {
            row.error = Some(errors.join("；"));
        }
        if row.ok {
            ok_accounts += 1;
        } else {
            failed_accounts += 1;
        }
        rows.push(row);
    }

    let (today_sum, week, month) = window_sums(&daily, &today);
    Ok(CreditsSummary {
        platform: "workbuddy".to_string(),
        accounts: rows,
        total_remaining,
        today: today_sum,
        week,
        month,
        daily: last_days(&daily, &today, 30),
        ok_accounts,
        failed_accounts,
    })
}

type SnapshotMap = BTreeMap<String, BTreeMap<String, f64>>;

fn snapshot_file(scope: Scope) -> PathBuf {
    store_dir().join(format!("credits-snapshots.{}.json", scope.as_str()))
}

fn load_snapshots(scope: Scope) -> SnapshotMap {
    let Ok(text) = std::fs::read_to_string(snapshot_file(scope)) else {
        return SnapshotMap::new();
    };
    serde_json::from_str(&text).unwrap_or_default()
}

fn save_snapshots(scope: Scope, snapshots: &SnapshotMap) {
    if let Ok(content) = serde_json::to_string_pretty(snapshots) {
        let _ = atomic_write(&snapshot_file(scope), &content);
    }
}

fn snapshot_series(
    scope: Scope,
    remaining: &BTreeMap<String, f64>,
    today: &str,
) -> (
    BTreeMap<String, f64>,
    BTreeMap<String, f64>,
    f64,
    f64,
    f64,
) {
    let mut snapshots = load_snapshots(scope);
    snapshots.insert(today.to_string(), remaining.clone());
    if snapshots.len() > 90 {
        let excess = snapshots.len() - 90;
        let keys: Vec<String> = snapshots.keys().take(excess).cloned().collect();
        for key in keys {
            snapshots.remove(&key);
        }
    }
    save_snapshots(scope, &snapshots);

    let dates: Vec<String> = snapshots.keys().cloned().collect();
    let mut daily: BTreeMap<String, f64> = BTreeMap::new();
    let mut today_by_uid: BTreeMap<String, f64> = BTreeMap::new();
    for pair in dates.windows(2) {
        let previous = &pair[0];
        let current = &pair[1];
        let (Some(previous_map), Some(current_map)) =
            (snapshots.get(previous), snapshots.get(current))
        else {
            continue;
        };
        let mut total = 0.0;
        for (uid, current_value) in current_map {
            if let Some(previous_value) = previous_map.get(uid) {
                let delta = (previous_value - current_value).max(0.0);
                total += delta;
                if current == today {
                    today_by_uid.insert(uid.clone(), delta);
                }
            }
        }
        daily.insert(current.clone(), total);
    }

    let (day, week, month) = window_sums(&daily, today);
    (daily, today_by_uid, day, week, month)
}

async fn trae_balance(
    scope: Scope,
    account: &Account,
) -> Result<(f64, f64, Option<i64>, Option<f64>), String> {
    let (status, body) = checkin_trae::post(scope, "/trae/api/v2/pay/ide_user_ent_usage", account).await?;
    let payload = checkin_trae::parse_json(status, &body)?;
    let packs = payload
        .get("user_entitlement_pack_list")
        .and_then(Value::as_array)
        .ok_or_else(|| "响应中缺少 user_entitlement_pack_list".to_string())?;
    let mut remaining = 0.0;
    let mut total = 0.0;
    let mut expiries: Vec<(f64, Option<i64>)> = Vec::new();
    for pack in packs {
        let limit = pack
            .get("entitlement_base_info")
            .and_then(|value| value.get("quota"))
            .and_then(|value| value.get("credits_limit"))
            .and_then(as_f64);
        let Some(limit) = limit else {
            continue;
        };
        let used = pack
            .get("usage")
            .and_then(|value| value.get("credits_amount"))
            .and_then(as_f64)
            .unwrap_or(0.0);
        let left = (limit - used).max(0.0);
        remaining += left;
        total += limit;
        let expire = pack
            .get("expire_time")
            .and_then(Value::as_i64)
            .filter(|seconds| *seconds > 0)
            .map(|seconds| seconds * 1000);
        expiries.push((left, expire));
    }
    if total == 0.0 {
        total = payload
            .get("usage_summary")
            .and_then(|value| value.get("total_amount"))
            .and_then(as_f64)
            .unwrap_or(0.0);
    }
    let (expires_at, expiring) = match nearest_expiring(expiries.into_iter()) {
        Some((ms, amount)) => (Some(ms), Some(amount)),
        None => (None, None),
    };
    Ok((remaining, total, expires_at, expiring))
}

async fn trae_summary(scope: Scope) -> Result<CreditsSummary, String> {
    let accounts = account::load(scope.as_str());
    let today = Local::now().format("%Y-%m-%d").to_string();
    let mut rows = Vec::new();
    let mut remaining_map: BTreeMap<String, f64> = BTreeMap::new();
    for account in accounts {
        let mut row = CreditAccount::new(&account);
        match trae_balance(scope, &account).await {
            Ok((remaining, total, expires_at, expiring)) => {
                row.ok = true;
                row.remaining = Some(remaining);
                row.total = Some(total);
                row.expires_at = expires_at;
                row.expiring = expiring;
                if !row.uid.is_empty() {
                    remaining_map.insert(row.uid.clone(), remaining);
                }
            }
            Err(error) => row.error = Some(error),
        }
        rows.push(row);
    }

    let (daily, today_by_uid, day, week, month) = snapshot_series(scope, &remaining_map, &today);
    for row in &mut rows {
        row.today = today_by_uid.get(&row.uid).copied();
    }
    let total_remaining = rows
        .iter()
        .filter_map(|row| row.remaining)
        .sum::<f64>();
    let ok_accounts = rows.iter().filter(|row| row.ok).count();

    Ok(CreditsSummary {
        platform: "trae".to_string(),
        failed_accounts: rows.len() - ok_accounts,
        accounts: rows,
        total_remaining,
        today: day,
        week,
        month,
        daily: last_days(&daily, &today, 30),
        ok_accounts,
    })
}

fn qoder_base(region: Region) -> &'static str {
    match region {
        Region::Cn => "https://openapi.qoder.com.cn",
        Region::Global => "https://openapi.qoder.sh",
    }
}

async fn qoder_get(scope: Scope, account: &Account, path: &str) -> Result<Value, String> {
    let url = format!("{}{}", qoder_base(scope.region), path);
    let mut request = checkin_qoder::client()
        .get(&url)
        .timeout(std::time::Duration::from_secs(20));
    for (name, value) in checkin_qoder::headers(scope, account) {
        request = request.header(name, value);
    }
    let response = request.send().await.map_err(|error| format!("网络错误：{error}"))?;
    let status = response.status();
    let text = response.text().await.unwrap_or_default();
    serde_json::from_str(&text).map_err(|_| {
        let snippet: String = text.chars().take(200).collect();
        format!("HTTP {status}：{snippet}")
    })
}

async fn qoder_balance(
    scope: Scope,
    account: &Account,
) -> Result<(f64, f64, Option<String>, Option<i64>, Option<f64>), String> {
    let quota = qoder_get(scope, account, "/api/v2/quota/usage").await?;
    let user = quota.get("userQuota");
    let addon = quota.get("addOnQuota");
    let number = |source: Option<&Value>, key: &str| {
        source
            .and_then(|value| value.get(key))
            .and_then(as_f64)
            .unwrap_or(0.0)
    };
    let remaining = number(user, "remaining") + number(addon, "remaining");
    let total = number(user, "total") + number(addon, "total");
    let plan = qoder_get(scope, account, "/api/v2/user/plan")
        .await
        .ok()
        .and_then(|payload| {
            payload
                .get("planName")
                .or_else(|| payload.get("name"))
                .and_then(Value::as_str)
                .map(str::to_string)
        });
    let expires_at = first_timestamp(&quota, &["expiresAt", "expireAt", "expiredAt"]);
    let expiring = expires_at.map(|_| remaining.max(0.0));
    Ok((remaining.max(0.0), total.max(0.0), plan, expires_at, expiring))
}

async fn qoder_summary(scope: Scope) -> Result<CreditsSummary, String> {
    let accounts = account::load(scope.as_str());
    let today = Local::now().format("%Y-%m-%d").to_string();
    let mut rows = Vec::new();
    let mut remaining_map: BTreeMap<String, f64> = BTreeMap::new();
    for account in accounts {
        let mut row = CreditAccount::new(&account);
        match qoder_balance(scope, &account).await {
            Ok((remaining, total, plan, expires_at, expiring)) => {
                row.ok = true;
                row.remaining = Some(remaining);
                row.total = Some(total);
                row.plan = plan;
                row.expires_at = expires_at;
                row.expiring = expiring;
                if !row.uid.is_empty() {
                    remaining_map.insert(row.uid.clone(), remaining);
                }
            }
            Err(error) => row.error = Some(error),
        }
        rows.push(row);
    }

    let (daily, today_by_uid, day, week, month) = snapshot_series(scope, &remaining_map, &today);
    for row in &mut rows {
        row.today = today_by_uid.get(&row.uid).copied();
    }
    let total_remaining = rows
        .iter()
        .filter_map(|row| row.remaining)
        .sum::<f64>();
    let ok_accounts = rows.iter().filter(|row| row.ok).count();

    Ok(CreditsSummary {
        platform: "qoder".to_string(),
        failed_accounts: rows.len() - ok_accounts,
        accounts: rows,
        total_remaining,
        today: day,
        week,
        month,
        daily: last_days(&daily, &today, 30),
        ok_accounts,
    })
}

pub async fn summary(scope: Scope) -> Result<CreditsSummary, String> {    match scope.platform {
        Platform::Workbuddy => wb_summary(scope).await,
        Platform::Trae => trae_summary(scope).await,
        Platform::Qoder => qoder_summary(scope).await,
    }
}



