//! 键盘事件处理模块
//!
//! 负责将 `evdev` 的按键事件转换为 Slint 的 `WindowEvent`。
//!
//! 本模块提供两种实现策略，通过编译特性 `xkb` 进行选择：
//! 1. **XKB 实现** (`feature = "xkb"`): 使用 `libxkbcommon` 进行完整的键盘布局、状态和死键处理。
//!    支持通过环境变量配置布局（如 `XKB_DEFAULT_LAYOUT=de`）。
//! 2. **简易实现** (`feature != "xkb"`): 内置一个简单的 US QWERTY 静态映射表。
//!    仅支持基本的字母、数字、Shift 组合符号和常用功能键，适用于资源受限或无需多语言输入的嵌入式环境。

use crate::error::Error;
use evdev::KeyCode;
use i_slint_core::platform::WindowEvent;
use i_slint_core::SharedString;

// -----------------------------------------------------------------------------
// 实现 1: 使用 xkbcommon (feature = "xkb")
// -----------------------------------------------------------------------------

#[cfg(feature = "xkb")]
mod impl_xkb {
    use super::*;
    use i_slint_core::input::key_codes;
    use xkbcommon_rs::{self as xkb, keycode, xkb_context, xkb_keymap, xkb_state};
    use xkeysym;

    /// 基于 xkbcommon 的键盘处理器
    pub struct KeyboardHandler {
        /// xkb 状态机，维护当前的修饰键（Shift/Ctrl/Alt）和键盘组状态
        state: xkb::State,
    }

    impl KeyboardHandler {
        /// 初始化 xkb 上下文、键映射和状态机
        ///
        /// 优先读取 `XKB_DEFAULT_*` 环境变量配置，否则使用系统默认值。
        pub fn new() -> Result<Self, Error> {
            // 创建上下文 (无特殊标志)
            let context = xkb::Context::new(xkb_context::ContextFlags::NO_FLAGS)
                .map_err(|_| Error::Other("Failed to create xkb context".into()))?;

            // 配置 RMLVO (Rules, Model, Layout, Variant, Options)
            let rmlvo = xkb_keymap::RuleNames {
                rules: None,
                model: None,
                layout: None,
                variant: None,
                options: None,
            };

            // 编译键映射 (Keymap)
            let keymap = xkb::Keymap::new_from_names(
                context,
                Some(rmlvo),
                xkb_keymap::CompileFlags::NO_FLAGS,
            )
            .map_err(|_| Error::Other("Failed to create xkb keymap".into()))?;

            // 创建状态机 (State)
            let state = xkb::State::new(keymap);

            Ok(Self { state })
        }

        /// 处理按键事件并转换为 Slint WindowEvent
        pub fn handle_key_event(&mut self, key_code: KeyCode, value: i32) -> Option<WindowEvent> {
            // Linux evdev keycodes 需要 +8 偏移量才能映射到 XKB keycodes
            let xkb_keycode = keycode::Keycode((key_code.code() + 8) as u32);

            let direction = match value {
                0 => xkb_state::KeyDirection::Up,       // Release
                1 | 2 => xkb_state::KeyDirection::Down, // Press or Repeat
                _ => return None,
            };

            // 更新 xkb 内部状态 (如 Shift 锁定等)
            self.state.update_key(xkb_keycode, direction);

            // 获取对应的 Unicode 字符或特殊键符号
            let text_char = self
                .state
                .key_get_one_sym(xkb_keycode)
                .and_then(map_keysym_to_char);

            let text: SharedString = text_char.map(|c| c.into()).unwrap_or_default();

            match value {
                0 => Some(WindowEvent::KeyReleased { text }),
                1 => Some(WindowEvent::KeyPressed { text }),
                2 => Some(WindowEvent::KeyPressRepeated { text }),
                _ => None,
            }
        }
    }

