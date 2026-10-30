use std::path::{Path, PathBuf};

use base64::Engine as _;
use serde_json::Value;
use sha2::{Digest, Sha512};

use crate::account;
use crate::scope::{Platform, Region, Scope};
use crate::store::{home_dir, store_dir};

const CORE_FILES: &[&str] = &[
    "User/globalStorage/storage.json",
    "User/globalStorage/state.vscdb",
    "User/globalStorage/state.vscdb.backup",
    "User/globalStorage/state.vscdb-wal",
    "User/globalStorage/state.vscdb-shm",
    "machineid",
    "Preferences",
    "Local State",
    "Local Storage/config.db",
];

const CORE_DIRS: &[&str] = &[
    "aha",
    "Local Storage/leveldb",
    "Session Storage",
    "Network",
    "Partitions/trae-webview",
    "Partitions/icube-web-crawler",
];

const PURGE: &[&str] = &[
    "logs",
    "User/globalStorage/state.vscdb-wal",
    "User/globalStorage/state.vscdb-shm",
    "User/globalStorage/state.vscdb-journal",
];

const PROCESSES: &[&str] = &[
    "TRAE SOLO CN.exe",
    "TRAE SOLO.exe",
    "Trae CN.exe",
    "Trae.exe",
];

fn appdata_dir() -> PathBuf {
    std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| home_dir().join("AppData/Roaming"))
}

fn data_dir_names(region: Region) -> &'static [&'static str] {
    match region {
        Region::Cn => &["TRAE SOLO CN", "TRAE SOLO", "Trae CN"],
        Region::Global => &["TRAE SOLO", "Trae"],
    }
}

fn client_data_dir(region: Region) -> Option<PathBuf> {
    let base = appdata_dir();
    data_dir_names(region)
        .iter()
        .map(|name| base.join(name))
        .find(|dir| dir.is_dir())
}

const CLOUDIDE_KEY: &str = "iCubeAuthInfo://icube.cloudide";
const TC_HEADER: [u8; 6] = [116, 99, 5, 16, 0, 0];

// Trae 客户端 iCube 信封的混淆常量（随安装包分发，非机密）。
const WOE_T: [u8; 64] = [
    82, 9, 106, 213, 48, 54, 165, 56, 191, 64, 163, 158, 129, 243, 215, 251, 124, 227, 57, 130,
    155, 47, 255, 135, 52, 142, 67, 68, 196, 222, 233, 203, 84, 123, 148, 50, 166, 194, 35, 61,
    238, 76, 149, 11, 66, 250, 195, 78, 8, 46, 161, 102, 40, 217, 36, 178, 118, 91, 162, 73, 109,
    139, 209, 37,
];
const VOE_T: [u8; 64] = [
    31, 221, 168, 51, 136, 7, 199, 49, 177, 18, 16, 89, 39, 128, 236, 95, 96, 81, 127, 169, 25,
    181, 74, 13, 45, 229, 122, 159, 147, 201, 156, 239, 160, 224, 59, 77, 174, 42, 245, 176, 200,
    235, 187, 60, 131, 83, 153, 97, 23, 43, 4, 126, 186, 119, 214, 38, 225, 105, 20, 99, 85, 33,
    12, 125,
];

fn tc_decrypt(envelope: &str) -> Option<String> {
    use aes::Aes128;
    use cbc::cipher::{block_padding::Pkcs7, BlockDecryptMut, KeyIvInit};

    let raw = base64::engine::general_purpose::STANDARD
        .decode(envelope.trim())
        .ok()?;
    if raw.len() < TC_HEADER.len() + 32 + 16 || raw[..TC_HEADER.len()] != TC_HEADER {
        return None;
    }
    let random = &raw[TC_HEADER.len()..TC_HEADER.len() + 32];
    let ciphertext = &raw[TC_HEADER.len() + 32..];

    let mut pepper = [0u8; 64];
    for index in 0..64 {
        pepper[index] = WOE_T[index] ^ VOE_T[index];
    }
    let mut joined = Vec::with_capacity(128);
    joined.extend_from_slice(&Sha512::digest(random));
    joined.extend_from_slice(&pepper);
    let digest = Sha512::digest(&joined);

    let decryptor = cbc::Decryptor::<Aes128>::new(digest[..16].into(), digest[16..32].into());
    let padded = decryptor.decrypt_padded_vec_mut::<Pkcs7>(ciphertext).ok()?;
    if padded.len() <= 64 {
        return None;
    }
    let (tag, body) = padded.split_at(64);
    if Sha512::digest(body)[..] != *tag {
        return None;
    }
    String::from_utf8(body.to_vec()).ok()
}

/// 读取 Trae 客户端当前登录的用户 ID；客户端未安装、未登录或信封无法解密时返回 None。
pub fn current_uid(region: Region) -> Option<String> {
    let storage = client_data_dir(region)?.join("User/globalStorage/storage.json");
    let text = std::fs::read_to_string(storage).ok()?;
    let json: Value = serde_json::from_str(&text).ok()?;
    let plain = tc_decrypt(json.get(CLOUDIDE_KEY)?.as_str()?)?;
    let session: Value = serde_json::from_str(&plain).ok()?;
    session
        .get("userId")
        .and_then(|value| {
            value
                .as_str()
                .map(str::to_string)
                .or_else(|| value.as_i64().map(|number| number.to_string()))
        })
        .filter(|uid| !uid.trim().is_empty())
}

fn running() -> bool {
    PROCESSES
        .iter()
        .any(|name| crate::process::image_running(name))
}

