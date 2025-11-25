//! Interface to the Linux Framebuffer API
//!
//! This crate provides high-level access to a linux framebuffer device (`/dev/fb*`).
//!
//! Check out the [`Framebuffer`] documentation for a simple example.
//!
//! Once you are familiar with the basic interface, check out the [`double::Buffer`]
//! documentation, for some more examples.

extern crate libc;
extern crate memmap2;

pub mod double;
pub mod fbio;
mod proc;

use memmap2::{MmapMut, MmapOptions};
use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};

pub use self::fbio::{
    set_terminal_mode, BlankingLevel, ErrnoError, PixelLayout, PixelLayoutChannel, TerminalMode,
};

/// Errors returned by `Framebuffer` methods
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Framebuffer error: {0}")]
    Fb(#[from] fbio::ErrnoError),
}

/// Represents a single framebuffer device
///
/// Example usage:
///
/// ```no_run
/// // Instead of hardcoding the path, you could also use `Framebuffer::list()`
/// // to find paths to available devices.
/// let fb = linuxfb::Framebuffer::new("/dev/fb0").unwrap();
///
/// println!("Size in pixels: {:?}", fb.get_size());
///
/// println!("Bytes per pixel: {:?}", fb.get_bytes_per_pixel());
///
/// println!("Physical size in mm: {:?}", fb.get_physical_size());
///
/// // Map the framebuffer into memory, so we can write to it:
/// let mut data = fb.map().unwrap();
///
/// // Make everything black:
/// for i in 0..data.len() {
///   data[i] = 0;
/// }
///
/// // Make everything white:
/// for i in 0..data.len() {
///   data[i] = 0xFF;
/// }
/// ```
pub struct Framebuffer {
    pub file: File,
    pub finfo: fbio::FixScreeninfo,
    pub vinfo: fbio::VarScreeninfo,
}

impl Framebuffer {
    /// Returns a list of paths to device nodes, which are handled by the "fb" driver.
    ///
    /// Relies on `/proc/devices` to discover the major number of the device,
    /// and on the device nodes to exist in `/dev`.
    ///
    /// Example, assuming there is one framebuffer named `fb0`:
    ///
    ///     let devices = linuxfb::Framebuffer::list().unwrap();
    ///     println!("Devices: {:?}", devices);
    ///     // prints:
    ///     //   Devices: ["/dev/fb0"]
    ///
    pub fn list() -> std::io::Result<Vec<PathBuf>> {
        match proc::devices()?.find(|device| device.driver == "fb") {
            None => Ok(vec![]),
            Some(device) => Ok(std::fs::read_dir("/dev")?
                .flat_map(|result| match result {
                    Err(_) => None,
                    Ok(entry) => {
                        let mut statbuf: libc::stat = unsafe { std::mem::zeroed() };
                        let path = entry.path();
                        let cpath = std::ffi::CString::new(path.to_str().unwrap()).unwrap();
                        match unsafe { libc::stat(cpath.as_ptr(), &mut statbuf) } {
                            -1 => None,
                            _ => {
                                let major = unsafe { libc::major(statbuf.st_rdev) } as u32;
                                if major == device.major {
                                    Some(path)
                                } else {
                                    None
                                }
                            }
                        }
                    }
                })
                .collect()),
        }
    }

    /// Attempts to open the framebuffer device at the given `path` and query its properties.
    ///
    /// This operation can fail for one of the following reasons:
    /// * The device cannot be opened. In this case, the error will be the `Error::Io` variant,
    ///   which wraps a `std::io::Error` containing specific details. This may occur if the provided
    ///   `path` points to a non-existent file, or if the user lacks sufficient permissions
    ///   to open the device.
    /// * Any of the `ioctl` calls used to query device properties fails. In this case, the
    ///   error will be the `Error::Fb` variant, which wraps an `ErrnoError`. Use the `errno` and
    ///   `message` fields of the wrapped `ErrnoError` to determine the cause of the failure.
    ///   This may occur if the provided `path` does not reference a valid framebuffer device,
    ///   or if the device driver encounters an error.
    pub fn new(path: impl AsRef<Path>) -> Result<Framebuffer, Error> {
        let file = OpenOptions::new().read(true).write(true).open(path)?;
        let finfo = fbio::get_fscreeninfo(&file)?;
        let vinfo = fbio::get_vscreeninfo(&file)?;
        Ok(Framebuffer { file, finfo, vinfo })
    }

