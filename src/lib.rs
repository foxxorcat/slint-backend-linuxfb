//! Slint 平台的 Linux Framebuffer (linuxfb) 后端
//!
//! 
pub mod error;
pub mod input;
pub mod pixels;
pub mod platform;
pub mod window;
pub mod linuxfb;

pub use error::Error;
pub use platform::{LinuxFbPlatform, LinuxFbPlatformBuilder};

/// 初始化 Slint 的 Linux Framebuffer 后端 (使用默认配置)。
///
/// 默认配置尝试打开 `/dev/fb0` 和 `/dev/tty1`，并自动发现输入设备。
/// 如需自定义，请使用 `LinuxFbPlatformBuilder`。
///
/// # 返回
/// 成功时返回 `Ok(())`，如果 framebuffer 无法打开或
/// 像素格式不受支持，则返回 `Err(Error)`。
pub fn init() -> Result<(), Error> {
    let platform = LinuxFbPlatform::new()?;
    i_slint_core::platform::set_platform(Box::new(platform))?;
    Ok(())
}