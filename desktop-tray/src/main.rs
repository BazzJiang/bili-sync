//! bili-sync 桌面托盘外壳的入口。
//!
//! 目标：双击启动一个常驻程序 → 托盘出现图标 → 自动拉起 bili-sync 并打开管理页；
//! 之后点图标就能打开管理页、启动/停止/重启服务。全程没有控制台窗口（Windows 下
//! 使用 windows 子系统），日志写文件。
//!
//! 进程模型：
//! - 托盘壳自己不是服务，bili-sync 是它的子进程（或已被外部程序启动的实例）；
//! - 退出托盘时，若服务是本壳拉起的，会先 SIGTERM 让它优雅退出；
//! - 单实例锁保证不会出现两套托盘去抢同一个服务。

#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod args;
mod http;
mod icon;
mod instance;
mod log;
mod service;
mod util;

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use tray_icon::TrayIcon;
use tray_icon::TrayIconBuilder;
use tray_icon::menu::{IsMenuItem, Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem};
use winit::application::ApplicationHandler;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};

use crate::args::{Args, DEFAULT_PORT, Parsed};
use crate::log::Logger;
use crate::service::{Service, State};

/// 状态刷新间隔：足够快，又不至于每秒醒来一次。
const TICK: Duration = Duration::from_millis(1500);

/// SIGINT/SIGTERM 只置位，实际退出动作留给事件循环（信号处理函数里只能做原子操作）。
static QUIT: AtomicBool = AtomicBool::new(false);

enum UserEvent {
    Menu(MenuEvent),
}

fn main() {
    if let Err(message) = run() {
        eprintln!("bili-sync-tray 错误：{message}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let args = match Args::parse(std::env::args().skip(1))? {
        Parsed::Help => {
            print!("{}", Args::usage());
            return Ok(());
        }
        Parsed::Version => {
            println!("bili-sync-tray {}", env!("CARGO_PKG_VERSION"));
            return Ok(());
        }
        Parsed::Run(args) => args,
    };
    let port = args.port.unwrap_or(DEFAULT_PORT);

    // --check：不初始化 GUI，适合脚本/CI 冒烟
    if args.check {
        let running = http::probe(port);
        println!(
            "{}",
            if running {
                format!("运行中（127.0.0.1:{port}）")
            } else {
                format!("未运行（127.0.0.1:{port}）")
            }
        );
        std::process::exit(if running { 0 } else { 1 });
    }

    let data_dir = data_dir()?;
    std::fs::create_dir_all(&data_dir).map_err(|e| format!("创建数据目录 {} 失败：{e}", data_dir.display()))?;
    let logger = Logger::new(data_dir.join("tray.log"))?;
    logger.write(
        "tray",
        &format!(
            "启动 bili-sync-tray {}（pid {}，端口 {port}）",
            env!("CARGO_PKG_VERSION"),
            std::process::id()
        ),
    );

    // 第二个实例只负责把用户带到管理页，然后自己退出
    let Some(_instance) = instance::acquire(&data_dir)? else {
        logger.write("tray", "已有托盘实例在运行，打开管理页后退出");
        if !args.no_open {
            open_url(port, &logger);
        }
        return Ok(());
    };

    install_signal_handlers();

    let bin = service::resolve_binary(args.bin.clone());
    if let Err(message) = &bin {
        logger.write("tray", message);
    }
    let config_dir = args
        .config_dir
        .clone()
        .or_else(service::default_config_dir)
        .map(|dir| service::normalize_dir(&dir));
    let service = Service::new(
        bin,
        args.config_dir.clone(),
        args.ffmpeg_path.clone(),
        port,
        logger.clone(),
    );

    let mut builder = EventLoop::<UserEvent>::with_user_event();
    #[cfg(target_os = "macos")]
    {
        // 托盘程序不需要 Dock 图标和菜单栏应用菜单，Accessory 更贴近后台工具
        use winit::platform::macos::{ActivationPolicy, EventLoopBuilderExtMacOS};
        builder.with_activation_policy(ActivationPolicy::Accessory);
    }
    let event_loop = builder.build().map_err(|e| format!("创建事件循环失败：{e}"))?;

    // 菜单事件在系统线程上产生，转发到事件循环里统一处理。
    // Windows 的 EventLoopProxy 只保证 Send，不保证 Sync，因此套一层 Mutex 满足回调约束。
    let proxy = Mutex::new(event_loop.create_proxy());
    MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
        if let Ok(proxy) = proxy.lock() {
            let _ = proxy.send_event(UserEvent::Menu(event));
        }
    }));

    let mut app = App::new(service, logger.clone(), port, args.no_open, config_dir);
    event_loop
        .run_app(&mut app)
        .map_err(|e| format!("事件循环异常退出：{e}"))?;
    logger.write("tray", "托盘已退出");
    Ok(())
}

