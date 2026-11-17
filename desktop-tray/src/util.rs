//! 少量通用工具。托盘壳只需要一个日志时间戳，为此引入 chrono 不值得。

use std::time::{SystemTime, UNIX_EPOCH};

/// 当前 UTC 时间，格式 `YYYY-MM-DD HH:MM:SS`。
pub fn utc_timestamp() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or_default();
    format_unix(secs)
}

/// 把 Unix 时间戳格式化为 UTC 日期时间。
///
/// 这里用的是 Howard Hinnant 的 `civil_from_days` 算法：纯整数运算，
/// 不处理时区（日志只求可比对，不需要本地时区）。
pub fn format_unix(secs: u64) -> String {
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    let (y, m, d) = civil_from_days(days);
    format!(
        "{y:04}-{m:02}-{d:02} {:02}:{:02}:{:02}",
        rem / 3600,
        (rem % 3600) / 60,
        rem % 60
    )
}

fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// 按字符截断，用于菜单里显示过长的错误信息。
pub fn truncate(s: &str, max_chars: usize) -> String {
    let mut out: String = s.chars().take(max_chars).collect();
    if s.chars().count() > max_chars {
        out.push('…');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{format_unix, truncate};

    #[test]
    fn formats_known_timestamps() {
        assert_eq!(format_unix(0), "1970-01-01 00:00:00");
        assert_eq!(format_unix(1_700_000_000), "2023-11-14 22:13:20");
        assert_eq!(format_unix(1_798_000_000), "2026-12-23 04:26:40");
    }

    #[test]
    fn truncates_by_chars() {
        assert_eq!(truncate("abcdef", 3), "abc…");
        assert_eq!(truncate("中文测试", 2), "中文…");
        assert_eq!(truncate("abc", 10), "abc");
    }
}
