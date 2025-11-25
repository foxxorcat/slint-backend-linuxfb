use crate::error::Error;
use crate::input::{InputConfig, InputManager}; 
use crate::pixels::PixelFormat;
use crate::window::LinuxFbWindowAdapter;
use i_slint_core::platform::{
    software_renderer::{RepaintBufferType, SoftwareRenderer},
    Platform, PlatformError, WindowAdapter, WindowEvent,
};
use i_slint_core::renderer::RendererSealed;
use crate::linuxfb::{
    double::Buffer,
    fbio::{self, TerminalMode},
    Framebuffer,
};
use std::cell::RefCell;
use std::fs::{File, OpenOptions};
use std::rc::Rc;
use std::time::Duration;
use std::sync::Mutex;
use std::path::PathBuf;
use libc;

// 全局静态变量，用于在 Ctrl+C 信号处理器中恢复 TTY
static ACTIVE_TTY_PATH: Mutex<Option<PathBuf>> = Mutex::new(None);

/// Linux Framebuffer 平台构建器 (V2)
#[derive(Default)]
pub struct LinuxFbPlatformBuilder {
    tty_path: Option<PathBuf>,
    fb_path: Option<PathBuf>,
    input_config: InputConfig,
    vsync: bool,
}

impl LinuxFbPlatformBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    /// 设置 TTY 设备路径 (例如 "/dev/tty3")
    /// 如果不设置，默认尝试使用环境变量 `SLINT_TTY_DEVICE`，然后是 /dev/tty1, /dev/tty0
    pub fn with_tty(mut self, path: impl Into<PathBuf>) -> Self {
        self.tty_path = Some(path.into());
        self
    }

    /// 设置 Framebuffer 设备路径 (例如 "/dev/fb1")
    /// 如果不设置，默认尝试使用环境变量 `SLINT_FRAMEBUFFER`，然后是 /dev/fb0
    pub fn with_framebuffer(mut self, path: impl Into<PathBuf>) -> Self {
        self.fb_path = Some(path.into());
        self
    }

    /// 配置是否自动发现输入设备
    pub fn with_input_autodiscovery(mut self, enable: bool) -> Self {
        self.input_config.autodiscovery = enable;
        self
    }

    /// 开启或关闭多线程输入设备扫描 (默认: true)
    /// 设置为 false 可用于不支持多线程的环境。
    pub fn with_threaded_input(mut self, enable: bool) -> Self {
        self.input_config.threaded_input = enable;
        self
    }

    /// 添加输入设备名称白名单
    /// 只有名称包含列表中字符串的设备会被加载。
    pub fn with_input_whitelist(mut self, list: Vec<String>) -> Self {
        self.input_config.whitelist = list;
        self
    }

    /// 添加输入设备名称黑名单
    /// 名称包含列表中字符串的设备将被忽略。
    pub fn with_input_blacklist(mut self, list: Vec<String>) -> Self {
        self.input_config.blacklist = list;
        self
    }

    /// 启用垂直同步 (VSync)
    ///
    /// 如果启用，渲染循环将尝试等待硬件垂直消隐信号。
    /// 这可以消除撕裂并降低静态画面下的 CPU 占用，但需要 Framebuffer 驱动支持。
    pub fn with_vsync(mut self, enable: bool) -> Self {
        self.vsync = enable;
        self
    }

    /// 构建并初始化平台
    pub fn build(self) -> Result<LinuxFbPlatform, Error> {
        LinuxFbPlatform::new_with_config(self)
    }
}

pub struct LinuxFbPlatform {
    adapter: RefCell<Option<Rc<LinuxFbWindowAdapter>>>,
    input_manager: RefCell<Option<InputManager>>,
    tty: Option<File>,
    config: LinuxFbPlatformBuilder,
}

impl LinuxFbPlatform {
    /// 使用默认配置创建平台
    pub fn new() -> Result<Self, Error> {
        LinuxFbPlatformBuilder::new().build()
    }

