use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::secrets;

const SECURE_MAGIC: &[u8; 4] = b"BSL1";

pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}

pub fn home_dir() -> PathBuf {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

pub fn store_dir() -> PathBuf {
    let home = home_dir();
    let directory = home.join(".roster");
    let legacy = home.join(".buddy-switch-lite");
    if !directory.exists() && legacy.exists() {
        if std::fs::rename(&legacy, &directory).is_err() {
            return legacy;
        }
    }
    directory
}

pub fn accounts_file(scope: &str) -> PathBuf {
    store_dir().join("accounts").join(format!("{scope}.json"))
}

/// 兼容两种形态：`BSL1` + DPAPI 密文；以及早期的明文 JSON（读到时下次写入自动加密）。
pub fn read_secure_json<T: DeserializeOwned>(path: &Path) -> Option<T> {
    let bytes = fs::read(path).ok()?;
    let plain = if bytes.starts_with(SECURE_MAGIC) {
        secrets::unprotect(&bytes[SECURE_MAGIC.len()..]).ok()?
    } else {
        bytes
    };
    serde_json::from_slice(&plain).ok()
}

pub fn write_secure_json<T: Serialize>(path: &Path, value: &T) -> io::Result<()> {
    let json = serde_json::to_vec_pretty(value)?;
    let protected = secrets::protect(&json).map_err(io::Error::other)?;
    let mut content = Vec::with_capacity(SECURE_MAGIC.len() + protected.len());
    content.extend_from_slice(SECURE_MAGIC);
    content.extend_from_slice(&protected);
    atomic_write_bytes(path, &content)
}

pub fn atomic_write(path: &Path, content: &str) -> io::Result<()> {
    atomic_write_bytes(path, content.as_bytes())
}

pub fn atomic_write_bytes(path: &Path, content: &[u8]) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let temp = path.with_extension("tmp");
    fs::write(&temp, content)?;
    fs::rename(&temp, path)
}
