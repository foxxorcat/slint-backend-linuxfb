// 引入生成的 UI 模块
slint::include_modules!();

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 初始化日志
    tracing_subscriber::fmt::init();

    // 初始化 Framebuffer 后端
    if let Err(e) = slint_backend_linuxfb::init() {
        eprintln!("错误: 无法初始化 Framebuffer 后端: {}", e);
        return Ok(());
    }

    // 创建并运行 UI
    let app = DemoApp::new()?;
    
    println!("UI 已启动。按 Ctrl+C 退出。");
    app.run()?;

    Ok(())
}