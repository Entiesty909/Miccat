//! 跨层 Win32/COM 基础设施；封装窗口、图标、剪贴板和线程公寓的系统资源边界。

pub(crate) mod clipboard;
pub(crate) mod com;
pub(crate) mod icon;
pub(crate) mod theme;
pub(crate) mod window;