fn resolve(root: &Path, relative: &str) -> PathBuf {
    relative
        .split('/')
        .fold(root.to_path_buf(), |path, part| path.join(part))
}

fn safe_slot(user_id: &str) -> bool {
    let trimmed = user_id.trim();
    !trimmed.is_empty()
        && !trimmed.contains('/')
        && !trimmed.contains('\\')
        && trimmed != "."
        && trimmed != ".."
}

fn profiles_root(scope: Scope) -> PathBuf {
    store_dir().join("trae-profiles").join(scope.as_str())
}

fn profile_dir(scope: Scope, uid: &str) -> Option<PathBuf> {
    safe_slot(uid).then(|| profiles_root(scope).join(uid.trim()))
}

fn copy_dir(source: &Path, target: &Path) -> Result<(), String> {
    std::fs::create_dir_all(target).map_err(|error| format!("创建目录失败：{error}"))?;
    let entries = std::fs::read_dir(source).map_err(|error| format!("读取目录失败：{error}"))?;
    for entry in entries.flatten() {
        let path = entry.path();
        let destination = target.join(entry.file_name());
        if path.is_dir() {
            copy_dir(&path, &destination)?;
        } else {
            std::fs::copy(&path, &destination).map_err(|error| {
                format!("复制 {} 失败：{error}", path.display())
            })?;
        }
    }
    Ok(())
}

fn copy_snapshot(source_root: &Path, target_root: &Path) -> Result<u64, String> {
    std::fs::create_dir_all(target_root).map_err(|error| format!("创建快照目录失败：{error}"))?;
    let mut copied = 0u64;
    for relative in CORE_FILES {
        let source = resolve(source_root, relative);
        if !source.is_file() {
            continue;
        }
        let target = resolve(target_root, relative);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(|error| format!("创建目录失败：{error}"))?;
        }
        std::fs::copy(&source, &target)
            .map_err(|error| format!("复制 {} 失败：{error}", source.display()))?;
        copied += 1;
    }
    for relative in CORE_DIRS {
        let source = resolve(source_root, relative);
        if !source.is_dir() {
            continue;
        }
        let target = resolve(target_root, relative);
        copy_dir(&source, &target)?;
        copied += 1;
    }
    Ok(copied)
}

fn remove_path(path: &Path) {
    if path.is_dir() {
        let _ = std::fs::remove_dir_all(path);
    } else if path.is_file() {
        let _ = std::fs::remove_file(path);
    }
}

pub fn save_profile(scope: Scope, account_id: &str) -> Result<u64, String> {
    if scope.platform != Platform::Trae {
        return Err("该命令仅用于 Trae".to_string());
    }
    if running() {
        return Err("Trae 客户端正在运行，请先退出后再保存登录态".to_string());
    }
    let account =
        account::find(scope.as_str(), account_id).ok_or_else(|| "账号不存在".to_string())?;
    let uid = account
        .uid
        .as_deref()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "该账号缺少用户 ID，无法保存登录态".to_string())?;
    let source = client_data_dir(scope.region).ok_or_else(|| {
        format!(
            "未找到 Trae 客户端数据目录（{}）。请先启动一次 Trae 并登录。",
            appdata_dir().display()
        )
    })?;
    let target = profile_dir(scope, uid).ok_or_else(|| "账号 ID 非法，无法保存登录态".to_string())?;

    remove_path(&target);
    let copied = copy_snapshot(&source, &target)?;
    if copied == 0 {
        remove_path(&target);
        return Err("没有复制到任何登录态文件，请确认 Trae 客户端已登录".to_string());
    }
    Ok(copied)
}

fn restore_snapshot(source: &Path, client: &Path) -> Result<u64, String> {
    for relative in PURGE {
        remove_path(&resolve(client, relative));
    }
    copy_snapshot(source, client)
}

pub fn switch(scope: Scope, account_id: &str) -> Result<(), String> {
    if scope.platform != Platform::Trae {
        return Err("该命令仅用于 Trae".to_string());
    }
    if running() {
        return Err("Trae 客户端正在运行，请先退出后再切换".to_string());
    }
    let account =
        account::find(scope.as_str(), account_id).ok_or_else(|| "账号不存在".to_string())?;
    let uid = account
        .uid
        .as_deref()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "该账号缺少用户 ID".to_string())?;
    let target = profile_dir(scope, uid).ok_or_else(|| "账号 ID 非法".to_string())?;
    if !target.is_dir() {
        return Err(
            "该账号还没有登录态快照：请先在 Trae 客户端登录该账号，退出客户端后点「保存登录态」再切换"
                .to_string(),
        );
    }
    let client = client_data_dir(scope.region).ok_or_else(|| {
        "未找到 Trae 客户端数据目录。请先启动一次 Trae 并登录。".to_string()
    })?;

    let last = profiles_root(scope).join("last");
    remove_path(&last);
    let saved = copy_snapshot(&client, &last).unwrap_or(0);

    if let Err(error) = restore_snapshot(&target, &client) {
        if saved > 0 {
            let _ = restore_snapshot(&last, &client);
            return Err(format!("恢复登录态失败（{error}），已尝试还原为切换前状态"));
        }
        return Err(format!("恢复登录态失败（{error}），且无法自动还原"));
    }

    let key_file = resolve(&client, "User/globalStorage/storage.json");
    let db_file = resolve(&client, "User/globalStorage/state.vscdb");
    if !key_file.is_file() && !db_file.is_file() {
        return Err("恢复完成但未找到登录态核心文件，请检查快照内容".to_string());
    }
    Ok(())
}

