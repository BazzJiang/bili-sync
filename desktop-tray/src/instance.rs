//! 单实例保护。
//!
//! 托盘重复启动会出现两个图标、两套菜单，且第二个实例会重复探测/拉起服务。
//! 用跨平台的 flock（fslock）：进程崩溃时锁由内核自动释放，不需要处理 PID 复用。

use std::path::Path;

use fslock::LockFile;

const LOCK_FILE: &str = "desktop.lock";

/// 持有它就代表当前是唯一实例；Drop 时自动解锁。
pub struct InstanceGuard {
    _lock: LockFile,
}

/// 尝试获取单实例锁。
///
/// - `Ok(Some(guard))`：拿到锁，本实例继续运行；
/// - `Ok(None)`：已有实例在运行；
/// - `Err`：锁文件本身有问题（例如没有写权限）。
pub fn acquire(dir: &Path) -> Result<Option<InstanceGuard>, String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("创建数据目录 {} 失败：{e}", dir.display()))?;
    let path = dir.join(LOCK_FILE);
    let mut lock = LockFile::open(path.as_path()).map_err(|e| format!("打开锁文件 {} 失败：{e}", path.display()))?;
    match lock.try_lock() {
        Ok(true) => Ok(Some(InstanceGuard { _lock: lock })),
        Ok(false) => Ok(None),
        Err(e) => Err(format!("获取单实例锁失败：{e}")),
    }
}

#[cfg(test)]
mod tests {
    use super::acquire;

    #[test]
    fn second_acquire_is_rejected_until_first_is_dropped() {
        let dir = std::env::temp_dir().join(format!("bili-sync-tray-lock-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);

        let first = acquire(&dir).expect("first acquire").expect("first should lock");
        assert!(
            acquire(&dir).expect("second acquire").is_none(),
            "两个实例不应同时拿到锁"
        );
        drop(first);
        assert!(acquire(&dir).expect("third acquire").is_some(), "释放后应能重新获取");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
