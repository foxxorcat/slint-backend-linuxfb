 # Slint Backend for Linux Framebuffer

 这是一个为 [Slint](https://slint.dev) UI 工具包提供的 **Linux Framebuffer** 后端实现。

 它允许你在没有 X11 或 Wayland 的嵌入式 Linux 系统上运行 Slint 应用程序，直接渲染到 `/dev/fb0`，并通过 `evdev` 读取触摸屏、鼠标和键盘输入。

 也可以尝试使用官方的 linuxkms 后端， 不过需要追踪[slint#10086](https://github.com/slint-ui/slint/issues/10086)

 ## ✨ 特性

 - **无复杂C依赖**: 支持使用 musl 工具链编译静态链接程序。
 - **输入支持**:
   - 支持 **触摸屏** (单点绝对坐标/常用手势)。
   - 支持 **鼠标** (相对坐标)。
   - 支持 **键盘** (支持键位映射)。
   - 支持 **热插拔**: 自动检测新插入的输入设备（可配置）。
 - **设备过滤**: 支持通过白名单或黑名单过滤特定的输入设备。
 - **TTY 管理**: 自动将控制台切换到图形模式以隐藏光标，并在退出（包括 Ctrl+C）时恢复文本模式。
 - **可配置性**: 支持通过环境变量或代码构建器 (Builder Pattern) 配置设备路径。
 - **日志系统**: 使用 `tracing` 提供结构化日志。

 ## 🚀 安装

 在你的 `Cargo.toml` 中添加依赖：

 ```toml
 [dependencies]
 slint = "1.14" # 或你使用的版本
 slint-backend-linuxfb = { version = "0.1.0" }
 # 如果你需要查看日志，建议添加 tracing-subscriber
 tracing-subscriber = "0.3" 
 ```

 ## ⚠️ 运行时注意事项：键盘支持

 为了确保键盘输入功能（依赖 `xkbcommon`）在目标设备上正常工作，目标主机必须包含 `xkeyboard-config` 数据文件。

 你需要将开发机上的 XKB 数据文件复制到目标主机的对应目录中。

 **示例操作：**

 ```bash
 # 将本机的 /usr/share/X11/xkb 目录复制到目标主机
 # 请根据实际情况修改 SSH 端口、IP 地址和路径
 scp -r /usr/share/X11/xkb root@x.x.x.x:/usr/share/X11/xkb
 ```

 ## 📖 使用方法

 ### 1. 简单模式 (默认配置)

 最简单的使用方式是调用 `init()`。它会自动寻找 `/dev/fb0`，使用 `/dev/tty1`，并扫描所有输入设备。

 ```rust
 fn main() -> Result<(), Box<dyn std::error::Error>> {
     // 1. 初始化后端
     slint_backend_linuxfb::init()?;
     
     // 2. 你的 Slint 应用逻辑...
     let main_window = MainWindow::new()?;
     main_window.run()?;
     
     Ok(())
 }
 ```

 ### 2. 高级模式 (Builder 模式)

 如果你需要指定设备路径、过滤输入设备，可以使用 `LinuxFbPlatformBuilder`。

 ```rust
 use slint_backend_linuxfb::LinuxFbPlatformBuilder;

 fn main() -> Result<(), Box<dyn std::error::Error>> {
     // 初始化日志打印 (可选，但推荐用于调试)
     tracing_subscriber::fmt::init();

     // 配置平台
     let platform = LinuxFbPlatformBuilder::new()
         .with_framebuffer("/dev/fb1")          // 指定显示设备
         .with_tty("/dev/tty3")                 // 指定控制台终端
         .with_input_autodiscovery(true)        // 开启热插拔支持
         .with_input_whitelist(vec![            // 仅加载特定的输入设备
             "Goodix Capacitive TouchScreen".to_string(),
             "My Keyboard".to_string()
         ])
         // .with_input_blacklist(vec!["Mouse".to_string()]) // 或者排除某些设备
         .build()?;

     // 应用平台
     i_slint_core::platform::set_platform(Box::new(platform))?;

     // 运行 Slint 应用
     // ...
     Ok(())
 }
 ```

 ## ⚙️ 配置与环境变量

 除了代码配置，你也使用环境变量来覆盖默认行为（优先级：代码配置 > 环境变量 > 默认值）。

 | 环境变量 | 描述 | 默认值 |
 |---|---|---|
 | `SLINT_FRAMEBUFFER` | Framebuffer 设备路径 | `/dev/fb0` |
 | `SLINT_TTY_DEVICE` | 用于图形模式切换的 TTY 路径 | `/dev/tty1` (失败则尝试 tty0) |
 | `XKB_DEFAULT_RULES` | XKB 规则文件 | 系统默认 |
 | `XKB_DEFAULT_MODEL` | 键盘型号 (Model) | 系统默认 |
 | `XKB_DEFAULT_LAYOUT` | 键盘布局 (Layout, 逗号分隔) | 系统默认 |
 | `XKB_DEFAULT_VARIANT` | 布局变体 (Variant, 逗号分隔) | 系统默认 |
 | `XKB_DEFAULT_OPTIONS` | 额外选项 (Options, 逗号分隔) | 系统默认 |

 ## ⚖️ License

 MIT License