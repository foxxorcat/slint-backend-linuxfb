//! 输入子系统主模块
//!
//! 负责协调键盘、鼠标和触摸设备。

mod keyboard;
mod touch;

use std::collections::HashSet;
use std::fs;
use std::io;
use std::os::unix::io::{AsRawFd, RawFd};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::thread;
use std::time::{Duration, Instant};

use evdev::{AbsInfo, AbsoluteAxisCode, Device, EventSummary, InputEvent, KeyCode, RelativeAxisCode, SynchronizationCode};
use i_slint_core::api::PhysicalPosition;
use i_slint_core::platform::{PointerEventButton, WindowEvent};

use crate::error::Error;
use self::keyboard::KeyboardHandler;
use self::touch::{TouchState, analyze_touch_gesture};

/// 重新扫描输入设备的时间间隔
const RESCAN_INTERVAL: Duration = Duration::from_secs(3);
/// 移动事件节流阈值 (约 120Hz)
const MOVE_THROTTLE_DURATION: Duration = Duration::from_millis(8);

/// 输入设备配置选项
#[derive(Debug, Clone)]
pub struct InputConfig {
    pub autodiscovery: bool,
    pub threaded_input: bool,
    pub whitelist: Vec<String>,
    pub blacklist: Vec<String>,
}

impl Default for InputConfig {
    fn default() -> Self {
        Self {
            autodiscovery: true,
            threaded_input: true,
            whitelist: Vec::new(),
            blacklist: Vec::new(),
        }
    }
}

/// 内部结构：封装 evdev 设备及状态
struct ManagedDevice {
    path: PathBuf,
    device: Device,
    abs_x_info: Option<AbsInfo>,
    abs_y_info: Option<AbsInfo>,
    
    // 协议类型
    is_protocol_b: bool,

    // 触摸状态
    touch: TouchState,
}

/// 全局输入状态
struct GlobalInputState {
    pointer_pos: PhysicalPosition,
    is_left_pressed: bool,
    screen_width: u32,
    screen_height: u32,
    
    // 键盘处理逻辑 (抽象层)
    keyboard: KeyboardHandler,
    
    // 节流控制
    last_move_time: Instant,
}

impl GlobalInputState {
    fn should_emit_move(&mut self) -> bool {
        let now = Instant::now();
        if now.duration_since(self.last_move_time) >= MOVE_THROTTLE_DURATION {
            self.last_move_time = now;
            true
        } else {
            false
        }
    }

    fn process_device_events(&mut self, dev: &mut ManagedDevice, events: Vec<InputEvent>) -> Vec<WindowEvent> {
        let mut output = Vec::new();
        let mut sync_needed = false;
        
        let mut wheel_dx = 0;
        let mut wheel_dy = 0;

        for ev in events {
            match ev.destructure() {
                // --- MT Protocol B / Touch Handling ---
                EventSummary::AbsoluteAxis(_, code, value) => {
                    dev.touch.process_axis(code, value, dev.is_protocol_b);
                }

                // --- 相对移动 (鼠标) ---
                EventSummary::RelativeAxis(_, RelativeAxisCode::REL_X, value) => {
                    self.pointer_pos.x = (self.pointer_pos.x + value).clamp(0, self.screen_width as i32 - 1);
                    sync_needed = true;
                }
                EventSummary::RelativeAxis(_, RelativeAxisCode::REL_Y, value) => {
                    self.pointer_pos.y = (self.pointer_pos.y + value).clamp(0, self.screen_height as i32 - 1);
                    sync_needed = true;
                }
                EventSummary::RelativeAxis(_, RelativeAxisCode::REL_WHEEL, value) => {
                    wheel_dy += value;
                }
                EventSummary::RelativeAxis(_, RelativeAxisCode::REL_HWHEEL, value) => {
                    wheel_dx += value;
                }

                // --- 按键 ---
                EventSummary::Key(_, key, value) => {
                    if let Some(btn) = map_key_to_pointer_button(key) {
                        // 鼠标/触摸按键
                        if dev.abs_x_info.is_none() { 
                            let pressed = value == 1;
                            if pressed {
                                output.push(WindowEvent::PointerPressed {
                                    position: self.pointer_pos.to_logical(1.0),
                                    button: btn,
                                });
                            } else {
                                output.push(WindowEvent::PointerReleased {
                                    position: self.pointer_pos.to_logical(1.0),
                                    button: btn,
                                });
                            }
                        }
                    } else {
                        // 键盘按键 (委托给 KeyboardHandler)
                        if let Some(e) = self.keyboard.handle_key_event(key, value) {
                            output.push(e);
                        }
                    }
                }

                // --- Protocol A 同步 ---
                EventSummary::Synchronization(_, SynchronizationCode::SYN_MT_REPORT, _) => {
                    if !dev.is_protocol_b {
                        dev.touch.sync_mt_report();
                    }
                }

                // --- 帧同步 ---
                EventSummary::Synchronization(_, SynchronizationCode::SYN_REPORT, _) => {
                    if !dev.is_protocol_b {
                        dev.touch.finish_frame_protocol_a();
                    }

                    if dev.abs_x_info.is_some() {
                        // 触摸手势分析
                        if let Some(gesture_events) = analyze_touch_gesture(
                            &mut dev.touch, 
                            &mut self.pointer_pos, 
                            &mut self.is_left_pressed,
                            self.screen_width,
                            self.screen_height,
                            &dev.abs_x_info,
                            &dev.abs_y_info
                        ) {
                            // 检查移动事件节流
                            let mut filtered_events = Vec::new();
                            for evt in gesture_events {
                                match evt {
                                    WindowEvent::PointerMoved { .. } => {
                                        if self.should_emit_move() {
                                            filtered_events.push(evt);
                                        }
                                    }
                                    _ => filtered_events.push(evt),
                                }
                            }
                            output.extend(filtered_events);
                        }
                    } else if sync_needed {
                        if self.should_emit_move() {
                            output.push(WindowEvent::PointerMoved {
                                position: self.pointer_pos.to_logical(1.0),
                            });
                        }
                        sync_needed = false;
                    }

                    if wheel_dx != 0 || wheel_dy != 0 {
                        let scroll_step = 20.0; 
                        output.push(WindowEvent::PointerScrolled {
                            position: self.pointer_pos.to_logical(1.0),
                            delta_x: (wheel_dx as f32) * scroll_step,
                            delta_y: (wheel_dy as f32) * scroll_step,
                        });
                        wheel_dx = 0;
                        wheel_dy = 0;
                    }
                }
                _ => {}
            }
        }
        output
    }
}

