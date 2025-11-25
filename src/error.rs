//! 定义库的统一错误类型。

use i_slint_core::api::PlatformError;
use thiserror::Error;

/// `slint-linuxfb` 后端的主错误类型。
#[derive(Debug, Error)]
pub enum Error {
    /// 封装了来自 `rust-linuxfb` 库的 I/O 或 ioctl 错误。
    #[error("Linux Framebuffer 错误: {0}")]
    LinuxFb(#[from] crate::linuxfb::Error),

    /// 封装了来自 Slint 核心的平台错误（例如设置平台失败）。
    #[error("Slint 平台错误: {0}")]
    SlintPlatform(#[from] i_slint_core::api::PlatformError),
    #[error("Slint 平台设置错误: {0}")]
    SetPlatformError(#[from] i_slint_core::platform::SetPlatformError),

    /// 当 framebuffer 的像素格式不是我们支持的格式之一时返回。
    #[error("不支持的 Framebuffer 像素格式")]
    UnsupportedPixelFormat,

    /// 兜底的其他错误。
    #[error("后端错误: {0}")]
    Other(String),
}

impl Into<PlatformError> for Error {
    fn into(self) -> PlatformError {
        PlatformError::Other(self.to_string())
    }
}