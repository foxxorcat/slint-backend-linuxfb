//! Double-buffered interface to a framebuffer
//!
//! See [`Buffer`] for an example.

use super::{Framebuffer, Error, BlankingLevel};
use memmap2::MmapMut;

#[derive(Debug)]
enum State {
    DrawToFirst,
    DrawToSecond,
}

impl State {
    fn flip(&mut self) -> &Self {
        *self = match self {
            State::DrawToFirst => State::DrawToSecond,
            State::DrawToSecond => State::DrawToFirst,
        };
        self
    }
}

/// Double-buffered interface to a framebuffer
///
/// ```no_run
/// let mut fb = linuxfb::Framebuffer::new("/dev/fb0").unwrap();
/// // Do any custom setup on the framebuffer here, such as
/// // setting bytes_per_pixel.
///
/// // The double::Buffer will take ownership of the framebuffer, and
/// // configure it's virtual_size to be twice the actual size.
/// let mut buffer = linuxfb::double::Buffer::new(fb).unwrap();
///
/// // These values represent the size of a single buffer,
/// // which is equivalent to the screen resolution:
/// let width = buffer.width as usize;
/// let height = buffer.height as usize;
///
/// // Retrieve a slice for the current backbuffer:
/// let frame: &mut[u8] = buffer.as_mut_slice();
///
/// // Write pixel data:
/// for i in 0..frame.len() {
///   frame[i] = 0;
/// }
///
/// // Flip the display, to make the data appear on screen:
/// buffer.flip().unwrap();
///
/// // The `frame` reference acquired above is no longer valid
/// // (it now points to the front buffer), so we need to get
/// // a new one:
///
/// let frame: &mut[u8] = buffer.as_mut_slice();
///
/// // Writing byte-wise is neither very efficient, nor convenient.
/// // To write whole pixels, we can cast our buffer to the right
/// // format (u32 in this case):
/// let (prefix, pixels, suffix) = unsafe { frame.align_to_mut::<u32>() };
///
/// // Since we are using a type that can hold a whole pixel, it should
/// // always align nicely.
/// // Thus there is no prefix or suffix here:
/// assert_eq!(prefix.len(), 0);
/// assert_eq!(suffix.len(), 0);
///
/// // Now we can start filling the pixels:
/// for y in 0..height {
///   for x in 0..width {
///     pixels[x + y * width] = 0xFF00FFFF; // magenta, assuming 32-bit RGBA
///   }
/// }
///
/// // Finally flip the buffer again, to make the changes visible on screen:
/// buffer.flip().unwrap();
/// ```
///
pub struct Buffer {
    pub width: u32,
    pub height: u32,
    fb: Framebuffer,
    map: MmapMut,
    state: State,
}

impl Buffer {
    /// Create a new Buffer object, backed by the given framebuffer.
    ///
    /// Initializes the virtual size and the offset of the buffer.
    ///
    /// Takes ownership of the framebuffer, so any other modifications
    /// to the framebuffer's state need to be done beforehand.
    ///
    /// Usually, after initialization the offset will be set to `(0, 0)`,
    /// and the first frame will be drawn into the backbuffer at `(0, height)`.
    /// However, when the offset of the framebuffer is already set to `(0, height)`,
    /// it is left like that and the initial backbuffer is at `(0, 0)`.
    /// This behavior prevents the display from showing an old, retained image
    /// between the call to `new` and the first call to [`flip`].
    pub fn new(mut fb: Framebuffer) -> Result<Self, Error> {
        let (width, height) = fb.get_size();
        let (virtual_width, virtual_height) = fb.get_virtual_size();
        if virtual_width != width || virtual_height != (height * 2) {
            fb.set_virtual_size(width, height * 2)?;
        }
        let (offset_x, mut offset_y) = fb.get_offset();
        if offset_x != 0 || (offset_y != 0 && offset_y != height) {
            fb.set_offset(0, 0)?;
            offset_y = 0;
        }
        let map = fb.map()?;
        let state = if offset_y == height {
            State::DrawToFirst
        } else {
            State::DrawToSecond
        };
        Ok(Self { width, height, fb, map, state })
    }

    /// Returns a mutable slice to the current backbuffer.
    ///
    /// Changes to this slice will not end up on screen,
    /// until [`flip`] is called.
    ///
    /// The slice has a length of `width * height * bytes_per_pixel`,
    /// where `width` and `height` are equal to the screen resolution,
    /// and `bytes_per_pixel` is equal to the value returned from [`Framebuffer::get_bytes_per_pixel`]
    pub fn as_mut_slice(&mut self) -> &mut[u8] {
        let page_size = (self.fb.get_bytes_per_pixel() * self.height * self.width) as usize;
        let (start, end) = match self.state {
            State::DrawToFirst => (0, page_size),
            State::DrawToSecond => (page_size, page_size * 2),
        };
        &mut self.map[start..end]
    }

    /// Flips the display, by exchanging 
    pub fn flip(&mut self) -> Result<(), Error> {
        match self.state.flip() {
            State::DrawToFirst => self.fb.set_offset(0, self.height),
            State::DrawToSecond => self.fb.set_offset(0, 0),
        }
    }

    /// Calls [`blank`](Framebuffer::blank) on the underlying Framebuffer
    pub fn blank(&self, level: BlankingLevel) -> Result<(), Error>{
        self.fb.blank(level)
    }

    /// Calls [`wait_for_vsync`](Framebuffer::blank) on the underlying Framebuffer
    pub fn wait_for_vsync(&self) -> Result<(), Error> {
        self.fb.wait_for_vsync()
    }
}