pub struct InputManager {
    devices: Vec<ManagedDevice>,
    last_rescan: Instant,
    config: InputConfig,
    state: GlobalInputState,
    hotplug_receiver: Option<Receiver<ManagedDevice>>,
}

impl InputManager {
    pub fn new(screen_width: u32, screen_height: u32, config: InputConfig) -> Result<Self, Error> {
        tracing::info!("InputManager 初始化: 屏幕 {}x{}, 自动发现: {}, 多线程: {}, XKB支持: {}", 
            screen_width, screen_height, config.autodiscovery, config.threaded_input, cfg!(feature = "xkb"));

        let keyboard = KeyboardHandler::new()?;

        let state = GlobalInputState {
            pointer_pos: PhysicalPosition::new((screen_width / 2) as i32, (screen_height / 2) as i32),
            is_left_pressed: false,
            screen_width,
            screen_height,
            keyboard,
            last_move_time: Instant::now(),
        };

        let mut manager = Self {
            devices: Vec::new(),
            last_rescan: Instant::now(),
            config: config.clone(),
            state,
            hotplug_receiver: None,
        };

        if config.autodiscovery {
            if config.threaded_input {
                let (tx, rx) = channel();
                manager.hotplug_receiver = Some(rx);
                spawn_hotplug_thread(tx, config);
            } else {
                manager.rescan_devices_blocking();
            }
        }

        Ok(manager)
    }

    pub fn get_poll_fds(&self) -> Vec<RawFd> {
        self.devices.iter().map(|dev| dev.device.as_raw_fd()).collect()
    }

    pub fn poll(&mut self) -> Vec<WindowEvent> {
        if self.config.autodiscovery {
            if self.config.threaded_input {
                if let Some(rx) = &self.hotplug_receiver {
                    while let Ok(device) = rx.try_recv() {
                        tracing::info!("热插拔: 添加新设备 {:?}", device.path);
                        self.devices.push(device);
                    }
                }
            } else {
                if self.last_rescan.elapsed() > RESCAN_INTERVAL {
                    self.rescan_devices_blocking();
                }
            }
        }

        let mut slint_events = Vec::new();
        let mut indices_to_remove = Vec::new();

        for (i, managed_dev) in self.devices.iter_mut().enumerate() {
            let events: Vec<_> = match managed_dev.device.fetch_events() {
                Ok(iter) => iter.collect(),
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => Vec::new(),
                Err(e) => {
                    tracing::error!("设备读取失败 {:?}: {}", managed_dev.path, e);
                    indices_to_remove.push(i);
                    Vec::new()
                }
            };

            if !events.is_empty() {
                let new_events = self.state.process_device_events(managed_dev, events);
                slint_events.extend(new_events);
            }
        }

        for &i in indices_to_remove.iter().rev() {
            self.devices.remove(i);
        }

        slint_events
    }

