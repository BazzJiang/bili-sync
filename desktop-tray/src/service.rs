//! bili-sync 服务的生命周期管理：探测、拉起、停止、状态刷新。
//!
//! 与 bili-sync 的约定：
//! - 它已经实现了 SIGINT/SIGTERM 优雅退出（`utils/signal.rs`），Unix 下用 SIGTERM 停止；
//! - Windows 没有 SIGTERM，只能 TerminateProcess（SQLite 是 WAL 模式，可恢复，但会丢失
//!   当前下载任务的进度），所以这里在 Windows 下明确降级；
//! - 鉴权用 `/api/config` 探测：不带 token 返回 401 即证明服务活着。

use std::env;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

use crate::{http, log::Logger};

#[cfg(unix)]
use std::time::{Duration, Instant};

/// 等子进程优雅退出的上限；超过就强杀。Windows 没有 SIGTERM，用不到这个常量。
#[cfg(unix)]
const STOP_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum State {
    Stopped,
    Starting { pid: u32 },
    RunningManaged { pid: u32 },
    RunningExternal,
    Failed { message: String },
}

impl State {
    /// 菜单里展示的中文状态。
    pub fn label(&self) -> String {
        match self {
            State::Stopped => "已停止".to_string(),
            State::Starting { .. } => "启动中…".to_string(),
            State::RunningManaged { .. } => "运行中（托盘管理）".to_string(),
            State::RunningExternal => "运行中（外部进程）".to_string(),
            State::Failed { message } => {
                format!("异常：{}", crate::util::truncate(message, 48))
            }
        }
    }

    /// 是否由本托盘拉起（只有这种才允许停止/重启）。
    pub fn managed(&self) -> bool {
        matches!(self, State::Starting { .. } | State::RunningManaged { .. })
    }

    /// 管理页是否真的可用（用于「就绪后再打开浏览器」）。
    pub fn serving(&self) -> bool {
        matches!(self, State::RunningManaged { .. } | State::RunningExternal)
    }

    pub fn failed_message(&self) -> Option<&str> {
        match self {
            State::Failed { message } => Some(message),
            _ => None,
        }
    }
}

pub struct Service {
    bin: Option<PathBuf>,
    /// 传给 bili-sync 的额外参数（-c / -f）
    extra_args: Vec<OsString>,
    port: u16,
    log: Logger,
    child: Option<Child>,
    state: State,
}

impl Service {
    /// `bin` 传入 `resolve_binary` 的结果：找不到可执行文件时托盘仍然要能起来并提示错误。
    pub fn new(
        bin: Result<PathBuf, String>,
        config_dir: Option<PathBuf>,
        ffmpeg_path: Option<PathBuf>,
        port: u16,
        log: Logger,
    ) -> Self {
        let (bin, state) = match bin {
            Ok(path) => (Some(path), State::Stopped),
            Err(message) => (None, State::Failed { message }),
        };
        let mut extra_args = Vec::new();
        if let Some(dir) = config_dir {
            extra_args.push(OsString::from("-c"));
            extra_args.push(dir.into_os_string());
        }
        // GUI 启动时 PATH 往往不含 Homebrew 目录，显式探测 ffmpeg 并传绝对路径
        if let Some(ffmpeg) = ffmpeg_path.or_else(detect_ffmpeg) {
            extra_args.push(OsString::from("-f"));
            extra_args.push(ffmpeg.into_os_string());
        }
        Self {
            bin,
            extra_args,
            port,
            log,
            child: None,
            state,
        }
    }

    pub fn state(&self) -> &State {
        &self.state
    }