    fn new_with_config(config: LinuxFbPlatformBuilder) -> Result<Self, Error> {
        // --- 确定 TTY 路径 ---
        let tty_path = config.tty_path.clone()
            .or_else(|| std::env::var("SLINT_TTY_DEVICE").ok().map(PathBuf::from))
            .or_else(|| Some(PathBuf::from("/dev/tty1")));

        // 尝试打开 TTY
        let tty = if let Some(path) = &tty_path {
            match OpenOptions::new().read(true).write(true).open(path) {
                Ok(file) => {
                    tracing::info!("使用 TTY: {:?}", path);
                    Some(file)
                },
                Err(_) => {
                    // 如果首选失败且是默认的 tty1，尝试 tty0
                    if path == &PathBuf::from("/dev/tty1") {
                        OpenOptions::new().read(true).write(true).open("/dev/tty0").ok()
                    } else {
                        None
                    }
                }
            }
        } else {
            None
        };

        if let Some(ref tty_file) = tty {
            // 保存实际打开的路径用于恢复
            let path_to_save = tty_path.unwrap_or_else(|| PathBuf::from("/dev/tty0"));
            *ACTIVE_TTY_PATH.lock().unwrap() = Some(path_to_save);

            if let Err(e) = fbio::set_terminal_mode(tty_file, TerminalMode::Graphics) {
                tracing::warn!("无法将 TTY 切换到图形模式: {}", e);
            } else {
                tracing::info!("TTY 已切换到图形模式 (KD_GRAPHICS)。");
            }
        } else {
            tracing::warn!("无法打开 TTY。fbcon 光标可能会干扰 UI。");
        }

        // --- 注册信号处理器 (处理 SIGINT/SIGTERM) ---
        let _ = ctrlc::set_handler(move || {
            tracing::info!("接收到退出信号，正在恢复 TTY...");
            if let Ok(guard) = ACTIVE_TTY_PATH.lock() {
                if let Some(ref path) = *guard {
                    if let Ok(file) = OpenOptions::new().read(true).write(true).open(path) {
                        let _ = fbio::set_terminal_mode(&file, TerminalMode::Text);
                    }
                }
            }
            std::process::exit(0);
        });

        Ok(Self {
            adapter: RefCell::new(None),
            input_manager: RefCell::new(None),
            tty,
            config,
        })
    }
}

impl Drop for LinuxFbPlatform {
    fn drop(&mut self) {
        if let Some(ref tty) = self.tty {
            tracing::info!("正在恢复 TTY 到文本模式 (Drop)...");
            if let Err(e) = fbio::set_terminal_mode(tty, TerminalMode::Text) {
                tracing::error!("无法恢复 TTY 到文本模式: {}", e);
            }
        }
        if let Ok(mut guard) = ACTIVE_TTY_PATH.lock() {
            *guard = None;
        }
    }
}

impl Platform for LinuxFbPlatform {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, PlatformError> {
        // --- 获取 Framebuffer 路径 ---
        let fb_path = self.config.fb_path.clone()
            .or_else(|| std::env::var("SLINT_FRAMEBUFFER").ok().map(PathBuf::from))
            .unwrap_or_else(|| PathBuf::from("/dev/fb0"));
            
        tracing::info!("打开 Framebuffer 设备: {:?}", fb_path);

        let fb = Framebuffer::new(&fb_path).map_err(|e| PlatformError::Other(e.to_string()))?;
        let vinfo = fb.vinfo.clone();
        let pixel_format = PixelFormat::from_fb_info(&vinfo);

        if pixel_format == PixelFormat::Unknown {
            return Err(PlatformError::Other(
                Error::UnsupportedPixelFormat.to_string(),
            ));
        }

        let fb_buffer = Buffer::new(fb).map_err(|e| PlatformError::Other(e.to_string()))?;
        let (width, height) = (fb_buffer.width, fb_buffer.height);