fn data_dir() -> Result<PathBuf, String> {
    dirs::data_local_dir()
        .map(|dir| dir.join("bili-sync-tray"))
        .ok_or_else(|| "无法确定系统数据目录".to_string())
}

fn open_url(port: u16, logger: &Logger) {
    let url = format!("http://127.0.0.1:{port}");
    match open::that(&url) {
        Ok(()) => logger.write("tray", &format!("已打开管理页 {url}")),
        Err(e) => logger.write("tray", &format!("打开浏览器失败：{e}")),
    }
}

#[cfg(unix)]
fn install_signal_handlers() {
    extern "C" fn handle(_signal: libc::c_int) {
        QUIT.store(true, Ordering::SeqCst);
    }
    unsafe {
        libc::signal(libc::SIGTERM, handle as *const () as libc::sighandler_t);
        libc::signal(libc::SIGINT, handle as *const () as libc::sighandler_t);
    }
}

#[cfg(not(unix))]
fn install_signal_handlers() {
    // Windows 上托盘通常没有控制台；退出走菜单里的「退出托盘」。
}

struct App {
    service: Service,
    logger: Logger,
    port: u16,
    no_open: bool,
    config_dir: Option<PathBuf>,
    tray: Option<TrayIcon>,
    status_item: Option<MenuItem>,
    open_item: Option<MenuItem>,
    data_item: Option<MenuItem>,
    log_item: Option<MenuItem>,
    start_item: Option<MenuItem>,
    stop_item: Option<MenuItem>,
    restart_item: Option<MenuItem>,
    quit_item: Option<MenuItem>,
    /// 服务起来后自动打开管理页（启动瞬间端口还没就绪）
    open_when_ready: bool,
    next_tick: Instant,
}

impl App {
    fn new(service: Service, logger: Logger, port: u16, no_open: bool, config_dir: Option<PathBuf>) -> Self {
        Self {
            service,
            logger,
            port,
            no_open,
            config_dir,
            tray: None,
            status_item: None,
            open_item: None,
            data_item: None,
            log_item: None,
            start_item: None,
            stop_item: None,
            restart_item: None,
            quit_item: None,
            open_when_ready: false,
            next_tick: Instant::now() + TICK,
        }
    }

    /// 创建菜单与托盘图标。必须在主线程、且 winit 初始化 AppKit 之后调用。
    fn init_ui(&mut self) -> Result<(), String> {
        let status = MenuItem::new("状态：启动中…", false, None);
        let open = MenuItem::new("打开管理页", true, None);
        let data = MenuItem::new("打开数据目录", true, None);
        let log = MenuItem::new("打开日志", true, None);
        let start = MenuItem::new("启动服务", false, None);
        let stop = MenuItem::new("停止服务", false, None);
        let restart = MenuItem::new("重启服务", false, None);
        let quit = MenuItem::new("退出托盘", true, None);

        let menu = Menu::new();
        let items: [&dyn IsMenuItem; 10] = [
            &status,
            &PredefinedMenuItem::separator(),
            &open,
            &data,
            &log,
            &PredefinedMenuItem::separator(),
            &start,
            &stop,
            &restart,
            &quit,
        ];
        menu.append_items(&items)
            .map_err(|e| format!("构建托盘菜单失败：{e}"))?;

        let (icon, is_template) = icon::build()?;
        let builder = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_tooltip("bili-sync");
        // macOS 的菜单栏图标要用 template，系统才会按明暗主题自动反色。
        // `with_icon_templated` 只有 macOS 后端提供，其它平台必须用普通图标。
        #[cfg(target_os = "macos")]
        let builder = if is_template {
            builder.with_icon_templated(icon)
        } else {
            builder.with_icon(icon)
        };
        #[cfg(not(target_os = "macos"))]
        let builder = {
            let _ = is_template;
            builder.with_icon(icon)
        };
        let tray = builder.build().map_err(|e| format!("创建托盘图标失败：{e}"))?;

        self.status_item = Some(status);
        self.open_item = Some(open);
        self.data_item = Some(data);
        self.log_item = Some(log);
        self.start_item = Some(start);
        self.stop_item = Some(stop);
        self.restart_item = Some(restart);
        self.quit_item = Some(quit);
        self.tray = Some(tray);
        Ok(())
    }

    fn startup(&mut self) {
        match self.service.start() {
            Ok(()) => {
                if self.service.state().serving() {
                    if !self.no_open {
                        self.open_url();
                    }
                } else {
                    self.open_when_ready = !self.no_open;
                }
            }
            Err(message) => {
                self.logger.write("tray", &format!("启动失败：{message}"));
            }
        }
        self.sync_ui();
    }