    /// 将 X11 Keysym 映射为 Slint 使用的字符或功能键代码
    fn map_keysym_to_char(sym: xkeysym::Keysym) -> Option<char> {
        match sym.raw() {
            // 修饰键
            xkeysym::key::Shift_L => Some(key_codes::Shift),
            xkeysym::key::Shift_R => Some(key_codes::ShiftR),
            xkeysym::key::Control_L => Some(key_codes::Control),
            xkeysym::key::Control_R => Some(key_codes::ControlR),
            xkeysym::key::Alt_L => Some(key_codes::Alt),
            xkeysym::key::Alt_R | xkeysym::key::ISO_Level3_Shift => Some(key_codes::AltGr),
            xkeysym::key::Meta_L | xkeysym::key::Super_L => Some(key_codes::Meta),
            xkeysym::key::Meta_R | xkeysym::key::Super_R => Some(key_codes::MetaR),
            xkeysym::key::Caps_Lock => Some(key_codes::CapsLock),

            // 编辑与导航
            xkeysym::key::Return | xkeysym::key::KP_Enter => Some(key_codes::Return),
            xkeysym::key::Escape => Some(key_codes::Escape),
            xkeysym::key::Tab | xkeysym::key::KP_Tab => Some(key_codes::Tab),
            xkeysym::key::ISO_Left_Tab => Some(key_codes::Backtab),
            xkeysym::key::BackSpace => Some(key_codes::Backspace),
            xkeysym::key::Delete | xkeysym::key::KP_Delete => Some(key_codes::Delete),
            xkeysym::key::Insert | xkeysym::key::KP_Insert => Some(key_codes::Insert),
            
            xkeysym::key::Home | xkeysym::key::KP_Home => Some(key_codes::Home),
            xkeysym::key::End | xkeysym::key::KP_End => Some(key_codes::End),
            xkeysym::key::Page_Up | xkeysym::key::KP_Page_Up => Some(key_codes::PageUp),
            xkeysym::key::Page_Down | xkeysym::key::KP_Page_Down => Some(key_codes::PageDown),
            
            xkeysym::key::Up | xkeysym::key::KP_Up => Some(key_codes::UpArrow),
            xkeysym::key::Down | xkeysym::key::KP_Down => Some(key_codes::DownArrow),
            xkeysym::key::Left | xkeysym::key::KP_Left => Some(key_codes::LeftArrow),
            xkeysym::key::Right | xkeysym::key::KP_Right => Some(key_codes::RightArrow),

            xkeysym::key::space | xkeysym::key::KP_Space => Some(key_codes::Space),
            xkeysym::key::Menu => Some(key_codes::Menu),
            xkeysym::key::Scroll_Lock => Some(key_codes::ScrollLock),
            xkeysym::key::Pause => Some(key_codes::Pause),
            xkeysym::key::Sys_Req | xkeysym::key::Print => Some(key_codes::SysReq),

            // 功能键 F1-F24
            xkeysym::key::F1 => Some(key_codes::F1),
            xkeysym::key::F2 => Some(key_codes::F2),
            xkeysym::key::F3 => Some(key_codes::F3),
            xkeysym::key::F4 => Some(key_codes::F4),
            xkeysym::key::F5 => Some(key_codes::F5),
            xkeysym::key::F6 => Some(key_codes::F6),
            xkeysym::key::F7 => Some(key_codes::F7),
            xkeysym::key::F8 => Some(key_codes::F8),
            xkeysym::key::F9 => Some(key_codes::F9),
            xkeysym::key::F10 => Some(key_codes::F10),
            xkeysym::key::F11 => Some(key_codes::F11),
            xkeysym::key::F12 => Some(key_codes::F12),
            xkeysym::key::F13 => Some(key_codes::F13),
            xkeysym::key::F14 => Some(key_codes::F14),
            xkeysym::key::F15 => Some(key_codes::F15),
            xkeysym::key::F16 => Some(key_codes::F16),
            xkeysym::key::F17 => Some(key_codes::F17),
            xkeysym::key::F18 => Some(key_codes::F18),
            xkeysym::key::F19 => Some(key_codes::F19),
            xkeysym::key::F20 => Some(key_codes::F20),
            xkeysym::key::F21 => Some(key_codes::F21),
            xkeysym::key::F22 => Some(key_codes::F22),
            xkeysym::key::F23 => Some(key_codes::F23),
            xkeysym::key::F24 => Some(key_codes::F24),

            // 默认尝试转换为 Unicode 字符
            _ => sym.key_char(),
        }
    }
}