    /// Maps the framebuffer device into memory.
    ///
    /// Returns a memory mapped region, which can be used to modify screen contents.
    ///
    /// The size of the region is chosen based on the current virtual size of the display,
    /// and the bytes per pixel. Therefore this method should be called *after* configuring
    /// the device.
    ///
    /// Since the returned `memmap::MmapMut` object implements the `Drop` trait, the region is
    /// automatically unmapped, when the returned map goes out of scope.
    ///
    /// Note that changes to the data directly appear on screen, so you will most likely
    /// see flicker, if you write to a visible region.
    ///
    /// To avoid this, you can set the virtual size to be twice as large as the actual size
    /// of the display, then only draw to the part of that region that's currently not shown.
    /// Once done drawing, use `set_offset` to make the drawn region appear on screen.
    ///
    /// See the [`double`] module for a convenient wrapper that does exactly that.
    pub fn map(&self) -> Result<MmapMut, Error> {
        let (width, height) = self.get_virtual_size();
        let size = width * height * self.get_bytes_per_pixel();
        let mmap = unsafe { MmapOptions::new().len(size as usize).map_mut(&self.file) }?;
        Ok(mmap)
    }

    /// Returns the number of bytes used to represent one pixel.
    ///
    /// This can be used to narrow down the format.
    pub fn get_bytes_per_pixel(&self) -> u32 {
        self.vinfo.bytes_per_pixel()
    }

    /// Sets the number of bytes per pixel.
    ///
    /// This modifies the `bits_per_pixel` attribute of the underlying
    /// device.
    /// The actual consequence of this action depends on the driver.
    ///
    /// For at least some drivers, setting a different number of pixels
    /// changes the color mode.
    ///
    /// Make sure to use [`get_bytes_per_pixel`](Framebuffer::get_bytes_per_pixel) afterwards to check if
    /// the value was changed.
    ///
    /// Also use [`get_pixel_layout`](Framebuffer::get_pixel_layout), to find out more about the format
    /// being used.
    ///
    /// This operation fails, when any of the underlying `ioctl` calls fail.
    /// After a failure, the device may be in an undefined state.
    pub fn set_bytes_per_pixel(&mut self, value: u32) -> Result<(), Error> {
        let mut vinfo = self.vinfo.clone();
        vinfo.set_bytes_per_pixel(value);
        vinfo.activate_now();
        fbio::put_vscreeninfo(&self.file, &mut vinfo)?;
        self.vinfo = fbio::get_vscreeninfo(&self.file)?;
        Ok(())
    }

    /// Returns the pixel layout, as reported by the driver.
    ///
    /// This value may change, after calling [`set_bytes_per_pixel`](Framebuffer::set_bytes_per_pixel).
    ///
    /// Some examples:
    ///
    /// **16-bit, RGB565**, meaning `rrrrrggggggrrrrr`, with LSB right, aka HighColor:
    /// ```
    /// # use linuxfb::*;
    /// PixelLayout {
    ///   red: PixelLayoutChannel { offset: 11, length: 5, msb_right: false },
    ///   green: PixelLayoutChannel { offset: 5, length: 6, msb_right: false },
    ///   blue: PixelLayoutChannel { offset: 0, length: 5, msb_right: false },
    ///   alpha: PixelLayoutChannel { offset: 0, length: 0, msb_right: false },
    /// };
    /// ```
    ///
    /// **32-bit, ABGR**, meaning `aaaaaaaabbbbbbbbbggggggggrrrrrrrr`, with LSB right:
    /// ```
    /// # use linuxfb::*;
    /// PixelLayout {
    ///   red: PixelLayoutChannel { offset: 0, length: 8, msb_right: false },
    ///   green: PixelLayoutChannel { offset: 8, length: 8, msb_right: false },
    ///   blue: PixelLayoutChannel { offset: 16, length: 8, msb_right: false },
    ///   alpha: PixelLayoutChannel { offset: 24, length: 8, msb_right: false },
    /// };
    /// ```
    ///
    /// **32-bit, RGBA**, meaning: `rrrrrrrrggggggggbbbbbbbbaaaaaaaa`, with LSB right:
    /// ```
    /// # use linuxfb::*;
    /// PixelLayout {
    ///   red: PixelLayoutChannel { offset: 24, length: 8, msb_right: false },
    ///   green: PixelLayoutChannel { offset: 16, length: 8, msb_right: false },
    ///   blue: PixelLayoutChannel { offset: 8, length: 8, msb_right: false },
    ///   alpha: PixelLayoutChannel { offset: 0, length: 8, msb_right: false },
    /// };
    /// ```
    ///
    /// Note that on most devices, setting alpha data does not have any effect, even
    /// when an alpha channel is specified in the layout.
    pub fn get_pixel_layout(&self) -> fbio::PixelLayout {
        self.vinfo.pixel_layout()
    }

