//! This module wraps the framebuffer API's `ioctl` calls.
//! It uses a generated binding, based on the `<linux/fb.h>` header.

#![allow(non_camel_case_types)]
include!(concat!(env!("OUT_DIR"), "/bindings.rs"));

// 手动定义 FBIO_WAITFORVSYNC
// _IOW('F', 0x20, __u32)
// 在大多数架构 (x86, ARM, AArch64) 上:
// Dir(2bit=01) | Size(14bit=4) | Type(8bit='F'=0x46) | Nr(8bit=0x20)
// = 0x40044620
const FBIO_WAITFORVSYNC: u32 = 0x40044620;

use std::default::Default;
use std::os::unix::io::AsRawFd;

/// Represents an error read from the libc global `errno`
///
/// These errors are returned, when `ioctl` or other wrapped
/// libc calls fail.
#[derive(Debug, thiserror::Error)]
#[error("FBIoError {errno}: {message}")] 
pub struct ErrnoError {
    /// Error number. Compare this with the `libc::E*` constants,
    /// to handle specific errors.
    ///
    /// e.g.:
    /// ```no_run
    /// # let error = linuxfb::ErrnoError { errno: libc::EBUSY, message: String::from("") };
    /// error.errno == libc::EBUSY; // true
    /// println!("{}", error.message); // prints "Resource busy" (on my system)
    /// ```
    pub errno: i32,
    /// Message produced by `strerror(errno)`. This value varies
    /// based on the user's locale, so do not use it for comparisons.
    pub message: String,
}

impl ErrnoError {
    fn new() -> Self {
        let errno = unsafe { *libc::__errno_location() };
        let message_c = unsafe { std::ffi::CStr::from_ptr(libc::strerror(errno)) };
        let message = String::from(message_c.to_str().unwrap());
        Self { errno, message }
    }
}

#[derive(Debug, PartialEq, Clone)]
pub struct PixelLayoutChannel {
    /// Start of data, in bits
    pub offset: u32,
    /// Size of data, in bits
    pub length: u32,
    /// When true, the most significant bit is on the right.
    pub msb_right: bool,
}

impl From<fb_bitfield> for PixelLayoutChannel {
    fn from(bitfield: fb_bitfield) -> Self {
        Self {
            offset: bitfield.offset,
            length: bitfield.length,
            msb_right: bitfield.msb_right != 0,
        }
    }
}

#[derive(Debug, PartialEq, Clone)]
pub struct PixelLayout {
    pub red: PixelLayoutChannel,
    pub green: PixelLayoutChannel,
    pub blue: PixelLayoutChannel,
    pub alpha: PixelLayoutChannel,
}

#[derive(Default, Clone)]
pub struct VarScreeninfo {
    pub internal: fb_var_screeninfo,
}

impl VarScreeninfo {
    pub fn size_in_pixels(&self) -> (u32, u32) {
        (self.internal.xres, self.internal.yres)
    }

    pub fn size_in_mm(&self) -> (u32, u32) {
        (self.internal.width, self.internal.height)
    }

    pub fn bytes_per_pixel(&self) -> u32 {
        self.internal.bits_per_pixel / 8
    }

    pub fn pixel_layout(&self) -> PixelLayout {
        PixelLayout {
            red: PixelLayoutChannel::from(self.internal.red),
            green: PixelLayoutChannel::from(self.internal.green),
            blue: PixelLayoutChannel::from(self.internal.blue),
            alpha: PixelLayoutChannel::from(self.internal.transp),
        }
    }

    pub fn set_bytes_per_pixel(&mut self, value: u32) {
        self.internal.bits_per_pixel = value * 8;
    }

    pub fn virtual_size(&self) -> (u32, u32) {
        (self.internal.xres_virtual, self.internal.yres_virtual)
    }

    pub fn set_virtual_size(&mut self, width: u32, height: u32) {
        self.internal.xres_virtual = width;
        self.internal.yres_virtual = height;
    }

    pub fn offset(&self) -> (u32, u32) {
        (self.internal.xoffset, self.internal.yoffset)
    }

    pub fn set_offset(&mut self, x: u32, y: u32) {
        self.internal.xoffset = x;
        self.internal.yoffset = y;
    }

    pub fn activate_now(&mut self) {
        self.internal.activate = FB_ACTIVATE_NOW;
    }
}

#[derive(Default, Clone)]
pub struct FixScreeninfo {
    pub internal: fb_fix_screeninfo,
}

impl FixScreeninfo {
    pub fn id(&self) -> String {
        let c_string = unsafe { std::ffi::CStr::from_ptr(self.internal.id.as_ptr()) };
        String::from(c_string.to_str().unwrap())
    }
}

/// Wrapper around `ioctl(fd, FBIOGET_VSCREENINFO, ...)`.
pub fn get_vscreeninfo(file: &impl AsRawFd) -> Result<VarScreeninfo, ErrnoError> {
    let mut vinfo: fb_var_screeninfo = Default::default();
    match unsafe { libc::ioctl(file.as_raw_fd(), FBIOGET_VSCREENINFO as _, &mut vinfo) } {
        -1 => Err(ErrnoError::new()),
        _ => Ok(VarScreeninfo { internal: vinfo }),
    }
}

