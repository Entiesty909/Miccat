//! 配置文件路径、JSON 读写与旧格式迁移。

use std::fs;
use std::path::PathBuf;

use serde::Deserialize;

use super::{AppConfig, Hotkey};

/// 读配置。不存在、损坏或旧版本都回退默认并保留旧热键迁移。
pub(crate) fn load() -> AppConfig {
    let Ok(text) = fs::read_to_string(config_path()) else {
        return AppConfig::default();
    };
    if let Ok(mut config) = serde_json::from_str::<AppConfig>(&text) {
        config.version = 2;
        return config;
    }
    if let Ok(legacy) = serde_json::from_str::<LegacyHotkey>(&text) {
        let mut config = AppConfig::default();
        config.hotkeys.toggle_route = legacy.into();
        return config;
    }
    tracing::warn!("config parse failed; using defaults");
    AppConfig::default()
}

/// 写配置；失败只打日志，不影响音频运行。
pub(crate) fn save(config: &AppConfig) {
    let path = config_path();
    if let Some(dir) = path.parent()
        && let Err(error) = fs::create_dir_all(dir)
    {
        tracing::warn!("create config directory: {error}");
        return;
    }
    match serde_json::to_string_pretty(config) {
        Ok(body) => {
            if let Err(error) = fs::write(&path, format!("{body}\n")) {
                tracing::warn!("save config: {error}");
            }
        }
        Err(error) => tracing::warn!("serialize config: {error}"),
    }
}

#[derive(Deserialize)]
struct LegacyHotkey {
    mods: u32,
    vk: u32,
}

impl From<LegacyHotkey> for Hotkey {
    fn from(value: LegacyHotkey) -> Self {
        Self {
            mods: value.mods,
            vk: value.vk,
        }
    }
}

fn config_path() -> PathBuf {
    let base = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("ProcessMic").join("config.json")
}