// -----------------------------------------------------------------------------
// 实现 2: 简易映射 (无 xkb, feature != "xkb")
// -----------------------------------------------------------------------------

#[cfg(not(feature = "xkb"))]
mod impl_simple {
    use super::*;
    use i_slint_core::input::key_codes;

    /// 简易键盘处理器 (静态 US QWERTY 布局)
    pub struct KeyboardHandler {
        /// 简单的 Shift 状态跟踪
        shift_pressed: bool,
    }

    impl KeyboardHandler {
        pub fn new() -> Result<Self, Error> {
            tracing::info!("Keyboard: Using simple static mapping (No XKB)");
            Ok(Self {
                shift_pressed: false,
            })
        }

        pub fn handle_key_event(&mut self, key_code: KeyCode, value: i32) -> Option<WindowEvent> {
            // 1. 更新修饰符状态 (仅跟踪 Shift)
            match value {
                1 => {
                    // Press
                    if matches!(key_code, KeyCode::KEY_LEFTSHIFT | KeyCode::KEY_RIGHTSHIFT) {
                        self.shift_pressed = true;
                    }
                }
                0 => {
                    // Release
                    if matches!(key_code, KeyCode::KEY_LEFTSHIFT | KeyCode::KEY_RIGHTSHIFT) {
                        self.shift_pressed = false;
                    }
                }
                _ => {} // Repeat
            }

            // 2. 获取按键对应的字符或功能码
            let text = self.map_key_code(key_code).unwrap_or_default();

            // 3. 生成事件
            match value {
                0 => Some(WindowEvent::KeyReleased { text }),
                1 => Some(WindowEvent::KeyPressed { text }),
                2 => Some(WindowEvent::KeyPressRepeated { text }),
                _ => None,
            }
        }

