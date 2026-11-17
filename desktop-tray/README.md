# bili-sync-tray

bili-sync 的桌面托盘外壳：**不是**新的服务端，只是给已有的 `bili-sync-rs` 套一层常驻 UI。

- 双击/启动后常驻托盘（macOS 菜单栏、Windows 通知区、Linux StatusNotifierItem）；
- 自动拉起 bili-sync 并打开管理页（`http://127.0.0.1:12345`）；
- 托盘菜单里可以打开管理页 / 打开数据目录 / 打开日志 / 启动 / 停止 / 重启服务；
- 退出托盘时，若服务是本壳拉起的，会发 `SIGTERM` 让它优雅退出；外部启动的实例不会被误杀。

```
托盘壳（本程序） ── 子进程 ──> bili-sync-rs（HTTP + 下载任务 + SQLite）
      │
      └─ 托盘图标 / 菜单：只是入口和开关，不做任何业务逻辑
```

## 平台支持

| 平台 | 托盘后端 | 运行依赖 | 状态 |
| --- | --- | --- | --- |
| macOS 11+ | NSStatusBar（template 图标，自动适配明暗） | 无 | 已验证（本文档的 E2E 测试在 macOS arm64 上跑通） |
| Windows 10/11 | Shell_NotifyIcon | 无（不需要 WebView2，不是 Tauri） | 代码层面已适配，需 CI/实机回归 |
| Linux（X11 / Wayland 桌面） | StatusNotifierItem（DBus，**纯 Rust 的 ksni 后端**） | 能提供 SNI host 的桌面环境 + D-Bus session | 代码层面已适配，需 CI/实机回归 |

Linux 说明：

