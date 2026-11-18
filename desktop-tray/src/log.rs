//! 日志与子进程输出收集。
//!
//! 托盘常驻后台，没有终端可看：把 bili-sync 的 stdout/stderr 追加到文件，
//! 菜单里的「打开日志」直接打开它。日志按行写入，行首带时间和来源标签。

use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::Child;
use std::sync::{Arc, Mutex};
use std::thread;

use crate::util::utc_timestamp;

#[derive(Clone)]
pub struct Logger {
    inner: Arc<Mutex<File>>,
    path: PathBuf,
}

impl Logger {
    pub fn new(path: PathBuf) -> Result<Self, String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("创建日志目录 {} 失败：{e}", parent.display()))?;
        }
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .map_err(|e| format!("打开日志文件 {} 失败：{e}", path.display()))?;
        Ok(Self {
            inner: Arc::new(Mutex::new(file)),
            path,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// 写一行日志。写失败不抛错：日志本身不应该影响托盘存活。
    pub fn write(&self, tag: &str, message: &str) {
        // 子进程的 tracing 输出带 ANSI 颜色控制符（管道下也照样输出），
        // 直接落盘会让日志里满是 `[2m`、`[32m` 这类噪声，这里统一剥掉。
        let message = strip_ansi(message);
        let line = format!("[{}] [{tag}] {message}\n", utc_timestamp());
        if let Ok(mut file) = self.inner.lock() {
            let _ = file.write_all(line.as_bytes());
            let _ = file.flush();
        }
    }

    /// 接管子进程的 stdout/stderr，逐行转发到日志文件。
    pub fn attach_child(&self, child: &mut Child) {
        // 子进程必须由 spawn 时的 Stdio::piped() 提供管道，否则这里拿不到
        if let Some(stdout) = child.stdout.take() {
            self.pump(stdout, "bili-sync");
        }
        if let Some(stderr) = child.stderr.take() {
            self.pump(stderr, "bili-sync!");
        }
    }

    fn pump<R>(&self, reader: R, tag: &'static str)
    where
        R: Read + Send + 'static,
    {
        let logger = self.clone();
        thread::spawn(move || {
            let mut reader = BufReader::new(reader);
            let mut buf = Vec::new();
            loop {
                buf.clear();
                match reader.read_until(b'\n', &mut buf) {
                    Ok(0) => break,
                    Ok(_) => {
                        let line = String::from_utf8_lossy(&buf);
                        let line = line.trim_end_matches(['\r', '\n']);
                        if !line.is_empty() {
                            logger.write(tag, line);
                        }
                    }
                    Err(_) => break,
                }
            }
        });
    }
}

/// 去掉 ANSI 转义序列（CSI / OSC / 其它两字节转义）。
fn strip_ansi(input: &str) -> String {
    if !input.contains('\u{1b}') {
        return input.to_string();
    }
    let bytes = input.as_bytes();
    let mut out = String::with_capacity(input.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == 0x1b {
            i += 1;
            if i >= bytes.len() {
                break;
            }
            match bytes[i] {
                b'[' => {
                    // CSI：参数/中间字节之后跟一个 0x40-0x7E 的终止字节
                    i += 1;
                    while i < bytes.len() && !(0x40..=0x7e).contains(&bytes[i]) {
                        i += 1;
                    }
                    i += 1;
                }
                b']' => {
                    // OSC：BEL 或 ST(ESC \) 结尾
                    i += 1;
                    while i < bytes.len() {
                        if bytes[i] == 0x07 {
                            i += 1;
                            break;
                        }
                        if bytes[i] == 0x1b && i + 1 < bytes.len() && bytes[i + 1] == b'\\' {
                            i += 2;
                            break;
                        }
                        i += 1;
                    }
                }
                _ => i += 1,
            }
        } else {
            let ch = input[i..].chars().next().expect("valid utf-8 boundary");
            out.push(ch);
            i += ch.len_utf8();
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{Logger, strip_ansi};
    use std::io::Write;

    #[test]
    fn appends_lines() {
        let dir = std::env::temp_dir().join(format!("bili-sync-tray-log-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let logger = Logger::new(dir.join("tray.log")).expect("logger");
        logger.write("tray", "第一行");
        logger.write("tray", "第二行");
        let content = std::fs::read_to_string(logger.path()).expect("read log");
        assert!(content.contains("第一行"), "{content}");
        assert!(content.contains("第二行"), "{content}");
        assert_eq!(content.lines().count(), 2);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn pipes_child_output() {
        // 用一个会向 stdout 写内容的子进程验证转发逻辑
        let dir = std::env::temp_dir().join(format!("bili-sync-tray-pipe-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let logger = Logger::new(dir.join("tray.log")).expect("logger");
        let mut child = if cfg!(windows) {
            let mut cmd = std::process::Command::new("cmd");
            cmd.args(["/C", "echo hello-from-child"]);
            cmd.stdout(std::process::Stdio::piped());
            cmd.stderr(std::process::Stdio::piped());
            cmd.spawn().expect("spawn")
        } else {
            let mut cmd = std::process::Command::new("sh");
            cmd.args(["-c", "echo hello-from-child"]);
            cmd.stdout(std::process::Stdio::piped());
            cmd.stderr(std::process::Stdio::piped());
            cmd.spawn().expect("spawn")
        };
        logger.attach_child(&mut child);
        let _ = child.wait();
        // 等转发线程落盘
        for _ in 0..50 {
            if std::fs::read_to_string(logger.path())
                .map(|s| s.contains("hello-from-child"))
                .unwrap_or(false)
            {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let content = std::fs::read_to_string(logger.path()).expect("read log");
        assert!(content.contains("hello-from-child"), "{content}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn strips_ansi_escapes() {
        let colored = "\u{1b}[2mNov 18 06:12:07\u{1b}[0m \u{1b}[32m INFO\u{1b}[0m 开始执行本轮视频下载任务..";
        assert_eq!(strip_ansi(colored), "Nov 18 06:12:07  INFO 开始执行本轮视频下载任务..");
        assert_eq!(strip_ansi("纯文本 without escapes"), "纯文本 without escapes");
        assert_eq!(strip_ansi("\u{1b}]0;标题\u{7}content"), "content");
    }

    #[test]
    fn create_parent_dirs() {
        let dir = std::env::temp_dir().join(format!("bili-sync-tray-nested-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let logger = Logger::new(dir.join("a/b/c.log")).expect("logger");
        logger.write("tray", "ok");
        assert!(logger.path().exists());
        let mut f = std::fs::OpenOptions::new().append(true).open(logger.path()).unwrap();
        f.write_all(b"").unwrap();
        let _ = std::fs::remove_dir_all(&dir);
    }
}
