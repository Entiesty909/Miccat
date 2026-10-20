//! 调试开关中心：按组件分类的日志级别，运行时可调并持久化。
//!
//! 调用点直接用 `tracing::debug!` 等宏，由本模块安装的过滤器决定是否输出；
//! 不需要在每个方法里手写 `if switch { ... }` 判断。只有当日志字段本身很贵时，
//! 才用 `tracing::enabled!(Level::DEBUG)` 先挡一层。

use std::collections::BTreeMap;
use std::sync::{Arc, RwLock};

use tracing_subscriber::EnvFilter;

/// 调试分类的日志级别。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DebugLevel {
    /// 关闭该类日志。
    Off,
    /// 只输出错误。
    Error,
    /// 输出警告及以上。
    Warn,
    /// 输出信息及以上（默认）。
    Info,
    /// 输出调试及以上。
    Debug,
    /// 输出全部跟踪。
    Trace,
}

/// 将诊断级别映射为持久化编码、过滤器文本，并提供反向解析。
impl DebugLevel {
    /// 持久化与 UI 使用的整数编码。
    pub(crate) fn code(self) -> i32 {
        match self {
            Self::Off => 0,
            Self::Error => 1,
            Self::Warn => 2,
            Self::Info => 3,
            Self::Debug => 4,
            Self::Trace => 5,
        }
    }

    /// 从持久化编码还原；未知值回退 `Info`。
    pub(crate) fn from_code(code: i32) -> Self {
        match code {
            0 => Self::Off,
            1 => Self::Error,
            2 => Self::Warn,
            4 => Self::Debug,
            5 => Self::Trace,
            _ => Self::Info,
        }
    }

    /// EnvFilter 指令值。
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Error => "error",
            Self::Warn => "warn",
            Self::Info => "info",
            Self::Debug => "debug",
            Self::Trace => "trace",
        }
    }
}

/// 调试分类：UI 一项开关，映射到一组 tracing 模块 target 前缀。
pub(crate) struct Category {
    /// 持久化键。
    pub(crate) key: &'static str,
    /// 中文显示名。
    pub(crate) label: &'static str,
    /// tracing target 前缀；EnvFilter 按模块前缀匹配。
    pub(crate) targets: &'static [&'static str],
}

/// 可在界面单独调节的调试分类。
pub(crate) const CATEGORIES: &[Category] = &[
    Category {
        key: "capture",
        label: "音频捕获",
        targets: &["mintcat_proc_mic::audio::capture"],
    },
    Category {
        key: "render",
        label: "音频渲染",
        targets: &["mintcat_proc_mic::audio::render"],
    },
    Category {
        key: "playback",
        label: "播放与解码",
        targets: &[
            "mintcat_proc_mic::audio::decode",
            "mintcat_proc_mic::audio::preview",
            "mintcat_proc_mic::audio::sfx",
            "mintcat_proc_mic::engine::playback",
        ],
    },
    Category {
        key: "device",
        label: "音频设备",
        targets: &[
            "mintcat_proc_mic::audio::device",
            "mintcat_proc_mic::audio::format",
            "mintcat_proc_mic::audio::policy",
        ],
    },
    Category {
        key: "engine",
        label: "音频引擎",
        targets: &["mintcat_proc_mic::engine"],
    },
    Category {
        key: "config",
        label: "配置",
        targets: &["mintcat_proc_mic::config"],
    },
    Category {
        key: "hotkey",
        label: "热键",
        targets: &["mintcat_proc_mic::hotkey"],
    },
    Category {
        key: "process",
        label: "进程",
        targets: &["mintcat_proc_mic::process"],
    },
    Category {
        key: "ui",
        label: "界面",
        targets: &["mintcat_proc_mic::ui_bridge"],
    },
    Category {
        key: "cable",
        label: "虚拟线缆",
        targets: &["mintcat_proc_mic::vbcable"],
    },
    Category {
        key: "platform",
        label: "平台/COM",
        targets: &["mintcat_proc_mic::platform"],
    },
];

