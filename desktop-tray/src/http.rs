//! 极简 HTTP 健康检查。
//!
//! 托盘只需要知道「管理页有没有起来」，为此拉一个 HTTP 客户端（reqwest/tokio）
//! 会让二进制体积翻几倍。这里直接手写一次 GET：能读到 `HTTP/` 开头的状态行即视为存活。
//!
//! 选 `/api/config` 是因为 bili-sync 的鉴权中间件会先返回 401——
//! 拿到 401 恰好证明「进程活着且 HTTP 服务在跑」，不依赖 token。
//! 根路径 `/` 在 release 前端下要求 gzip/br，否则 406，不适合当探针。

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::time::Duration;

const CONNECT_TIMEOUT: Duration = Duration::from_millis(400);
const IO_TIMEOUT: Duration = Duration::from_millis(800);

/// 探测本地端口上是否有一个 HTTP 服务（预期是 bili-sync）。
pub fn probe(port: u16) -> bool {
    status_code(port).is_some()
}

/// 返回 HTTP 状态码；连不上或不是 HTTP 服务时返回 None。
pub fn status_code(port: u16) -> Option<u16> {
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let mut stream = TcpStream::connect_timeout(&addr, CONNECT_TIMEOUT).ok()?;
    stream.set_read_timeout(Some(IO_TIMEOUT)).ok()?;
    stream.set_write_timeout(Some(IO_TIMEOUT)).ok()?;
    stream
        .write_all(b"GET /api/config HTTP/1.1\r\nHost: 127.0.0.1\r\nAccept-Encoding: gzip\r\nConnection: close\r\n\r\n")
        .ok()?;
    let mut buf = [0u8; 64];
    let n = stream.read(&mut buf).ok()?;
    parse_status_code(&buf[..n])
}

fn parse_status_code(buf: &[u8]) -> Option<u16> {
    let line = std::str::from_utf8(buf).ok()?.lines().next()?;
    if !line.starts_with("HTTP/") {
        return None;
    }
    let mut parts = line.split_whitespace();
    parts.next()?; // 协议版本
    parts.next()?.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::{parse_status_code, probe, status_code};
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;

    #[test]
    fn parses_status_line() {
        assert_eq!(parse_status_code(b"HTTP/1.1 401 Unauthorized\r\n"), Some(401));
        assert_eq!(parse_status_code(b"HTTP/1.0 200 OK\r\n"), Some(200));
        assert_eq!(parse_status_code(b"garbage"), None);
    }

    #[test]
    fn probes_a_fake_server() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().unwrap().port();
        thread::spawn(move || {
            // 两次探测（probe + status_code）各建一个连接
            for _ in 0..2 {
                let Ok((mut stream, _)) = listener.accept() else {
                    return;
                };
                let mut buf = [0u8; 128];
                let _ = stream.read(&mut buf);
                let _ =
                    stream.write_all(b"HTTP/1.1 401 Unauthorized\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
            }
        });
        assert!(probe(port));
        assert_eq!(status_code(port), Some(401));
    }

    #[test]
    fn closed_port_is_not_running() {
        // 先占一个端口再释放，拿到一个几乎确定空闲的端口号
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        assert!(!probe(port));
        assert_eq!(status_code(port), None);
    }
}