/// Wrapper around `ioctl(fd, FBIOPUT_VSCREENINFO, ...)`.
pub fn put_vscreeninfo(
    file: &impl AsRawFd,
    var_screeninfo: &mut VarScreeninfo,
) -> Result<(), ErrnoError> {
    let mut vinfo = var_screeninfo.internal;
    match unsafe { libc::ioctl(file.as_raw_fd(), FBIOPUT_VSCREENINFO as _, &mut vinfo) } {
        -1 => Err(ErrnoError::new()),
        _ => Ok(()),
    }
}

/// Wrapper around `ioctl(fd, FBIOGET_FSCREENINFO, ...)`.
pub fn get_fscreeninfo(file: &impl AsRawFd) -> Result<FixScreeninfo, ErrnoError> {
    let mut finfo: fb_fix_screeninfo = Default::default();
    match unsafe { libc::ioctl(file.as_raw_fd(), FBIOGET_FSCREENINFO as _, &mut finfo) } {
        -1 => Err(ErrnoError::new()),
        _ => Ok(FixScreeninfo { internal: finfo }),
    }
}

/// Wrapper around `ioctl(fd, FBIO_WAITFORVSYNC, ...)`.
///
/// Blocks until the next vertical blanking interval.
pub fn wait_for_vsync(file: &impl AsRawFd) -> Result<(), ErrnoError> {
    let mut dummy: u32 = 0;
    match unsafe { libc::ioctl(file.as_raw_fd(), FBIO_WAITFORVSYNC as _, &mut dummy) } {
        -1 => Err(ErrnoError::new()),
        _ => Ok(()),
    }
}


/// Represents a screen blanking level
///
/// See [`Framebuffer::blank`] for usage.
///
/// Note that not all drivers support all of these modes.
/// In particular the `VsyncSuspend` and `HsyncSuspend` values
/// may not be supported, in which case `Normal` behaves
/// exactly the same as `Powerdown`.
#[derive(Debug, Clone)]
pub enum BlankingLevel {
    /// Undoes any blank, and turns the screen back on.
    /// Note that the picture is usually not retained while
    /// in blank mode, so you need to redraw everything after
    /// unblanking.
    Unblank,
    /// Blanks the screen, but leaves hsync/vsync running.
    Normal,
    /// Like Normal, but additionally suspends vsync
    VsyncSuspend,
    /// Like Normal, but additionally suspends hsync
    HsyncSuspend,
    /// Blanks the screen and powers down sync circuitry as well.
    Powerdown,
}

impl BlankingLevel {
    fn to_ulong(&self) -> std::os::raw::c_ulong {
        match self {
            BlankingLevel::Unblank => FB_BLANK_UNBLANK,
            BlankingLevel::Normal => FB_BLANK_NORMAL,
            BlankingLevel::VsyncSuspend => FB_BLANK_VSYNC_SUSPEND,
            BlankingLevel::HsyncSuspend => FB_BLANK_HSYNC_SUSPEND,
            BlankingLevel::Powerdown => FB_BLANK_POWERDOWN,
        }
        .into()
    }
}

pub fn blank(file: &impl AsRawFd, level: BlankingLevel) -> Result<(), ErrnoError> {
    match unsafe { libc::ioctl(file.as_raw_fd(), FBIOBLANK as _, level.to_ulong()) } {
        -1 => Err(ErrnoError::new()),
        _ => Ok(()),
    }
}

#[derive(Copy, Clone)]
pub enum TerminalMode {
    Text,
    Graphics,
}

impl TerminalMode {
    fn to_ulong(&self) -> std::os::raw::c_ulong {
        match self {
            TerminalMode::Text => KD_TEXT,
            TerminalMode::Graphics => KD_GRAPHICS,
        }
        .into()
    }
}

/// Switch the terminal into desired mode.
///
/// There are two modes: "text" and "graphics". In text mode, console
/// output will be drawn to the terminal by the fbcon driver.
/// In graphics mode it will not.
///
/// The given `tty` must refer to a real terminal (`/dev/tty*`).
///
/// When switching to graphics mode, make sure to switch back to text mode whenever the application exits.
/// Otherwise the terminal will appear to be "stuck", since no output will be shown.
///
/// Example:
/// ```no_run
/// # use linuxfb::{set_terminal_mode, TerminalMode};
/// let tty = std::fs::File::open("/dev/tty1").unwrap();
/// set_terminal_mode(&tty, TerminalMode::Graphics);
/// ```
pub fn set_terminal_mode(tty: &impl AsRawFd, mode: TerminalMode) -> Result<(), ErrnoError> {
    match unsafe { libc::ioctl(tty.as_raw_fd(), KDSETMODE as _, mode.to_ulong()) } {
        -1 => Err(ErrnoError::new()),
        _ => Ok(()),
    }
}
