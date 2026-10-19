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

    #[test]
    fn custom_audio_and_category_icons_round_trip() {
        let mut config = AppConfig::default();
        let audio_icon = "C:\\icons\\audio.png";
        let category_icon = "file:C:\\icons\\category.ico";
        config
            .audio_category_icons
            .insert("效果".into(), category_icon.into());
        config.audio_entries.push(crate::config::AudioEntry {
            name: "提示音".into(),
            path: "C:\\audio\\beep.wav".into(),
            icon_path: Some(audio_icon.into()),
            icon_glyph: "".into(),
            category: "效果".into(),
            duration_hns: 0,
            file_size_bytes: 0,
            sample_rate: 0,
            channels: 0,
            loop_playback: false,
            hotkey: None,
            hotkey_binding: None,
        });

        let text = serde_json::to_string(&config).expect("serialize icon paths");
        let decoded: AppConfig = serde_json::from_str(&text).expect("deserialize icon paths");

        assert_eq!(
            decoded.audio_entries[0].icon_path.as_deref(),
            Some(audio_icon)
        );
        assert_eq!(decoded.audio_category_icons["效果"], category_icon);
    }
    #[test]
    fn window_positions_round_trip_and_old_configs_default_empty() {
        use crate::config::WindowPosition;

        let mut config = AppConfig::default();
        config.windows.main = Some(WindowPosition { x: 12, y: 34 });
        config.windows.add_source = Some(WindowPosition { x: 56, y: 78 });
        config.windows.audio_edit = Some(WindowPosition { x: -90, y: 120 });

        let text = serde_json::to_string(&config).expect("serialize window positions");
        let decoded: AppConfig = serde_json::from_str(&text).expect("deserialize positions");
        assert_eq!(decoded.windows.main, Some(WindowPosition { x: 12, y: 34 }));
        assert_eq!(
            decoded.windows.add_source,
            Some(WindowPosition { x: 56, y: 78 })
        );
        assert_eq!(
            decoded.windows.audio_edit,
            Some(WindowPosition { x: -90, y: 120 })
        );

        let mut old_config =
            serde_json::to_value(AppConfig::default()).expect("serialize default config");
        old_config.as_object_mut().unwrap().remove("windows");
        let decoded: AppConfig =
            serde_json::from_value(old_config).expect("older config has no window positions");
        assert!(decoded.windows.main.is_none());
        assert!(decoded.windows.add_source.is_none());
        assert!(decoded.windows.audio_edit.is_none());
    }
}

fn config_path() -> PathBuf {
    let base = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("Miccat").join("config.json")
}