- 编译**不需要** GTK / libappindicator / libwebkit 这些 C 库（`tray-icon` 用了 `ksni` 特性）；
- KDE / Xfce / Cinnamon / MATE 一般自带 SNI host；**GNOME 需要装 [AppIndicator and KStatusNotifierItem Support](https://extensions.gnome.org/extension/615/appindicator-support/) 扩展**，否则图标不显示；
- 若当前环境没有 SNI host，程序仍会正常拉起/接管服务并写日志，只是没有图标可点。

## 构建

需要 Rust 1.90+（`desktop-tray` 是**独立 workspace**，不会进入仓库根 workspace，也不会影响 NAS 侧的 Linux musl 发布构建）。

```bash
cd desktop-tray
cargo build --release
# 产物：desktop-tray/target/release/bili-sync-tray（Windows 为 .exe）
```

沙箱内 `~/.cargo` 不可写时（参考仓库 AGENTS.md）：

```bash
export CARGO_HOME=/tmp/bili-sync-cargo
```

## 使用

```bash
# 1. 最常见：和 bili-sync-rs 放在同一目录，直接运行
./bili-sync-tray

# 2. 指定 bili-sync 位置 / 配置目录 / 端口
./bili-sync-tray --bin /path/to/bili-sync-rs --config-dir /data/bili-sync --port 12345

# 3. 开机自启场景：启动服务但不自动打开浏览器
./bili-sync-tray --no-open

# 4. 脚本/冒烟：只探测管理页是否活着（0=运行中，1=未运行）
./bili-sync-tray --check --port 12345
```

完整参数：

| 参数 | 说明 |
| --- | --- |
| `-b, --bin <PATH>` | `bili-sync-rs` 路径；默认按「同目录 → `BILI_SYNC_TRAY_BIN` → PATH → `target/release/`」查找 |
| `-c, --config-dir <PATH>` | 原样作为 `-c` 传给 bili-sync（不传则用它自己的默认配置目录） |
| `-p, --port <PORT>` | 管理页端口，默认 `12345`（只用于探测和打开浏览器，不修改 bili-sync 的绑定） |
| `-f, --ffmpeg-path <PATH>` | 原样作为 `-f` 传给 bili-sync；不传则自动探测常见安装位置 |
| `--no-open` | 启动后不自动打开管理页 |
| `--check` | 只探测状态后退出，不创建托盘 |
| `-h, --help` / `-V, --version` | 帮助 / 版本 |

> 端口要和 bili-sync 配置里的「绑定地址」一致。`--port` 只是托盘探测用的，
> 想让 bili-sync 换端口请改管理页设置里的绑定地址。

## 进程生命周期

| 场景 | 行为 |
| --- | --- |
| 启动时管理页已在运行 | 判定为「外部进程」接管，**不再拉起第二个实例**，退出时也不会杀掉它 |
| 启动时端口空闲 | 拉起 `bili-sync-rs` 子进程，等 `/api/config` 能响应后（约 1~2s）再打开浏览器 |
| 子进程意外退出 | 托盘状态变「异常」，菜单里可以重新「启动服务」 |
| 菜单「退出托盘」/ 收到 `SIGTERM`、`SIGINT` | 先停子进程（Unix 发 `SIGTERM`，bili-sync 会优雅关闭数据库），再退出 |
| 重复启动托盘 | 单实例锁（flock）拦下第二个实例，并直接帮你打开管理页 |
| 托盘自己被强杀 | 子进程会变成孤儿继续跑；此时重启托盘会**接管**它而不是再拉一个（靠端口探测） |

平台差异：

- **Unix**：用 `SIGTERM` 停止，日志里能看到 bili-sync 的「接收到终止信号…数据库连接已关闭」；
- **Windows**：没有 `SIGTERM`，只能 `TerminateProcess`（SQLite 是 WAL 模式，可恢复，但会丢当前下载进度）。
  后续更优做法是上游加一个 `POST /api/shutdown`（带 token），托盘优先走 HTTP 停服务、再退回强杀；
- **macOS**：托盘是 `Accessory` 策略，不占 Dock 图标。想要 Dock 图标 + 点击弹回窗口，可参考路线 B（Swift + WKWebView 壳）。

## 数据与日志位置

| 内容 | 位置 |
| --- | --- |
| bili-sync 数据库 | bili-sync 自己的配置目录（如 macOS `~/Library/Application Support/bili-sync/data.sqlite`），本壳不改动 |
| 托盘日志（含子进程 stdout/stderr） | `dirs::data_local_dir()/bili-sync-tray/tray.log`：macOS `~/Library/Application Support/bili-sync-tray/tray.log`，Linux `~/.local/share/bili-sync-tray/tray.log`，Windows `%LOCALAPPDATA%\bili-sync-tray\tray.log` |
| 单实例锁 | 同目录下的 `desktop.lock` |

日志只追加不轮转（一个常驻进程的量级很小，需要时手动截断即可）。

## 开机自启

托盘壳本身不写系统自启项，按平台自行配置一次即可：

- **macOS**：LaunchAgent（`~/Library/LaunchAgents/*.plist`，`RunAtLoad` + `ProgramArguments` 指向 `bili-sync-tray`），
  或者「系统设置 → 通用 → 登录项」里添加；
- **Windows**：把快捷方式放进 `shell:startup`，或任务计划程序「登录时」触发；
- **Linux**：`~/.config/autostart/bili-sync-tray.desktop`。

> 如果服务已经交给 launchd/systemd 常驻，托盘可以只当入口用：此时托盘启动时会识别出「外部进程」，
> 不会重复拉起，也不会在退出时误停服务。

## 测试

```bash
cd desktop-tray
cargo test          # 21 个单元测试：参数解析、HTTP 探针、时间戳、图标、单实例锁、日志转发
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```

端到端（用临时配置目录，避免污染真实数据；需要一个已构建的 `bili-sync-rs`）：

```bash
# 准备一个监听 12399 的临时库（把 DB 文件复制出来改 bind_address 即可）
mkdir -p /tmp/bili-sync-e2e
cp ~/Library/Application\ Support/bili-sync/data.sqlite /tmp/bili-sync-e2e/data.sqlite
sqlite3 /tmp/bili-sync-e2e/data.sqlite \
  "update config set data=json_set(data,'\$.bind_address','127.0.0.1:12399') where id=1;"

# 启动托盘：应自动拉起 bili-sync、写日志、--check 返回 0
./target/debug/bili-sync-tray --bin /path/to/bili-sync-rs -c /tmp/bili-sync-e2e -p 12399 --no-open
./target/debug/bili-sync-tray --check -p 12399; echo $?   # 0

# 给托盘发 SIGTERM：日志里应出现 bili-sync 的优雅退出，且 12399 端口释放
```

## 已知限制 / 后续可做

1. **Windows 优雅停止**：如上，建议上游加 `/api/shutdown`；
2. **Linux 托盘可见性**依赖桌面环境（GNOME 扩展），无 SNI host 时无图标；
3. **没有自动更新**，升级靠替换二进制（同一位置直接覆盖即可，重新启动托盘生效）；
4. **没有 Dock 图标/应用内窗口**：这是「托盘 + 浏览器」路线，与 Tauri 的「独立窗口」路线不同；
5. 日志不轮转、不支持开机自启配置（见上文）。
