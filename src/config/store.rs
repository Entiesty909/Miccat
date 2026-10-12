//! 配置文件路径、JSON 读写与旧格式迁移。

use std::fs;
use std::path::PathBuf;

use serde::Deserialize;

use super::{AppConfig, Hotkey};

/// 读配置；旧版本补入默认分类根项，损坏或旧热键格式回退默认。
pub(crate) fn load() -> AppConfig {
    let Ok(text) = fs::read_to_string(config_path()) else {
        return AppConfig::default();
    };
    if let Ok(config) = serde_json::from_str::<AppConfig>(&text) {
        return migrate(config);
    }
    if let Ok(legacy) = serde_json::from_str::<LegacyHotkey>(&text) {
        let mut config = AppConfig::default();
        config.hotkeys.toggle_route = legacy.into();
        return config;
    }
    tracing::warn!("config parse failed; using defaults");
    AppConfig::default()
}

fn migrate(mut config: AppConfig) -> AppConfig {
    if config.version < 3 {
        if !config.audio_categories.iter().any(|path| path == "全部") {
            config.audio_categories.insert(0, "全部".into());
        }
        if !config.audio_categories.iter().any(|path| path == "未分类") {
            config
                .audio_categories
                .insert(1.min(config.audio_categories.len()), "未分类".into());
        }
        config.version = 3;
    }
    config
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn v2_config_gains_default_category_roots() {
        let mut config = AppConfig::default();
        config.version = 2;
        config.audio_categories = vec!["游戏".into()];

        let migrated = migrate(config);

        assert_eq!(migrated.version, 3);
        assert_eq!(migrated.audio_categories, vec!["全部", "未分类", "游戏"]);
    }

    #[test]
    fn v3_config_keeps_deleted_category_roots_deleted() {
        let mut config = AppConfig::default();
        config.audio_categories.clear();

        let migrated = migrate(config);

        assert!(migrated.audio_categories.is_empty());
    }
}

fn config_path() -> PathBuf {
    let base = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("Miccat").join("config.json")
}
