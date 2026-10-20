//! 配置文件路径、JSON 读写与旧格式迁移。

use std::fs;
use std::path::PathBuf;

use serde::Deserialize;

use super::{AppConfig, Hotkey};

/// 读配置；旧版本补入默认分类根项，损坏或旧热键格式回退默认。
/// 配置文件缺失时返回内存默认值，避免首次启动依赖磁盘状态。
pub(crate) fn load() -> AppConfig {
    // 读取失败统一走内存默认值：启动阶段不应因缺失或暂时不可读的配置阻塞音频。
    let Ok(text) = fs::read_to_string(config_path()) else {
        return AppConfig::default();
    };
    // 先尝试当前 schema；成功后仍要经过迁移以补齐版本级不变量。
    if let Ok(config) = serde_json::from_str::<AppConfig>(&text) {
        // 当前格式反序列化后仍走迁移，确保版本号和根分类不变量一次性补齐。
        return migrate(config);
    }
    // 旧版只有热键根对象，转换后其余设置使用当前默认配置。
    if let Ok(legacy) = serde_json::from_str::<LegacyHotkey>(&text) {
        // 旧格式只携带根热键；其余字段不能臆造，使用当前默认树补齐。
        let mut config = AppConfig::default();
        config.hotkeys.toggle_route = legacy.into();
        return config;
    }
    // JSON 既不是当前 schema 也不是已知旧格式时，不让损坏文件阻塞启动。
    tracing::warn!("config parse failed; using defaults");
    AppConfig::default()
}

/// 把旧版本提升到当前版本，并只补充当时必需的根分类。
fn migrate(mut config: AppConfig) -> AppConfig {
    // 迁移只处理明确已知的旧版本，避免覆盖新版本可能新增的用户数据。
    // 已是当前版本的配置保持用户主动删除的分类，不强行复活。
    if config.version < 3 {
        // 插入顺序固定为“全部”“未分类”，使分类树下标和界面初始顺序稳定。
        if !config.audio_categories.iter().any(|path| path == "全部") {
            config.audio_categories.insert(0, "全部".into());
        }
        if !config.audio_categories.iter().any(|path| path == "未分类") {
            config
                .audio_categories
                .insert(1.min(config.audio_categories.len()), "未分类".into());
        }
        // 版本号写回内存对象，下一次保存即可完成格式升级。
        // 仅更新内存对象；由调用方下一次 save 持久化，避免加载阶段产生隐式写盘。
        config.version = 3;
    }
    config
}

/// 写配置；失败只打日志，不影响音频运行。
/// 将完整配置写入 APPDATA；目录或序列化失败仅记录诊断，不中断音频线程。
pub(crate) fn save(config: &AppConfig) {
    // 先创建父目录，再一次性写入完整 JSON，避免目录缺失导致无意义的写盘尝试。
    let path = config_path();
    // 配置目录可能是首次创建，失败时无法安全继续写文件。
    if let Some(dir) = path.parent()
        && let Err(error) = fs::create_dir_all(dir)
    {
        tracing::warn!("create config directory: {error}");
        return;
    }
    // pretty JSON 便于用户诊断；末尾换行保持文本文件工具兼容。
    // 序列化在写文件前完成，失败时保留旧文件而不是写入半截内容。
    match serde_json::to_string_pretty(config) {
        Ok(body) => {
            if let Err(error) = fs::write(&path, format!("{body}\n")) {
                tracing::warn!("save config: {error}");
            }
        }
        Err(error) => tracing::warn!("serialize config: {error}"),
    }
}

/// 旧版根对象中的热键字段，供迁移到当前 `Hotkey` 表示。
#[derive(Deserialize)]
struct LegacyHotkey {
    /// RegisterHotKey 修饰符位掩码，1=Alt、2=Ctrl、4=Shift。
    mods: u32,
    /// Windows 虚拟键码，原样保留以避免迁移改变组合键。
    vk: u32,
}

/// 将旧热键值转换为当前配置类型，不改变位协议。
impl From<LegacyHotkey> for Hotkey {
    // 该转换是无损字段搬运；旧位协议由热键注册层继续解释。
    /// 复制旧字段，确保反序列化后的热键可直接注册。
    fn from(value: LegacyHotkey) -> Self {
        Self {
            mods: value.mods,
            vk: value.vk,
        }
    }
}

/// 只验证迁移与序列化契约，不触碰真实用户配置文件。
#[cfg(test)]
mod tests {
    use super::*;

    /// v2 配置应插入当前版本要求的两个根分类。
    #[test]
    fn v2_config_gains_default_category_roots() {
        let mut config = AppConfig::default();
        config.version = 2;
        config.audio_categories = vec!["游戏".into()];

        let migrated = migrate(config);

        assert_eq!(migrated.version, 3);
        assert_eq!(migrated.audio_categories, vec!["全部", "未分类", "游戏"]);
    }

    /// v3 配置尊重用户已经删除根分类的选择。
    #[test]
    fn v3_config_keeps_deleted_category_roots_deleted() {
        let mut config = AppConfig::default();
        config.audio_categories.clear();

        let migrated = migrate(config);

        assert!(migrated.audio_categories.is_empty());
    }

    /// 图标路径和分类映射在 JSON 往返后保持原样。
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
    /// 窗口坐标可往返保存，缺少旧字段时使用空位置。
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

/// 解析当前用户配置文件位置；APPDATA 缺失时退回工作目录。
// APPDATA 是用户级配置边界；缺失时使用当前目录仍能让便携/测试环境启动。
fn config_path() -> PathBuf {
    let base = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("Miccat").join("config.json")
}