              /// 静态映射逻辑：evdev KeyCode -> Slint SharedString
        /// 实现了标准的 US 键盘 Shift 组合逻辑
        fn map_key_code(&self, code: KeyCode) -> Option<SharedString> {
            let s = match code {
                // 修饰键 (Modifiers)
                KeyCode::KEY_LEFTSHIFT => return Some(key_codes::Shift.into()),
                KeyCode::KEY_RIGHTSHIFT => return Some(key_codes::ShiftR.into()),
                KeyCode::KEY_LEFTCTRL => return Some(key_codes::Control.into()),
                KeyCode::KEY_RIGHTCTRL => return Some(key_codes::ControlR.into()),
                KeyCode::KEY_LEFTALT => return Some(key_codes::Alt.into()),
                KeyCode::KEY_RIGHTALT => return Some(key_codes::AltGr.into()),
                KeyCode::KEY_LEFTMETA => return Some(key_codes::Meta.into()),
                KeyCode::KEY_RIGHTMETA => return Some(key_codes::MetaR.into()),
                KeyCode::KEY_CAPSLOCK => return Some(key_codes::CapsLock.into()),

                // 字母 (A-Z)
                KeyCode::KEY_Q => if self.shift_pressed { "Q" } else { "q" },
                KeyCode::KEY_W => if self.shift_pressed { "W" } else { "w" },
                KeyCode::KEY_E => if self.shift_pressed { "E" } else { "e" },
                KeyCode::KEY_R => if self.shift_pressed { "R" } else { "r" },
                KeyCode::KEY_T => if self.shift_pressed { "T" } else { "t" },
                KeyCode::KEY_Y => if self.shift_pressed { "Y" } else { "y" },
                KeyCode::KEY_U => if self.shift_pressed { "U" } else { "u" },
                KeyCode::KEY_I => if self.shift_pressed { "I" } else { "i" },
                KeyCode::KEY_O => if self.shift_pressed { "O" } else { "o" },
                KeyCode::KEY_P => if self.shift_pressed { "P" } else { "p" },
                KeyCode::KEY_A => if self.shift_pressed { "A" } else { "a" },
                KeyCode::KEY_S => if self.shift_pressed { "S" } else { "s" },
                KeyCode::KEY_D => if self.shift_pressed { "D" } else { "d" },
                KeyCode::KEY_F => if self.shift_pressed { "F" } else { "f" },
                KeyCode::KEY_G => if self.shift_pressed { "G" } else { "g" },
                KeyCode::KEY_H => if self.shift_pressed { "H" } else { "h" },
                KeyCode::KEY_J => if self.shift_pressed { "J" } else { "j" },
                KeyCode::KEY_K => if self.shift_pressed { "K" } else { "k" },
                KeyCode::KEY_L => if self.shift_pressed { "L" } else { "l" },
                KeyCode::KEY_Z => if self.shift_pressed { "Z" } else { "z" },
                KeyCode::KEY_X => if self.shift_pressed { "X" } else { "x" },
                KeyCode::KEY_C => if self.shift_pressed { "C" } else { "c" },
                KeyCode::KEY_V => if self.shift_pressed { "V" } else { "v" },
                KeyCode::KEY_B => if self.shift_pressed { "B" } else { "b" },
                KeyCode::KEY_N => if self.shift_pressed { "N" } else { "n" },
                KeyCode::KEY_M => if self.shift_pressed { "M" } else { "m" },

                // 数字行 (Shift 符号映射)
                KeyCode::KEY_1 => if self.shift_pressed { "!" } else { "1" },
                KeyCode::KEY_2 => if self.shift_pressed { "@" } else { "2" },
                KeyCode::KEY_3 => if self.shift_pressed { "#" } else { "3" },
                KeyCode::KEY_4 => if self.shift_pressed { "$" } else { "4" },
                KeyCode::KEY_5 => if self.shift_pressed { "%" } else { "5" },
                KeyCode::KEY_6 => if self.shift_pressed { "^" } else { "6" },
                KeyCode::KEY_7 => if self.shift_pressed { "&" } else { "7" },
                KeyCode::KEY_8 => if self.shift_pressed { "*" } else { "8" },
                KeyCode::KEY_9 => if self.shift_pressed { "(" } else { "9" },
                KeyCode::KEY_0 => if self.shift_pressed { ")" } else { "0" },

                // 符号键 (Shift 符号映射)
                KeyCode::KEY_MINUS | KeyCode::KEY_KPMINUS => if self.shift_pressed { "_" } else { "-" },
                KeyCode::KEY_EQUAL | KeyCode::KEY_KPEQUAL => if self.shift_pressed { "+" } else { "=" },
                KeyCode::KEY_LEFTBRACE => if self.shift_pressed { "{" } else { "[" },
                KeyCode::KEY_RIGHTBRACE => if self.shift_pressed { "}" } else { "]" },
                KeyCode::KEY_BACKSLASH => if self.shift_pressed { "|" } else { "\\" },
                KeyCode::KEY_SEMICOLON => if self.shift_pressed { ":" } else { ";" },
                KeyCode::KEY_APOSTROPHE => if self.shift_pressed { "\"" } else { "'" },
                KeyCode::KEY_COMMA | KeyCode::KEY_KPCOMMA => if self.shift_pressed { "<" } else { "," },
                KeyCode::KEY_DOT | KeyCode::KEY_KPDOT => if self.shift_pressed { ">" } else { "." },
                KeyCode::KEY_SLASH | KeyCode::KEY_KPSLASH => if self.shift_pressed { "?" } else { "/" },
                KeyCode::KEY_GRAVE => if self.shift_pressed { "~" } else { "`" },

                // 控制键与功能键
                KeyCode::KEY_ESC => return Some(key_codes::Escape.into()),
                KeyCode::KEY_ENTER | KeyCode::KEY_KPENTER => return Some(key_codes::Return.into()),
                KeyCode::KEY_BACKSPACE => return Some(key_codes::Backspace.into()),
                KeyCode::KEY_TAB => {
                    if self.shift_pressed {
                        return Some(key_codes::Backtab.into());
                    } else {
                        return Some(key_codes::Tab.into());
                    }
                },
                KeyCode::KEY_SPACE => return Some(key_codes::Space.into()),

                KeyCode::KEY_UP => return Some(key_codes::UpArrow.into()),
                KeyCode::KEY_DOWN => return Some(key_codes::DownArrow.into()),
                KeyCode::KEY_LEFT => return Some(key_codes::LeftArrow.into()),
                KeyCode::KEY_RIGHT => return Some(key_codes::RightArrow.into()),

                KeyCode::KEY_F1 => return Some(key_codes::F1.into()),
                KeyCode::KEY_F2 => return Some(key_codes::F2.into()),
                KeyCode::KEY_F3 => return Some(key_codes::F3.into()),
                KeyCode::KEY_F4 => return Some(key_codes::F4.into()),
                KeyCode::KEY_F5 => return Some(key_codes::F5.into()),
                KeyCode::KEY_F6 => return Some(key_codes::F6.into()),
                KeyCode::KEY_F7 => return Some(key_codes::F7.into()),
                KeyCode::KEY_F8 => return Some(key_codes::F8.into()),
                KeyCode::KEY_F9 => return Some(key_codes::F9.into()),
                KeyCode::KEY_F10 => return Some(key_codes::F10.into()),
                KeyCode::KEY_F11 => return Some(key_codes::F11.into()),
                KeyCode::KEY_F12 => return Some(key_codes::F12.into()),
                KeyCode::KEY_F13 => return Some(key_codes::F13.into()),
                KeyCode::KEY_F14 => return Some(key_codes::F14.into()),
                KeyCode::KEY_F15 => return Some(key_codes::F15.into()),
                KeyCode::KEY_F16 => return Some(key_codes::F16.into()),
                KeyCode::KEY_F17 => return Some(key_codes::F17.into()),
                KeyCode::KEY_F18 => return Some(key_codes::F18.into()),
                KeyCode::KEY_F19 => return Some(key_codes::F19.into()),
                KeyCode::KEY_F20 => return Some(key_codes::F20.into()),
                KeyCode::KEY_F21 => return Some(key_codes::F21.into()),
                KeyCode::KEY_F22 => return Some(key_codes::F22.into()),
                KeyCode::KEY_F23 => return Some(key_codes::F23.into()),
                KeyCode::KEY_F24 => return Some(key_codes::F24.into()),

                KeyCode::KEY_DELETE => return Some(key_codes::Delete.into()),
                KeyCode::KEY_HOME => return Some(key_codes::Home.into()),
                KeyCode::KEY_END => return Some(key_codes::End.into()),
                KeyCode::KEY_PAGEUP => return Some(key_codes::PageUp.into()),
                KeyCode::KEY_PAGEDOWN => return Some(key_codes::PageDown.into()),
                KeyCode::KEY_INSERT => return Some(key_codes::Insert.into()),

                KeyCode::KEY_SYSRQ => return Some(key_codes::SysReq.into()),
                KeyCode::KEY_SCROLLLOCK => return Some(key_codes::ScrollLock.into()),
                KeyCode::KEY_PAUSE => return Some(key_codes::Pause.into()),
                KeyCode::KEY_STOP => return Some(key_codes::Stop.into()),
                KeyCode::KEY_MENU => return Some(key_codes::Menu.into()),
                KeyCode::KEY_BACK => return Some(key_codes::Back.into()),

                _ => return None,
            };
            Some(s.into())
        }
      }
}

// -----------------------------------------------------------------------------
// 统一导出类型
// -----------------------------------------------------------------------------

#[cfg(feature = "xkb")]
pub use impl_xkb::KeyboardHandler;

#[cfg(not(feature = "xkb"))]
pub use impl_simple::KeyboardHandler;
