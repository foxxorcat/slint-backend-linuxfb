//! 像素格式定义和 TargetPixel 实现。
//!
//! 负责将 Slint 的 RGBA 颜色数据转换并混合到底层 Framebuffer 的特定格式中。

use i_slint_core::platform::software_renderer::{PremultipliedRgbaColor, TargetPixel};
use crate::linuxfb::fbio;

/// 支持的 Framebuffer 像素格式
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PixelFormat {
    /// 32-bpp ABGR 格式 (Alpha在最高位, 内存序: BB GG RR AA)
    Abgr8888,
    /// 32-bpp RGBA 格式 (内存序: RR GG BB AA)
    Rgba8888,
    /// 32-bpp BGRA 格式 (常用于桌面系统, 内存序: BB GG RR AA)
    Bgra8888,
    /// 16-bpp RGB565 格式 (嵌入式常用)
    Rgb565,
    /// 未知或不支持的格式
    Unknown,
}

impl PixelFormat {
    /// 根据 fb_var_screeninfo 检测像素格式
    pub fn from_fb_info(vinfo: &fbio::VarScreeninfo) -> Self {
        let layout = vinfo.pixel_layout();
        match vinfo.internal.bits_per_pixel {
            32 => {
                // 32位格式判断逻辑：根据各颜色通道的偏移量(offset)推断
                if layout.alpha.offset == 24 {
                    // Alpha 在高位
                    if layout.red.offset == 0 && layout.green.offset == 8 && layout.blue.offset == 16 {
                         // Offset 0=Red, 8=Green, 16=Blue, 24=Alpha -> RGBA (小端序下)
                        PixelFormat::Rgba8888
                    } else if layout.blue.offset == 0 && layout.green.offset == 8 && layout.red.offset == 16 {
                        // Offset 0=Blue, 8=Green, 16=Red, 24=Alpha -> ABGR (某些特定的嵌入式控制器)
                        PixelFormat::Abgr8888
                    } else {
                        tracing::warn!(
                            "不支持的 32-bpp 布局 (Alpha@24): R={}, G={}, B={}",
                            layout.red.offset, layout.green.offset, layout.blue.offset
                        );
                        PixelFormat::Unknown
                    }
                } else if layout.alpha.length == 0 {
                     // 无 Alpha 通道 (XRGB/BGRX)
                    if layout.blue.offset == 0 && layout.green.offset == 8 && layout.red.offset == 16 {
                        PixelFormat::Bgra8888 
                    } else {
                        PixelFormat::Unknown
                    }
                } else {
                    tracing::warn!("未知的 32-bpp 布局");
                    PixelFormat::Unknown
                }
            }
            16 => {
                if layout.red.offset == 11
                    && layout.green.offset == 5
                    && layout.blue.offset == 0
                    && layout.red.length == 5
                    && layout.green.length == 6
                    && layout.blue.length == 5
                {
                    PixelFormat::Rgb565
                } else {
                    tracing::warn!("不支持的 16-bpp 布局 (非标准 RGB565)");
                    PixelFormat::Unknown
                }
            }
            bpp => {
                tracing::warn!("不支持的色深: {} bpp (仅支持 16 和 32)", bpp);
                PixelFormat::Unknown
            }
        }
    }
}

// --- 32-bpp ABGR ---
#[repr(transparent)]
#[derive(Clone, Copy, Default, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct PixelAbgr8888(pub u32);

impl TargetPixel for PixelAbgr8888 {
    fn from_rgb(red: u8, green: u8, blue: u8) -> Self {
        Self(u32::from_le_bytes([blue, green, red, 0xff]))
    }

    fn blend(&mut self, color: PremultipliedRgbaColor) {
        if color.alpha == 0 { return; }
        
        let [b, g, r, a] = self.0.to_le_bytes();
        let mut old_color = PremultipliedRgbaColor { red: r, green: g, blue: b, alpha: a };
        
        old_color.blend(color);
        
        self.0 = u32::from_le_bytes([old_color.blue, old_color.green, old_color.red, old_color.alpha]);
    }

