use crate::error::Error;
use crate::pixels::{PixelAbgr8888, PixelBgra8888, PixelFormat, PixelRgb565, PixelRgba8888};
use i_slint_core::platform::{software_renderer::SoftwareRenderer, WindowAdapter};
use crate::linuxfb::double;
use std::cell::RefCell;
use std::rc::Rc;

pub struct LinuxFbWindowAdapter {
    pub window: Rc<i_slint_core::api::Window>,
    pub fb_buffer: RefCell<double::Buffer>,
    pub renderer: SoftwareRenderer,
    pub pixel_format: PixelFormat,
    pub needs_redraw: RefCell<bool>,
}

impl LinuxFbWindowAdapter {
    /// 负责在 `draw_if_needed` 闭包中实际执行渲染
    /// 它在运行时分发到正确的 TargetPixel 实现
    pub fn render_frame(&self, renderer: &SoftwareRenderer) -> Result<(), Error> {
        // 1. 获取 fb_buffer 的可变借用
        let mut fb_buffer = self.fb_buffer.borrow_mut();

        // 2. 获取所有不可变属性 (stride)
        //    stride 是像素数量，不是字节数
        let stride = fb_buffer.width as usize;

        // 3. 获取可变切片
        let mmap_slice: &mut [u8] = fb_buffer.as_mut_slice();

        // 4. 运行时分发到正确的 TargetPixel 实现
        match self.pixel_format {
            PixelFormat::Abgr8888 => {
                let pixel_slice: &mut [PixelAbgr8888] = bytemuck::cast_slice_mut(mmap_slice);
                renderer.render(pixel_slice, stride);
            }
            PixelFormat::Rgba8888 => {
                let pixel_slice: &mut [PixelRgba8888] = bytemuck::cast_slice_mut(mmap_slice);
                renderer.render(pixel_slice, stride);
            }
            PixelFormat::Bgra8888 => {
                let pixel_slice: &mut [PixelBgra8888] = bytemuck::cast_slice_mut(mmap_slice);
                renderer.render(pixel_slice, stride);
            }
            PixelFormat::Rgb565 => {
                let pixel_slice: &mut [PixelRgb565] = bytemuck::cast_slice_mut(mmap_slice);
                renderer.render(pixel_slice, stride);
            }
            _ => return Err(Error::UnsupportedPixelFormat),
        }

        Ok(())
    }
}

impl WindowAdapter for LinuxFbWindowAdapter {
    fn window(&self) -> &i_slint_core::api::Window {
        &self.window
    }

    fn renderer(&self) -> &dyn i_slint_core::renderer::Renderer {
        &self.renderer
    }

    fn request_redraw(&self) {
        *self.needs_redraw.borrow_mut() = true;
    }

    fn size(&self) -> i_slint_core::api::PhysicalSize {
        let fb = self.fb_buffer.borrow();
        i_slint_core::api::PhysicalSize::new(fb.width, fb.height)
    }
}