    /// Returns the size of the display, in pixels.
    pub fn get_size(&self) -> (u32, u32) {
        self.vinfo.size_in_pixels()
    }

    /// Returns the virtual size of the display, in pixels.
    ///
    /// See `set_virtual_size` for details.
    pub fn get_virtual_size(&self) -> (u32, u32) {
        self.vinfo.virtual_size()
    }

    /// Sets the virtual size of the display.
    ///
    /// The virtual size defines the area where pixel data can be written to.
    /// It should always be equal to or larger than the values returned from
    /// [`get_size`](Framebuffer::get_size).
    ///
    /// After setting the virtual size, you can use [`set_offset`](Framebuffer::set_offset)
    /// to control what region of the "virtual display" is actually shown on screen.
    ///
    /// This operation fails, when any of the underlying `ioctl` calls fail.
    /// After a failure, the device may be in an undefined state.
    pub fn set_virtual_size(&mut self, w: u32, h: u32) -> Result<(), Error> {
        let mut vinfo = self.vinfo.clone();
        vinfo.set_virtual_size(w, h);
        vinfo.activate_now();
        fbio::put_vscreeninfo(&self.file, &mut vinfo)?;
        self.vinfo = fbio::get_vscreeninfo(&self.file)?;
        Ok(())
    }

    /// Returns the current `xoffset` and `yoffset` of the underlying device.
    pub fn get_offset(&self) -> (u32, u32) {
        self.vinfo.offset()
    }

    /// Sets the `xoffset` and `yoffset` of the underlying device.
    ///
    /// This can be used to pan the display.
    ///
    /// This operation fails, when any of the underlying `ioctl` calls fail.
    /// After a failure, the device may be in an undefined state.
    pub fn set_offset(&mut self, x: u32, y: u32) -> Result<(), Error> {
        let mut vinfo = self.vinfo.clone();
        vinfo.set_offset(x, y);
        vinfo.activate_now();
        fbio::put_vscreeninfo(&self.file, &mut vinfo)?;
        self.vinfo = fbio::get_vscreeninfo(&self.file)?;
        Ok(())
    }

    /// Returns the physical size of the device
    /// in millimeters, as reported by the driver.
    pub fn get_physical_size(&self) -> (u32, u32) {
        self.vinfo.size_in_mm()
    }

    /// Get identifier string of the device, as reported by the driver.
    pub fn get_id(&self) -> String {
        self.finfo.id()
    }

    /// Sets the blanking level. This can be used to turn off the screen.
    ///
    /// See [`BlankingLevel`] for a list of available options, and their
    /// meaning.
    ///
    /// Brief example:
    /// ```no_run
    /// use linuxfb::{Framebuffer, BlankingLevel};
    ///
    /// let mut fb = Framebuffer::new("/dev/fb0").unwrap();
    ///
    /// // Turn off the screen:
    /// fb.blank(BlankingLevel::Powerdown).unwrap();
    ///
    /// // Turn the screen back on:
    /// fb.blank(BlankingLevel::Unblank).unwrap();
    /// ```
    ///
    /// This operation fails, when the underlying `ioctl` call fails.
    ///
    /// At least some drivers produce an error, when the new blanking
    /// mode does not actually change the state of the device.
    ///
    /// For example:
    /// ```no_run
    /// # use linuxfb::{Framebuffer, BlankingLevel};
    /// # let mut fb = Framebuffer::new("/dev/fb0").unwrap();
    /// fb.blank(BlankingLevel::Powerdown).unwrap(); // this call goes through fine
    /// fb.blank(BlankingLevel::Powerdown).unwrap(); // this call fails with EBUSY
    /// ```
    ///
    /// Since there is no way to determine beforehand what the current
    /// state of the screen is, you should always expect that these calls
    /// may fail, and continue normally (possibly with a warning).
    pub fn blank(&self, level: BlankingLevel) -> Result<(), Error> {
        fbio::blank(&self.file, level)?;
        Ok(())
    }

    /// 等待垂直同步 (Vertical Sync)。
    /// 这是一个阻塞调用，直到下一次垂直消隐开始时返回。
    pub fn wait_for_vsync(&self) -> Result<(), Error> {
        fbio::wait_for_vsync(&self.file)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        println!("Framebuffer devices: {:?}", crate::linuxfb::Framebuffer::list());
    }
}
