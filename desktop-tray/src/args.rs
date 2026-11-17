//! 极简命令行解析。参数只有个位数，引入 clap 反而增加体积。

use std::path::PathBuf;

/// bili-sync 管理页的默认端口，与 bili-sync 的 `default_bind_address` 保持一致。
pub const DEFAULT_PORT: u16 = 12345;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Args {
    /// bili-sync-rs 可执行文件路径；不传则自动查找。
    pub bin: Option<PathBuf>,
    /// 传给 bili-sync 的配置目录（`-c`），不传则用它自己的默认目录。
    pub config_dir: Option<PathBuf>,
    /// 管理页端口，不传则用 12345。只在探测/打开浏览器时使用。
    pub port: Option<u16>,
    /// 传给 bili-sync 的 ffmpeg 路径（`-f`），不传则自动探测。
    pub ffmpeg_path: Option<PathBuf>,
    /// 启动后不自动打开管理页，适合开机自启场景。
    pub no_open: bool,
    /// 只探测服务状态后退出：0=运行中，1=未运行。便于脚本/CI 冒烟。
    pub check: bool,
}

#[derive(Debug)]
pub enum Parsed {
    Run(Args),
    Help,
    Version,
}

impl Args {
    pub fn parse<I>(argv: I) -> Result<Parsed, String>
    where
        I: IntoIterator<Item = String>,
    {
        let mut args = Args::default();
        let mut iter = argv.into_iter();
        while let Some(raw) = iter.next() {
            // 同时支持 `--port 1234` 和 `--port=1234`
            let (flag, inline) = match raw.split_once('=') {
                Some((f, v)) if f.starts_with("--") => (f.to_string(), Some(v.to_string())),
                _ => (raw.clone(), None),
            };
            let mut take_value = |name: &str| -> Result<String, String> {
                if let Some(v) = inline.clone() {
                    if v.is_empty() {
                        return Err(format!("参数 {name} 的取值为空"));
                    }
                    return Ok(v);
                }
                iter.next().ok_or_else(|| format!("参数 {name} 缺少取值"))
            };
            match flag.as_str() {
                "-h" | "--help" => return Ok(Parsed::Help),
                "-V" | "--version" => return Ok(Parsed::Version),
                "-b" | "--bin" => args.bin = Some(PathBuf::from(take_value("--bin")?)),
                "-c" | "--config-dir" => args.config_dir = Some(PathBuf::from(take_value("--config-dir")?)),
                "-f" | "--ffmpeg-path" => args.ffmpeg_path = Some(PathBuf::from(take_value("--ffmpeg-path")?)),
                "-p" | "--port" => {
                    let raw_port = take_value("--port")?;
                    let port: u16 = raw_port
                        .parse()
                        .map_err(|_| format!("端口必须是 1-65535 的整数：{raw_port}"))?;
                    if port == 0 {
                        return Err("端口不能为 0".to_string());
                    }
                    args.port = Some(port);
                }
                "--no-open" => args.no_open = true,
                "--check" => args.check = true,
                other => return Err(format!("未知参数：{other}")),
            }
        }
        Ok(Parsed::Run(args))
    }

    pub fn usage() -> String {
        let mut out = String::new();
        out.push_str("bili-sync 桌面托盘外壳\n\n");
        out.push_str("用法：bili-sync-tray [选项]\n\n");
        out.push_str("选项：\n");
        out.push_str("  -b, --bin <PATH>          bili-sync-rs 路径（默认：同目录 → PATH → target/release）\n");
        out.push_str("  -c, --config-dir <PATH>   传给 bili-sync 的配置目录\n");
        out.push_str(&format!(
            "  -p, --port <PORT>         管理页端口，默认 {DEFAULT_PORT}\n"
        ));
        out.push_str("  -f, --ffmpeg-path <PATH>  传给 bili-sync 的 ffmpeg 路径（默认自动探测）\n");
        out.push_str("      --no-open             启动后不自动打开管理页\n");
        out.push_str("      --check               只探测服务状态后退出（0=运行中，1=未运行）\n");
        out.push_str("  -h, --help                显示帮助\n");
        out.push_str("  -V, --version             显示版本\n");
        out
    }
}

#[cfg(test)]
mod tests {
    use super::{Args, DEFAULT_PORT, Parsed};

    fn run(argv: &[&str]) -> Args {
        match Args::parse(argv.iter().map(|s| s.to_string())).expect("should parse") {
            Parsed::Run(args) => args,
            _ => panic!("expected run args"),
        }
    }

    #[test]
    fn defaults() {
        let args = run(&[]);
        assert_eq!(args, Args::default());
        assert!(args.port.is_none());
        assert!(!args.no_open);
    }

    #[test]
    fn parses_values() {
        let args = run(&[
            "--bin",
            "/tmp/bili-sync-rs",
            "-p",
            "12399",
            "-c",
            "/tmp/cfg",
            "-f",
            "/opt/homebrew/bin/ffmpeg",
            "--no-open",
        ]);
        assert_eq!(args.bin.unwrap().to_str(), Some("/tmp/bili-sync-rs"));
        assert_eq!(args.port, Some(12399));
        assert_eq!(args.config_dir.unwrap().to_str(), Some("/tmp/cfg"));
        assert_eq!(args.ffmpeg_path.unwrap().to_str(), Some("/opt/homebrew/bin/ffmpeg"));
        assert!(args.no_open);
    }

    #[test]
    fn parses_equals_form() {
        let args = run(&["--port=23456", "--config-dir=/tmp/x y"]);
        assert_eq!(args.port, Some(23456));
        assert_eq!(args.config_dir.unwrap().to_str(), Some("/tmp/x y"));
    }

    #[test]
    fn rejects_bad_input() {
        let err = Args::parse(["--port".to_string()]).unwrap_err();
        assert!(err.contains("缺少取值"), "{err}");
        let err = Args::parse(["--port".to_string(), "0".to_string()]).unwrap_err();
        assert!(err.contains("不能为 0"), "{err}");
        let err = Args::parse(["--port".to_string(), "70000".to_string()]).unwrap_err();
        assert!(err.contains("端口"), "{err}");
        let err = Args::parse(["--nope".to_string()]).unwrap_err();
        assert!(err.contains("未知参数"), "{err}");
    }

    #[test]
    fn detects_help_and_version() {
        assert!(matches!(Args::parse(["-h".to_string()]), Ok(Parsed::Help)));
        assert!(matches!(Args::parse(["--version".to_string()]), Ok(Parsed::Version)));
        assert!(Args::usage().contains(&DEFAULT_PORT.to_string()));
    }
}