    /// 启动服务（或识别出外部已有实例）。不等待就绪，就绪由 `refresh` 异步确认。
    pub fn start(&mut self) -> Result<(), String> {
        if http::probe(self.port) {
            self.log.write("tray", "管理页已在运行（不是本托盘启动的），跳过启动");
            self.state = State::RunningExternal;
            return Ok(());
        }
        if self.child.is_some() {
            return Ok(());
        }
        let Some(bin) = self.bin.clone() else {
            let message = self
                .state
                .failed_message()
                .unwrap_or("未找到 bili-sync 可执行文件")
                .to_string();
            self.state = State::Failed {
                message: message.clone(),
            };
            return Err(message);
        };

        let mut command = Command::new(&bin);
        command
            .args(&self.extra_args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = match command.spawn() {
            Ok(child) => child,
            Err(e) => {
                let message = format!("启动 {} 失败：{e}", bin.display());
                self.log.write("tray", &message);
                self.state = State::Failed {
                    message: message.clone(),
                };
                return Err(message);
            }
        };
        let pid = child.id();
        self.log.attach_child(&mut child);
        self.log.write(
            "tray",
            &format!(
                "已启动 {}（pid {pid}），等待 127.0.0.1:{} 就绪",
                bin.display(),
                self.port
            ),
        );
        self.child = Some(child);
        self.state = State::Starting { pid };
        Ok(())
    }

    /// 停止服务。只停止由本托盘拉起的进程，外部管理的实例不受影响。
    pub fn stop(&mut self) -> Result<(), String> {
        let Some(mut child) = self.child.take() else {
            if matches!(self.state, State::RunningExternal) {
                return Err("服务不是由托盘启动的，托盘不会停止它".to_string());
            }
            self.state = State::Stopped;
            return Ok(());
        };
        let pid = child.id();
        self.log.write("tray", &format!("正在停止 bili-sync（pid {pid}）"));

        #[cfg(unix)]
        {
            // SIGTERM：bili-sync 会走优雅退出流程（清理临时文件、关闭数据库）
            unsafe {
                libc::kill(pid as libc::pid_t, libc::SIGTERM);
            }
            let deadline = Instant::now() + STOP_TIMEOUT;
            loop {
                match child.try_wait() {
                    Ok(Some(status)) => {
                        self.log.write("tray", &format!("bili-sync 已优雅退出（{status}）"));
                        self.state = State::Stopped;
                        return Ok(());
                    }
                    Ok(None) if Instant::now() >= deadline => {
                        self.log.write("tray", "等待优雅退出超时，强制结束 bili-sync");
                        break;
                    }
                    Ok(None) => std::thread::sleep(Duration::from_millis(100)),
                    Err(e) => {
                        self.log.write("tray", &format!("等待子进程退出时出错：{e}"));
                        break;
                    }
                }
            }
        }
        #[cfg(not(unix))]
        {
            self.log
                .write("tray", "当前平台没有 SIGTERM，直接结束进程（WAL 模式下数据库可恢复）");
        }

        let _ = child.kill();
        let _ = child.wait();
        self.state = State::Stopped;
        Ok(())
    }

    pub fn restart(&mut self) -> Result<(), String> {
        self.stop()?;
        self.start()
    }

    /// 刷新状态：子进程是否还活着、管理页是否可达。
    pub fn refresh(&mut self) {
        if let Some(child) = self.child.as_mut() {
            match child.try_wait() {
                Ok(Some(status)) => {
                    self.log.write("tray", &format!("bili-sync 已退出（{status}）"));
                    self.child = None;
                    self.state = State::Failed {
                        message: format!("进程已退出（{status}），详情见日志"),
                    };
                    return;
                }
                Ok(None) => {}
                Err(e) => self.log.write("tray", &format!("查询子进程状态失败：{e}")),
            }
        }

        let up = http::probe(self.port);
        let state = self.state.clone();
        match state {
            State::Starting { pid } if up => {
                self.log.write("tray", "管理页已就绪");
                self.state = State::RunningManaged { pid };
            }
            State::Stopped if up => {
                self.state = State::RunningExternal;
            }
            State::RunningExternal if !up => {
                self.log.write("tray", "外部管理页已不可达，状态回到已停止");
                self.state = State::Stopped;
            }
            // 之前启动失败、但用户后来自己把服务跑起来了：自动接管为「外部运行中」
            State::Failed { .. } if up => {
                self.log.write("tray", "检测到服务已在运行，转为外部进程管理");
                self.state = State::RunningExternal;
            }
            _ => {}
        }
    }
}

impl Drop for Service {
    fn drop(&mut self) {
        // 兜底：进程退出时不要留下孤儿服务（正常路径在 exiting() 里已停过）
        if self.child.is_some() {
            let _ = self.stop();
        }
    }
}

/// 解析 bili-sync 可执行文件位置，顺序：--bin → 环境变量 → 同目录 → PATH → 开发态 target/release。
pub fn resolve_binary(explicit: Option<PathBuf>) -> Result<PathBuf, String> {
    let name = binary_name();
    if let Some(path) = explicit {
        if path.is_file() {
            return Ok(path);
        }
        return Err(format!("--bin 指定的文件不存在：{}", path.display()));
    }
    if let Some(path) = env::var_os("BILI_SYNC_TRAY_BIN").map(PathBuf::from) {
        if path.is_file() {
            return Ok(path);
        }
        return Err(format!(
            "环境变量 BILI_SYNC_TRAY_BIN 指向的文件不存在：{}",
            path.display()
        ));
    }
    if let Ok(exe) = env::current_exe()
        && let Some(dir) = exe.parent()
    {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Ok(candidate);
        }
    }
    if let Some(found) = find_in_path(name) {
        return Ok(found);
    }
    // 开发态：从仓库根目录 cargo run
    let dev = PathBuf::from("target").join("release").join(name);
    if dev.is_file() {
        return Ok(dev);
    }
    Err(format!(
        "未找到 {name}。请把它放到托盘程序同目录，或使用 --bin 指定路径"
    ))
}