    fn blend_slice(slice: &mut [Self], color: PremultipliedRgbaColor) {
        if color.alpha == 0 { return; }
        let target_pixel = u32::from_le_bytes([color.blue, color.green, color.red, color.alpha]);
        
        if color.alpha == 0xFF {
            slice.fill(Self(target_pixel));
        } else {
            for px in slice {
                px.blend(color);
            }
        }
    }
}

// --- 32-bpp RGBA ---
#[repr(transparent)]
#[derive(Clone, Copy, Default, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct PixelRgba8888(pub u32);

impl TargetPixel for PixelRgba8888 {
    fn from_rgb(red: u8, green: u8, blue: u8) -> Self {
        Self(u32::from_le_bytes([red, green, blue, 0xff]))
    }

    fn blend(&mut self, color: PremultipliedRgbaColor) {
        if color.alpha == 0 { return; }
        let [r, g, b, a] = self.0.to_le_bytes();
        let mut old_color = PremultipliedRgbaColor { red: r, green: g, blue: b, alpha: a };
        old_color.blend(color);
        self.0 = u32::from_le_bytes([old_color.red, old_color.green, old_color.blue, old_color.alpha]);
    }

    fn blend_slice(slice: &mut [Self], color: PremultipliedRgbaColor) {
        if color.alpha == 0 { return; }
        let target_pixel = u32::from_le_bytes([color.red, color.green, color.blue, color.alpha]);
        if color.alpha == 0xFF {
            slice.fill(Self(target_pixel));
        } else {
            for px in slice { px.blend(color); }
        }
    }
}

// --- 32-bpp BGRA ---
#[repr(transparent)]
#[derive(Clone, Copy, Default, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct PixelBgra8888(pub u32);

impl TargetPixel for PixelBgra8888 {
    fn from_rgb(red: u8, green: u8, blue: u8) -> Self {
        Self(u32::from_le_bytes([blue, green, red, 0xff]))
    }

    fn blend(&mut self, color: PremultipliedRgbaColor) {
        if color.alpha == 0 { return; }
        let [b, g, r, a] = self.0.to_le_bytes();
        let mut old_color = PremultipliedRgbaColor { red: r, green: g, blue: b, alpha: a };
        old_color.blend(color);
        self.0 = u32::from_le_bytes([old_color.blue, old_color.green, old_color.red, old_color.alpha]);
    }

    fn blend_slice(slice: &mut [Self], color: PremultipliedRgbaColor) {
        if color.alpha == 0 { return; }
        let target_pixel = u32::from_le_bytes([color.blue, color.green, color.red, color.alpha]);
        if color.alpha == 0xFF {
            slice.fill(Self(target_pixel));
        } else {
            for px in slice { px.blend(color); }
        }
    }
}

// --- 16-bpp Rgb565 ---
#[repr(transparent)]
#[derive(Clone, Copy, Default, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct PixelRgb565(pub u16);

impl TargetPixel for PixelRgb565 {
    fn from_rgb(red: u8, green: u8, blue: u8) -> Self {
        let r = (red as u16 & 0xF8) << 8;
        let g = (green as u16 & 0xFC) << 3;
        let b = (blue as u16 & 0xF8) >> 3;
        Self((r | g | b).to_le()) 
    }

    fn blend(&mut self, color: PremultipliedRgbaColor) {
        if color.alpha == 0 { return; }

        let pixel_data = self.0.to_le();
        let r_565 = (pixel_data & 0xF800) >> 8;
        let g_565 = (pixel_data & 0x07E0) >> 3;
        let b_565 = (pixel_data & 0x001F) << 3;

        let r = (r_565 as u8) | (r_565 >> 5) as u8;
        let g = (g_565 as u8) | (g_565 >> 6) as u8;
        let b = (b_565 as u8) | (b_565 >> 5) as u8;

        let mut old_color = PremultipliedRgbaColor { red: r, green: g, blue: b, alpha: 0xFF };
        old_color.blend(color);

        self.0 = Self::from_rgb(old_color.red, old_color.green, old_color.blue).0;
    }

    fn blend_slice(slice: &mut [Self], color: PremultipliedRgbaColor) {
        if color.alpha == 0 { return; }
        let target_pixel = Self::from_rgb(color.red, color.green, color.blue);

        if color.alpha == 0xFF {
            slice.fill(target_pixel);
        } else {
            for px in slice { px.blend(color); }
        }
    }
}