    fn on_menu(&mut self, event_loop: &ActiveEventLoop, id: &MenuId) {
        if clicked(&self.open_item, id) {
            self.open_url();
        } else if clicked(&self.data_item, id) {
            if let Some(dir) = self.config_dir.clone() {
                self.open_path(&dir, "数据目录");
            }
        } else if clicked(&self.log_item, id) {
            let path = self.logger.path().to_path_buf();
            self.open_path(&path, "日志");
        } else if clicked(&self.start_item, id) {
            if let Err(message) = self.service.start() {
                self.logger.write("tray", &format!("启动失败：{message}"));
            }
        } else if clicked(&self.stop_item, id) {
            match self.service.stop() {
                Ok(()) => self.logger.write("tray", "服务已停止"),
                Err(message) => self.logger.write("tray", &format!("停止失败：{message}")),
            }
        } else if clicked(&self.restart_item, id) {
            match self.service.restart() {
                Ok(()) => self.logger.write("tray", "服务已重启"),
                Err(message) => self.logger.write("tray", &format!("重启失败：{message}")),
            }
        } else if clicked(&self.quit_item, id) {
            self.logger.write("tray", "通过菜单退出");
            event_loop.exit();
            return;
        }
        self.tick(event_loop);
    }

    /// 周期性刷新：子进程状态、外部服务探测、就绪后开浏览器。
    fn tick(&mut self, event_loop: &ActiveEventLoop) {
        if QUIT.load(Ordering::SeqCst) {
            self.logger.write("tray", "收到退出信号");
            event_loop.exit();
            return;
        }

        self.service.refresh();
        if self.open_when_ready && self.service.state().serving() {
            self.open_when_ready = false;
            if !self.no_open {
                self.open_url();
            }
        }
        self.sync_ui();
        self.next_tick = Instant::now() + TICK;
        event_loop.set_control_flow(ControlFlow::WaitUntil(self.next_tick));
    }

    fn sync_ui(&mut self) {
        let state = self.service.state().clone();
        let label = state.label();
        if let Some(item) = &self.status_item {
            item.set_text(format!("状态：{label}"));
        }
        if let Some(tray) = &self.tray {
            let _ = tray.set_tooltip(Some(format!("bili-sync · {label}")));
        }
        let can_start = matches!(state, State::Stopped | State::Failed { .. });
        let managed = state.managed();
        let has_config_dir = self.config_dir.as_ref().is_some_and(|dir| dir.is_dir());
        if let Some(item) = &self.start_item {
            item.set_enabled(can_start);
        }
        if let Some(item) = &self.stop_item {
            item.set_enabled(managed);
        }
        if let Some(item) = &self.restart_item {
            item.set_enabled(managed);
        }
        if let Some(item) = &self.data_item {
            item.set_enabled(has_config_dir);
        }
    }

    fn open_url(&self) {
        open_url(self.port, &self.logger);
    }

    fn open_path(&self, path: &Path, what: &str) {
        match open::that(path) {
            Ok(()) => self.logger.write("tray", &format!("已打开{what}：{}", path.display())),
            Err(e) => self.logger.write("tray", &format!("打开{what}失败：{e}")),
        }
    }

    fn shutdown(&mut self) {
        if self.service.state().managed() {
            self.logger.write("tray", "正在退出，停止由托盘管理的服务");
            if let Err(message) = self.service.stop() {
                self.logger.write("tray", &format!("停止服务时出错：{message}"));
            }
        } else {
            // 外部管理的实例（launchd/systemd/用户手动启动）不归托盘管，不能跟着一起停
            self.logger.write("tray", "服务由外部管理，托盘退出后保持运行");
        }
    }
}

fn clicked(item: &Option<MenuItem>, id: &MenuId) -> bool {
    item.as_ref().is_some_and(|item| item.id().as_ref() == id.as_ref())
}

impl ApplicationHandler<UserEvent> for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        // desktop 平台只会来一次；这里做个幂等保护
        if self.tray.is_some() {
            return;
        }
        if let Err(message) = self.init_ui() {
            self.logger.write("tray", &format!("托盘初始化失败：{message}"));
            eprintln!("bili-sync-tray 初始化失败：{message}");
            event_loop.exit();
            return;
        }
        self.startup();
        self.next_tick = Instant::now() + TICK;
        event_loop.set_control_flow(ControlFlow::WaitUntil(self.next_tick));
    }

    fn window_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _window_id: winit::window::WindowId,
        _event: winit::event::WindowEvent,
    ) {
        // 托盘壳没有窗口，这个方法仅用于满足 trait
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: UserEvent) {
        match event {
            UserEvent::Menu(event) => self.on_menu(event_loop, event.id()),
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if self.tray.is_some() && Instant::now() >= self.next_tick {
            self.tick(event_loop);
        } else {
            event_loop.set_control_flow(ControlFlow::WaitUntil(self.next_tick));
        }
    }

    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {
        self.shutdown();
    }
}