    fn rescan_devices_blocking(&mut self) {
        let found_paths = scan_input_dir();
        self.devices.retain(|dev| found_paths.contains(&dev.path));
        
        for path in found_paths {
            if !self.devices.iter().any(|dev| dev.path == path) {
                if let Ok(Some(managed_device)) = open_device_if_compatible(&path, &self.config) {
                    self.devices.push(managed_device);
                }
            }
        }
        self.last_rescan = Instant::now();
    }
}

// --- 独立函数与线程逻辑 ---

fn scan_input_dir() -> HashSet<PathBuf> {
    let mut found = HashSet::new();
    if let Ok(entries) = fs::read_dir("/dev/input") {
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            if path.to_str().unwrap_or("").starts_with("/dev/input/event") {
                found.insert(path);
            }
        }
    }
    found
}

fn spawn_hotplug_thread(sender: Sender<ManagedDevice>, config: InputConfig) {
    thread::spawn(move || {
        let mut known_paths = HashSet::new();
        loop {
            let current_paths = scan_input_dir();
            for path in &current_paths {
                if !known_paths.contains(path) {
                    if let Ok(Some(device)) = open_device_if_compatible(path, &config) {
                        if sender.send(device).is_err() {
                            return;
                        }
                        known_paths.insert(path.clone());
                    }
                }
            }
            known_paths.retain(|p| current_paths.contains(p));
            thread::sleep(RESCAN_INTERVAL);
        }
    });
}

fn open_device_if_compatible(path: &Path, config: &InputConfig) -> io::Result<Option<ManagedDevice>> {
    let mut device = Device::open(path)?;
    let name = device.name().unwrap_or("Unknown Device");

    for block in &config.blacklist {
        if name.contains(block) { return Ok(None); }
    }
    if !config.whitelist.is_empty() {
        let mut found = false;
        for allow in &config.whitelist {
            if name.contains(allow) { found = true; break; }
        }
        if !found { return Ok(None); }
    }

    device.set_nonblocking(true)
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;

    let mut abs_x_info = None;
    let mut abs_y_info = None;

    let is_protocol_b = device.supported_absolute_axes().map_or(false, |axes| {
        axes.contains(AbsoluteAxisCode::ABS_MT_SLOT)
    });

    if is_touchscreen(&device) {
        if let Ok(axes) = device.get_absinfo() {
            for (code, info) in axes {
                match code {
                    AbsoluteAxisCode::ABS_X | AbsoluteAxisCode::ABS_MT_POSITION_X => abs_x_info = Some(info),
                    AbsoluteAxisCode::ABS_Y | AbsoluteAxisCode::ABS_MT_POSITION_Y => abs_y_info = Some(info),
                    _ => {}
                }
            }
        }
    } else if is_mouse(&device) {
        // Just log
    } else if is_keyboard(&device) {
        let repeat_config = evdev::AutoRepeat { delay: 250, period: 33 };
        let _ = device.update_auto_repeat(&repeat_config);
    } else {
        return Ok(None);
    }

    Ok(Some(ManagedDevice {
        path: path.to_path_buf(),
        device,
        abs_x_info,
        abs_y_info,
        is_protocol_b,
        touch: TouchState::new(),
    }))
}

fn map_key_to_pointer_button(key: KeyCode) -> Option<PointerEventButton> {
    match key {
        KeyCode::BTN_LEFT | KeyCode::BTN_TOUCH => Some(PointerEventButton::Left),
        KeyCode::BTN_RIGHT => Some(PointerEventButton::Right),
        KeyCode::BTN_MIDDLE => Some(PointerEventButton::Middle),
        KeyCode::BTN_SIDE => Some(PointerEventButton::Back),
        KeyCode::BTN_EXTRA => Some(PointerEventButton::Forward),
        _ => None,
    }
}

fn is_touchscreen(dev: &Device) -> bool {
    dev.supported_absolute_axes().map_or(false, |axes| {
        axes.contains(AbsoluteAxisCode::ABS_MT_POSITION_X) || axes.contains(AbsoluteAxisCode::ABS_X)
    })
}

fn is_mouse(dev: &Device) -> bool {
    let has_rel = dev.supported_relative_axes().map_or(false, |axes| {
        axes.contains(RelativeAxisCode::REL_X)
    });
    let has_btn = dev.supported_keys().map_or(false, |keys| keys.contains(KeyCode::BTN_LEFT));
    has_rel && has_btn
}

fn is_keyboard(dev: &Device) -> bool {
    dev.supported_keys().map_or(false, |keys| {
        keys.contains(KeyCode::KEY_A) && keys.contains(KeyCode::KEY_ENTER)
    })
}