pub fn binary_name() -> &'static str {
    if cfg!(windows) {
        "bili-sync-rs.exe"
    } else {
        "bili-sync-rs"
    }
}

fn find_in_path(name: &str) -> Option<PathBuf> {
    let paths = env::var_os("PATH")?;
    env::split_paths(&paths)
        .map(|dir| dir.join(name))
        .find(|path| path.is_file())
}

/// 探测 ffmpeg：GUI 启动时 PATH 很干净，先看常见安装位置再退回 PATH。
pub fn detect_ffmpeg() -> Option<PathBuf> {
    const CANDIDATES: &[&str] = &["/opt/homebrew/bin/ffmpeg", "/usr/local/bin/ffmpeg", "/usr/bin/ffmpeg"];
    CANDIDATES
        .iter()
        .map(PathBuf::from)
        .find(|path| path.is_file())
        .or_else(|| find_in_path("ffmpeg"))
}

/// bili-sync 自己的默认配置目录（`dirs::config_dir()/bili-sync`）。
pub fn default_config_dir() -> Option<PathBuf> {
    dirs::config_dir().map(|dir| dir.join("bili-sync"))
}

/// 打开数据目录时用：相对路径要基于当前工作目录解析。
pub fn normalize_dir(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::{State, binary_name, default_config_dir, detect_ffmpeg, normalize_dir, resolve_binary};

    #[test]
    fn state_labels_and_flags() {
        assert_eq!(State::Stopped.label(), "已停止");
        assert!(State::Starting { pid: 1 }.managed());
        assert!(!State::Starting { pid: 1 }.serving());
        assert!(State::RunningManaged { pid: 2 }.serving());
        assert!(!State::RunningExternal.managed());
        assert!(State::RunningExternal.serving());
        assert!(State::Failed { message: "boom".into() }.label().contains("boom"));
    }

    #[test]
    fn explicit_missing_binary_is_an_error() {
        let err = resolve_binary(Some("/definitely/not/here/bili-sync-rs".into())).unwrap_err();
        assert!(err.contains("--bin"), "{err}");
    }

    #[test]
    fn binary_name_matches_platform() {
        if cfg!(windows) {
            assert_eq!(binary_name(), "bili-sync-rs.exe");
        } else {
            assert_eq!(binary_name(), "bili-sync-rs");
        }
    }

    #[test]
    fn normalize_keeps_missing_path() {
        let path = std::path::Path::new("/definitely/not/here");
        assert_eq!(normalize_dir(path), path);
    }

    #[test]
    fn default_config_dir_ends_with_bili_sync() {
        if let Some(dir) = default_config_dir() {
            assert!(dir.ends_with("bili-sync"), "{}", dir.display());
        }
    }

    #[test]
    fn ffmpeg_detection_is_optional_but_never_panics() {
        // 只要求不 panic：有些 CI 环境确实没有 ffmpeg
        let _ = detect_ffmpeg();
    }
}
