pub mod model;

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::de::DeserializeOwned;
use serde::Serialize;
use tauri::{AppHandle, Manager};

use crate::error::{AppError, AppResult};
use model::{MatchKind, RuleBook, Settings};

/// 配置文件目录（%APPDATA%\com.muxidream.florime）。
pub fn config_dir(app: &AppHandle) -> AppResult<PathBuf> {
    let dir = app
        .path()
        .app_config_dir()
        .map_err(|e| AppError::Config(e.to_string()))?;
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// 数据目录（SQLite 落盘位置）。
pub fn data_dir(app: &AppHandle) -> AppResult<PathBuf> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::Config(e.to_string()))?;
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// 系统默认下载目录（导出文件的落点）。
pub fn download_dir(app: &AppHandle) -> AppResult<PathBuf> {
    let dir = app
        .path()
        .download_dir()
        .map_err(|e| AppError::Config(e.to_string()))?;
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// 规范化进程键：小写、统一反斜杠、去掉首尾空白。
///
/// Windows 路径大小写不敏感，直接小写比较即可。
pub fn normalize_exe_key(s: &str) -> String {
    s.trim().replace('/', "\\").to_lowercase()
}

/// 规范化规则匹配值。按名称匹配时只保留文件名部分。
pub fn normalize_pattern(kind: MatchKind, s: &str) -> String {
    let key = normalize_exe_key(s);
    match kind {
        MatchKind::FullPath => key,
        MatchKind::ExeName => key
            .rsplit('\\')
            .next()
            .unwrap_or(key.as_str())
            .to_string(),
    }
}

/// 读取 JSON，文件不存在或损坏时回退默认值（并备份损坏文件）。
pub fn load_json_or_default<T>(path: &Path) -> T
where
    T: DeserializeOwned + Default,
{
    if !path.exists() {
        return T::default();
    }
    match fs::read_to_string(path) {
        Ok(raw) => match serde_json::from_str::<T>(&raw) {
            Ok(v) => v,
            Err(e) => {
                log::warn!("配置文件解析失败，已回退默认值: {} ({e})", path.display());
                let backup = path.with_extension("json.broken");
                let _ = fs::rename(path, backup);
                T::default()
            }
        },
        Err(e) => {
            log::warn!("配置文件读取失败: {} ({e})", path.display());
            T::default()
        }
    }
}

/// 原子写：先写临时文件再 rename，避免半截文件。
pub fn save_json_atomic<T: Serialize>(path: &Path, value: &T) -> AppResult<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let body = serde_json::to_vec_pretty(value)?;
    let tmp = path.with_extension("json.tmp");
    {
        let mut f = fs::File::create(&tmp)?;
        f.write_all(&body)?;
        f.sync_all()?;
    }
    // Windows 上 rename 到已存在的目标会失败，先删目标。
    if path.exists() {
        let _ = fs::remove_file(path);
    }
    fs::rename(&tmp, path)?;
    Ok(())
}

pub fn rules_path(app: &AppHandle) -> AppResult<PathBuf> {
    Ok(config_dir(app)?.join("rules.json"))
}

pub fn settings_path(app: &AppHandle) -> AppResult<PathBuf> {
    Ok(config_dir(app)?.join("settings.json"))
}

pub fn load_rules(app: &AppHandle) -> RuleBook {
    rules_path(app)
        .map(|p| load_json_or_default::<RuleBook>(&p))
        .unwrap_or_default()
}

pub fn load_settings(app: &AppHandle) -> Settings {
    let mut s = settings_path(app)
        .map(|p| load_json_or_default::<Settings>(&p))
        .unwrap_or_default();
    s.sanitize();
    s
}

pub fn save_rules(app: &AppHandle, book: &RuleBook) -> AppResult<()> {
    save_json_atomic(&rules_path(app)?, book)
}

pub fn save_settings(app: &AppHandle, settings: &Settings) -> AppResult<()> {
    save_json_atomic(&settings_path(app)?, settings)
}