        // --- 初始化输入管理器 ---
        let input_manager = InputManager::new(width, height, self.config.input_config.clone())
            .map_err(|e| PlatformError::Other(e.to_string()))?;
            
        *self.input_manager.borrow_mut() = Some(input_manager);

        // --- 创建 Window Adapter ---
        let adapter = Rc::<LinuxFbWindowAdapter>::new_cyclic(|weak_adapter| {
            let window = Rc::new(i_slint_core::api::Window::new(weak_adapter.clone()));
            let renderer =
                SoftwareRenderer::new_with_repaint_buffer_type(RepaintBufferType::SwappedBuffers);

            LinuxFbWindowAdapter {
                window,
                fb_buffer: RefCell::new(fb_buffer),
                renderer,
                pixel_format,
                needs_redraw: RefCell::new(true),
            }
        });

        adapter
            .renderer
            .set_window_adapter(&(adapter.clone() as Rc<dyn WindowAdapter>));
        *self.adapter.borrow_mut() = Some(adapter.clone());

        adapter.window.dispatch_event(WindowEvent::Resized {
            size: i_slint_core::api::LogicalSize::new(width as f32, height as f32),
        });
        adapter
            .window
            .dispatch_event(WindowEvent::ScaleFactorChanged { scale_factor: 1.0 });

        Ok(adapter)
    }

    fn run_event_loop(&self) -> Result<(), PlatformError> {
        let adapter = self
            .adapter
            .borrow()
            .as_ref()
            .cloned()
            .ok_or_else(|| PlatformError::Other("Window adapter not created".into()))?;

        let window = adapter.window.clone();

        let mut input_manager_guard = self.input_manager.borrow_mut();
        let input_manager = input_manager_guard
            .as_mut()
            .expect("Input manager not initialized");

        if self.config.vsync {
            tracing::info!("VSync 已启用。渲染循环将等待硬件垂直消隐。");
        }

        loop {
            // 1. 处理 Slint 定时器和动画
            i_slint_core::platform::update_timers_and_animations();

            // 2. 轮询输入事件
            for event in input_manager.poll() {
                window.dispatch_event(event);
            }

            // 3. 渲染逻辑
            if *adapter.needs_redraw.borrow() {
                *adapter.needs_redraw.borrow_mut() = false;

                if let Err(e) = adapter.render_frame(&adapter.renderer) {
                    tracing::error!("帧渲染错误: {}", e);
                }

                let mut fb_buffer = adapter.fb_buffer.borrow_mut();

                // VSync 等待
                if self.config.vsync {
                    if let Err(e) = fb_buffer.wait_for_vsync() {
                        tracing::warn!("等待 VSync 失败 (可能驱动不支持): {}", e);
                    }
                }

                // 缓冲区翻转
                if let Err(e) = fb_buffer.flip() {
                    tracing::error!("Framebuffer 翻转(Flip)失败: {}", e);
                    return Err(PlatformError::Other(e.to_string()));
                }
            }

            // 4. 计算休眠时间 & 等待事件 (Poll)
            let next_timer = i_slint_core::platform::duration_until_next_timer_update();
            
            // 保持 16ms 心跳，处理跨线程事件回调
            let timeout = next_timer.unwrap_or(Duration::from_millis(16));

            // 获取所有输入设备的文件描述符
            let input_fds = input_manager.get_poll_fds();
            let mut poll_fds: Vec<libc::pollfd> = input_fds.into_iter().map(|fd| libc::pollfd {
                fd,
                events: libc::POLLIN,
                revents: 0
            }).collect();

            let timeout_ms = timeout.as_millis() as i32;

            // 调用 libc::poll 挂起线程
            if !poll_fds.is_empty() || timeout_ms > 0 {
                unsafe {
                    libc::poll(poll_fds.as_mut_ptr(), poll_fds.len() as libc::nfds_t, timeout_ms);
                }
            } else {
                // 最小休眠时间
                if timeout_ms > 0 {
                    std::thread::sleep(timeout);
                }
            }
        }
    }
}