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

#[cfg(test)]
mod tests {
    use super::Logger;
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