/// 当前生效的日志级别集合。
#[derive(Clone, Debug)]
pub(crate) struct Levels {
    /// 未单独设置的分类使用的默认级别。
    pub(crate) master: DebugLevel,
    /// 分类键到级别的覆盖。
    pub(crate) overrides: BTreeMap<String, DebugLevel>,
}

// 使用信息级别作为默认值，启动时保留必要日志且不放大分类噪声。

/// 为未单独设置分类的日志使用信息级别默认值。
impl Default for Levels {
    /// 创建使用 Info 级别且没有分类覆盖的初始日志配置。
    fn default() -> Self {
        Self {
            master: DebugLevel::Info,
            overrides: BTreeMap::new(),
        }
    }
}

/// 负责把持久化级别转换为 EnvFilter 指令，并维护分类覆盖的顺序。

impl Levels {
    /// 从持久化的编码构建；未知分类键保留，未知级别回退 `Info`。
    pub(crate) fn from_codes(master: i32, categories: &BTreeMap<String, i32>) -> Self {
        Self {
            master: DebugLevel::from_code(master),
            overrides: categories
                .iter()
                .map(|(key, code)| (key.clone(), DebugLevel::from_code(*code)))
                .collect(),
        }
    }
    /// 拼出 EnvFilter 指令串；master 在前，分类覆盖在后。
    pub(crate) fn filter_string(&self) -> String {
        // 先放全局级别，再追加更具体的 target 覆盖，让 EnvFilter 按优先级解析。

        let mut parts = vec![self.master.as_str().to_string()];
        // 每个分类可能覆盖多个模块，因此展开为一条条 target=level 指令。

        for category in CATEGORIES {
            if let Some(level) = self.overrides.get(category.key) {
                for target in category.targets {
                    parts.push(format!("{target}={}", level.as_str()));
                }
            }
        }
        parts.join(",")
    }
}

/// 运行时调试开关；重载底层 EnvFilter 不重启进程。
pub(crate) struct Control {
    /// 当前级别快照；读写锁让 UI 调整与日志线程读取互不破坏。
    levels: Arc<RwLock<Levels>>,
    /// 由安装阶段注入的重载回调；返回值表示底层过滤器是否接受新配置。
    reload: Box<dyn Fn(EnvFilter) -> bool + Send + Sync>,
}

/// 通过共享级别快照协调 UI 更新与 tracing 过滤器重载。

impl Control {
    /// 用初始级别与过滤器重载句柄创建。
    pub(crate) fn new(
        levels: Levels,
        reload: Box<dyn Fn(EnvFilter) -> bool + Send + Sync>,
    ) -> Self {
        Self {
            levels: Arc::new(RwLock::new(levels)),
            reload,
        }
    }

    /// 设置默认级别并立即生效。
    pub(crate) fn set_master(&self, level: DebugLevel) {
        self.update(|levels| levels.master = level);
    }

    /// 设置某分类级别；`None` 表示恢复默认。
    pub(crate) fn set_category(&self, key: &str, level: Option<DebugLevel>) {
        self.update(|levels| match level {
            Some(level) => {
                levels.overrides.insert(key.to_string(), level);
            }
            None => {
                levels.overrides.remove(key);
            }
        });
    }

    /// 在锁内更新权威状态并生成过滤器；释放锁后再回调，避免重载路径重入锁。

    fn update(&self, change: impl FnOnce(&mut Levels)) {
        let filter = {
            let Ok(mut levels) = self.levels.write() else {
                // 锁中毒时放弃本次调整，避免诊断设置路径再次触发 panic。
                return;
            };
            change(&mut levels);
            EnvFilter::try_new(levels.filter_string()).ok()
        };
        // 过滤器构建成功后才替换运行中的实例；失败时保留旧过滤器继续工作。
        if let Some(filter) = filter {
            (self.reload)(filter);
        }
    }
}
