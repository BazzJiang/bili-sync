# 从源码构建

本项目由两部分组成：`web/` 下的 Svelte 前端，以及 `crates/` 下的 Rust 后端。前端构建产物会被 `rust-embed-for-web` 编译进二进制，因此最终只产出一个可执行文件。

> [!IMPORTANT]
> 必须先构建前端，再编译 Rust。`web/build` 目录不入库，跳过前端会让二进制里没有管理页面。

## 前置要求

| 依赖 | 版本 | 说明 |
| --- | --- | --- |
| Rust | 1.98.0 | 由 `rust-toolchain.toml` 固定，使用 rustup 进入项目目录时会自动安装 |
| Bun | 1.x | 构建 `web/`，也可用 npm / pnpm 替代 |
| C 工具链 | clang 或 gcc | `ring`、`libgit2-sys`、`libsqlite3-sys` 需要本地 C 编译器 |
| Git | 任意版本 | `rust-embed-for-web` 是 git 依赖，需要克隆 |
| FFmpeg | 任意版本 | **仅运行期需要**，程序启动时会检查 |

无需安装 OpenSSL、CMake 或 Python：TLS 使用 rustls + ring，SQLite 由 sqlx 内置编译。

## 构建步骤

```bash
# 1. 构建前端，产物输出到 web/build
cd web
bun install --frozen-lockfile
bun run build
cd ..

# 2. 编译后端，产物输出到 target/release/bili-sync-rs
cargo build --release
```

使用 npm 的等价写法：

```bash
cd web && npm ci && npm run build && cd ..
cargo build --release
```

仓库还提供了 `Justfile`：

| 命令 | 作用 |
| --- | --- |
| `just debug` | 构建前端后执行 `cargo run` |
| `just build` | 构建前端并编译 `x86_64-unknown-linux-musl` 发布包，需要交叉编译工具链 |
| `just build-docker` | 在 `just build` 基础上打包 Docker 镜像 |

> [!NOTE]
> `just build` 固定使用 musl 目标，在 macOS 或 Windows 上直接执行会失败，本机开发请使用 `cargo build`。

## 验证构建结果

```bash
# 版本信息（含 git 版本、架构、构建时间）
./target/release/bili-sync-rs --version

# 用临时配置目录启动，避免污染真实配置
./target/release/bili-sync-rs -c /tmp/bili-sync-test

# 访问管理页：注意必须带压缩协商，否则 release 模式会返回 406
curl -s --compressed http://127.0.0.1:12345/

# 未携带 token 应返回 401，携带日志中打印的 auth_token 应返回 200
curl -s http://127.0.0.1:12345/api/config
curl -s -H "Authorization: <auth_token>" http://127.0.0.1:12345/api/config
```

首次运行会在日志中打印 `auth_token`，该信息只打印一次。

## 增量构建

首次构建完成后，`target/` 与 `CARGO_HOME` 中的依赖都会保留，后续只需重新编译受影响的部分。

```bash
# 后端代码改动：只重编译改动的 crate
cargo build --release

# 只做类型检查，不产出二进制，速度更快
cargo check

# 前端改动：必须先重建前端，再重新嵌入
cd web && bun run build && cd ..
cargo build --release

# 依赖没有变化时可以完全断网构建
cargo build --release --offline
```

关键行为：

- `build.rs` 声明了 `cargo::rerun-if-changed=../../web/build`，因此**只有 `web/build` 变化才会触发后端重新链接**；仅修改 `web/src` 不会，必须先执行 `bun run build`。
- 依赖未变化时，增量构建通常在数秒到数十秒内完成；改动 `Cargo.toml` 会触发相关依赖重新编译。
- 产物路径固定为 `target/release/bili-sync-rs`，构建完成后直接覆盖即可。
- 除非依赖损坏，**不要执行 `cargo clean`**，它会清空全部编译缓存。

## 镜像与加速（本机已固化）

仓库内的 `.cargo/config.toml` 已配置好 crates.io 镜像与拉取方式，**对任何 agent、任何 `CARGO_HOME` 都生效**，无需再手动传环境变量：

| 配置 | 作用 |
| --- | --- |
| `[source.crates-io]` → `rsproxy-sparse` | crates.io 改走 `rsproxy.cn`，实测约 8.9 MB/s，远快于官方源 |
| `[net] git-fetch-with-cli = true` | git 依赖交给 git 命令行处理，便于使用镜像改写 |
| `[http] timeout = 120` | 慢速网络下避免过早超时 |

> [!NOTE]
> 镜像偶尔会出现 `SSL connection timeout`，cargo 会自动重试；若持续失败，可临时改用 `https://mirrors.ustc.edu.cn/crates.io-index/`（实测约 1.7 MB/s）。

该文件被 `.gitignore` 忽略，不会进入版本库；换机器或全新 clone 后按需重建：

```bash
mkdir -p .cargo && cat > .cargo/config.toml <<'EOF'
[source.crates-io]
replace-with = "rsproxy-sparse"

[source.rsproxy-sparse]
registry = "sparse+https://rsproxy.cn/index/"

[net]
git-fetch-with-cli = true

[http]
timeout = 120
multiplexing = false
EOF
```

唯一的 git 依赖 `rust-embed-for-web` 通过全局 git 配置镜像：cargo 的 git 命令在 `CARGO_HOME` 下执行，读不到仓库级 git 配置。

```bash
git config --global url."https://ghfast.top/https://github.com/amtoaer/rust-embed-for-web".insteadOf \
  "https://github.com/amtoaer/rust-embed-for-web"
```

> [!NOTE]
> 这条改写只针对上述依赖地址，不影响其它 GitHub 仓库的 fetch / push。若无法写入 `~/.gitconfig`，可临时用 `GIT_CONFIG_GLOBAL=/path/to/gitconfig` 指定配置文件。

## 给自动化 agent 的检查清单

1. 用 `command -v cargo bun` 确认工具链存在，**不要重复安装**已有环境。
2. 执行 `cargo build --release` 前先确认 `web/build/index.html` 存在；不存在则执行 `cd web && bun install --frozen-lockfile && bun run build`。
3. 若 `~/.cargo` 不可写（沙箱环境），设置 `CARGO_HOME=/tmp/bili-sync-cargo` 即可，仓库内的 `.cargo/config.toml` 会自动生效。
4. 构建完成后用 `./target/release/bili-sync-rs --version` 校验产物。
5. 增量构建不要执行 `cargo clean`；仅修改前端时必须重建 `web/build`。
6. 网络异常时先确认上一节的镜像配置是否仍在，而不是直接重试整轮下载。

## 常见问题

### git 依赖拉取失败

`rust-embed-for-web` 来自 GitHub 的 tag，网络受限时会卡在 `Updating git repository`。本机已按上文「镜像与加速」固化；在别的机器上重现时，请使用**只针对该依赖**的改写，不要全局替换 `https://github.com/`（会影响其它仓库的 push）：

```bash
git config --global url."https://ghfast.top/https://github.com/amtoaer/rust-embed-for-web".insteadOf \
  "https://github.com/amtoaer/rust-embed-for-web"
```

### crates.io 下载过慢

已在仓库 `.cargo/config.toml` 中切换到 `rsproxy.cn` 镜像（见上文）。若该文件缺失，按上文片段重建即可。

### 无法写入 `~/.cargo`

在受限环境中（如沙箱），可把 cargo 主目录重定向到可写位置：

```bash
export CARGO_HOME=/tmp/bili-sync-cargo
```

### `stripping debug info with rust-objcopy failed`

部分工具链的 `rust-objcopy` 缺少 `libLLVM.dylib`，导致 `release` 配置中的 `strip = true` 未生效。该警告不影响运行，只是二进制体积偏大，可手动执行 `strip target/release/bili-sync-rs`。

### 版本号带 `-dirty` 后缀

说明工作区存在未提交改动。若只是换行符差异，可参考仓库根目录的 `.gitattributes`（已统一为 LF）。

## 交叉编译与容器

| 目标 | 三元组 | 构建机 | 链接器 / 工具链 | 产物 |
| --- | --- | --- | --- | --- |
| Linux 静态 | `x86_64-unknown-linux-musl` | Linux | `musl-gcc`（`musl-tools`） | `bili-sync-rs`（静态，可跑在任意 x86_64 发行版） |
| Windows（MinGW） | `x86_64-pc-windows-gnu` | Linux / WSL2 | `x86_64-w64-mingw32-gcc` | `bili-sync-rs.exe`，**推荐方案**，见下一节 |
| Windows（MSVC） | `x86_64-pc-windows-msvc` | Windows，或 Linux + `cargo-xwin` | `link.exe` / `lld` | 与官方 CI 产物 ABI 一致，见「可选：MSVC ABI」 |
| macOS | `aarch64-apple-darwin` / `x86_64-apple-darwin` | macOS | Apple clang | `bili-sync-rs` |

- `--locked` 使用 `Cargo.lock` 锁定的版本；**Cargo.lock 与目标平台无关**，同一份即可，不需要为 Windows 重新生成。
- `web/build` 是平台无关的前端产物，但**必须与后端来自同一 commit**（前后端有接口契约，见仓库根的 AGENTS.md）；
  只在 WSL 里编后端时，可以把开发机已构建的 `web/build` 同步过去，例如
  `rsync -a --delete web/build/ wsl:/home/<user>/bili-sync/web/build/`，但要确认前端源码版本一致。
- **Linux 静态包**：`rustup target add x86_64-unknown-linux-musl`，准备 musl 交叉工具链后 `cargo build --target x86_64-unknown-linux-musl --release`。
- **Docker**：`Dockerfile` 需要已构建好的 `bili-sync-rs-Linux-*.tar.gz`，不是从源码直接构建的捷径，请使用 `just build-docker`。

## Windows 交叉编译（WSL2 实测）

不需要 Windows SDK 或 MSVC 环境，在 Linux（WSL2 Ubuntu）上装好 MinGW 就能直接产出可双击运行的 exe。以下为一次真实构建的完整命令，可直接照抄。

### 构建机环境

| 项目 | 值 |
| --- | --- |
| 主机 | WSL2 内的 Ubuntu 26.04 LTS（x86_64），Windows 宿主机 |
| 硬件 | 2 核 / 3.8 GB 内存 / 1 TB 磁盘（内存偏小，见下文 swap） |
| 访问方式 | SSH 2222 端口，普通用户（有 sudo 权限）；凭据向维护者索取，**不要写入仓库** |
| 源码位置 | `/home/<user>/bili-sync`（含从开发机同步来的 `web/build` 与 `.cargo/config.toml`） |
| 工具链 | rustc / cargo 1.98.0、Bun 1.4.4、gcc 15.2、MinGW-w64 gcc 13 |
| 依赖规模 | Cargo.lock 507 个包、前端 288 个包 |
| 产物 | `bili-sync-rs.exe`，约 17 MB，PE32+ x86-64 |

### 安装依赖（全部走国内镜像）

```bash
# 1. apt 换阿里云（Ubuntu 26.04 使用 deb822 格式；先备份原文件）
sudo cp -a /etc/apt/sources.list.d/ubuntu.sources /etc/apt/sources.list.d/ubuntu.sources.bak
sudo sed -i 's|http://archive.ubuntu.com/ubuntu/|https://mirrors.aliyun.com/ubuntu/|g; s|http://security.ubuntu.com/ubuntu/|https://mirrors.aliyun.com/ubuntu/|g' \
  /etc/apt/sources.list.d/ubuntu.sources
sudo apt-get update

# 2. 基础工具链 + MinGW 交叉编译器（约 1 GB）
sudo apt-get install -y --no-install-recommends \
  build-essential pkg-config gcc-mingw-w64-x86-64 g++-mingw-w64-x86-64

# 3. Rust 1.98.0 + Windows 目标（rsproxy 镜像）
export RUSTUP_DIST_SERVER=https://rsproxy.cn RUSTUP_UPDATE_ROOT=https://rsproxy.cn/rustup
curl --proto '=https' --tlsv1.2 -sSf https://rsproxy.cn/rustup-init.sh | sh -s -- -y --profile minimal --default-toolchain 1.98.0
rustup target add x86_64-pc-windows-gnu x86_64-pc-windows-msvc --toolchain 1.98.0

# 4. Bun：用 npmmirror 的平台包，不需要 Node/npm
curl -fsSL https://registry.npmmirror.com/@oven/bun-linux-x64/-/bun-linux-x64-1.4.4.tgz -o /tmp/bun.tgz
mkdir -p ~/.bun/tmp && tar xzf /tmp/bun.tgz -C ~/.bun/tmp
install -m 755 "$(find ~/.bun/tmp -name bun -type f | head -1)" ~/.bun/bin/bun
export PATH="$HOME/.bun/bin:$PATH"

# 5. 其余镜像：crates 用仓库内 .cargo/config.toml（rsproxy），git 依赖走 ghfast，前端用 npm 镜像
git config --global url."https://ghfast.top/https://github.com/amtoaer/rust-embed-for-web".insteadOf \
  "https://github.com/amtoaer/rust-embed-for-web"
echo 'registry=https://registry.npmmirror.com' >> ~/.npmrc
```

> [!NOTE]
> WSL 默认只有约 4 GB 内存，安装 MinGW（近 1 GB 的包）或链接 507 个 crate 时容易 OOM（实测会把 sshd 一起杀掉，导致 SSH 断连）。建议先加 swap：
>
> ```bash
> sudo fallocate -l 4G /swapfile2 && sudo chmod 600 /swapfile2 && \
>   sudo mkswap /swapfile2 && sudo swapon /swapfile2
> ```
>
> WSL 重启后 swap 不会自动恢复，重新构建前需再执行一次。

### 链接器配置（二选一）

交叉编译必须告诉 cargo 用 MinGW 的 gcc 而不是系统 cc。

临时（只对当前 shell 生效）：

```bash
export CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER=x86_64-w64-mingw32-gcc
```

长期或 CI 推荐写进配置，避免每次 export。仓库根的 `.cargo/config.toml` 是**本地镜像配置**（已被 gitignore、不随仓库分发），
在文件末尾**追加**下面这段即可（不要覆盖已有的 `[source.crates-io]` 段）：

```toml
[target.x86_64-pc-windows-gnu]
linker = "x86_64-w64-mingw32-gcc"
```

也可以写到 `~/.cargo/config.toml`，对该用户的所有仓库生效。配置好后 `cargo build --target x86_64-pc-windows-gnu` 就不再需要环境变量。

### 构建

```bash
cd ~/bili-sync

# 1. 前端（必须先做，产物 web/build 会被嵌入二进制）
cd web && bun install --frozen-lockfile && bun run build && cd ..

# 2. 交叉编译 Windows exe
export PATH="$HOME/.cargo/bin:$HOME/.bun/bin:$PATH"
export CARGO_BUILD_JOBS=2                                    # 2 核，压低内存峰值
export CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER=x86_64-w64-mingw32-gcc
time cargo build --release --locked --target x86_64-pc-windows-gnu

# 3. 检查产物：应为 PE32+ executable for MS Windows
file target/x86_64-pc-windows-gnu/release/bili-sync-rs.exe
# 应只出现 Windows 系统 DLL（kernel32 / ntdll / msvcrt / bcrypt / crypt32 / ws2_32 等）
x86_64-w64-mingw32-objdump -p target/x86_64-pc-windows-gnu/release/bili-sync-rs.exe | grep -i "DLL Name"
```

首次交叉编译在 2 核机器上约 7 分钟（依赖已由 `cargo fetch` 预取时）；依赖下载另约 1 分钟（rsproxy 镜像）。

### 产物自包含检查（推荐）

MinGW 默认可能动态链接 `libgcc_s_seh-1.dll` / `libwinpthread-1.dll`，那样拷到没有 MinGW 的机器上会缺 DLL：

```bash
x86_64-w64-mingw32-objdump -p target/x86_64-pc-windows-gnu/release/bili-sync-rs.exe \
  | grep -i "DLL Name" | sort -u
```

正常只应出现 Windows 系统 DLL：`kernel32` / `user32` / `shell32` / `ntdll` / `msvcrt` / `bcrypt` / `bcryptprimitives` /
`crypt32` / `ws2_32` / `pdh` / `powrprof` / `psapi` / `oleaut32` / `combase` 等。
如果看到 `libgcc_s_seh-1.dll` 或 `libwinpthread-1.dll`，加静态 CRT 重建：

```bash
RUSTFLAGS="-C target-feature=+crt-static" \
cargo build --release --locked --target x86_64-pc-windows-gnu
```

> [!NOTE]
> 改 `RUSTFLAGS` 会让整个依赖树按新 flags 重新编译（缓存按 flags 区分）；如果长期需要，
> 可以把 `rustflags = ["-C", "target-feature=+crt-static"]` 直接写进上面的 `[target.x86_64-pc-windows-gnu]` 段。

### 验证

```bash
# 产物信息（WSL 内可直接运行 Windows exe，走 WSL interop）
./target/x86_64-pc-windows-gnu/release/bili-sync-rs.exe --version
# Bili-Sync v2.11.1-22-g0fdaa8c
# Architecture: windows-x86_64

# 用 Windows 侧临时配置目录启动，再做冒烟测试
./target/x86_64-pc-windows-gnu/release/bili-sync-rs.exe -c 'C:\Users\<user>\bili-sync-test'
curl -s --compressed -o /dev/null -w '%{http_code}\n' http://127.0.0.1:12345/    # 200
curl -s -o /dev/null -w '%{http_code}\n' http://127.0.0.1:12345/api/config       # 401
curl -s --compressed -H "Authorization: <日志中的 auth_token>" \
  -o /dev/null -w '%{http_code}\n' http://127.0.0.1:12345/api/config             # 200
```

> [!IMPORTANT]
> Windows 上运行需要 **ffmpeg 在 PATH 中**，否则启动即报 `初始化失败：ffmpeg 不存在或无法执行`。
> 常规做法是 `winget install Gyan.FFmpeg`；若 Windows 侧连不上 winget 源（`0x80072efd`），
> 可在 WSL 内下载 npmmirror 的 `@ffmpeg-installer/win32-x64` 包，把其中的 `ffmpeg.exe` 放进
> `%LOCALAPPDATA%\Microsoft\WindowsApps`（该目录已在用户 PATH 中，放进去即生效）。
> 也可以在启动时用 `--ffmpeg-path <路径>` 显式指定 ffmpeg。

### 在 WSL 里运行 Windows 产物（interop 与进程管理）

WSL2 默认开启 interop，可以像 Linux 程序一样直接执行 exe：

```bash
cat /proc/sys/fs/binfmt_misc/WSLInterop    # 文件存在即 interop 可用（WSL2 默认）
./target/x86_64-pc-windows-gnu/release/bili-sync-rs.exe --version
```

三个容易卡住的点：

1. **Windows 进程在 WSL 的 `ps` 里看不到**，查/杀要用 Windows 工具。SSH 登录的 shell 里 PATH
   只包含 `System32` 的一部分：`tasklist.exe` / `taskkill.exe` / `netstat.exe` / `cmd.exe` 可以直接调用，
   但 **`powershell.exe` 不在 PATH**，要用绝对路径。
2. **`netstat.exe -ano` 在非交互 SSH 会话里可能输出为空**（实测），查端口占用改用
   `Get-NetTCPConnection`：

```bash
PS=/mnt/c/Windows/System32/WindowsPowerShell/v1.0/powershell.exe
$PS -NoProfile -Command 'Get-NetTCPConnection -LocalPort 12345 -State Listen | Select LocalAddress,LocalPort,OwningProcess'
/mnt/c/Windows/System32/tasklist.exe | grep -i bili        # 按名字找进程
/mnt/c/Windows/System32/taskkill.exe /PID <pid>            # 正常结束（需要强制时加 /F）
```

3. **`-c` 必须是 Windows 能识别的路径**（`C:\Users\<user>\...`、`D:\bili-sync` 或
   `\\wsl.localhost\Ubuntu\home\...`）；传 `/home/...` 这种 Linux 路径会被 Windows 侧解析到奇怪位置。
   路径含空格时保留单引号：

```bash
./target/x86_64-pc-windows-gnu/release/bili-sync-rs.exe -c 'C:\Users\<user>\bili-sync-test'
```

### 打包、复制到 Windows 与分发

WSL 镜像里通常没有 `zip` / `7z`（实测 `zip: command not found`），三种做法：

```bash
# 1) tar（Windows 侧用 7-Zip / WinRAR / 系统自带 tar 都能解）
tar czf bili-sync-rs-Windows-x86_64.tar.gz -C target/x86_64-pc-windows-gnu/release bili-sync-rs.exe

# 2) 用 Python 打 zip，不需要额外装包
python3 -m zipfile -c bili-sync-rs-Windows-x86_64.zip target/x86_64-pc-windows-gnu/release/bili-sync-rs.exe

# 3) 安装 zip 工具
sudo apt-get install -y zip
```

复制到 Windows 侧（WSL 里直接写 `/mnt/c`）并生成校验值：

```bash
cp bili-sync-rs-Windows-x86_64.zip /mnt/c/Users/<user>/Desktop/
sha256sum bili-sync-rs-Windows-x86_64.zip     # 附在发行说明里，方便对方校验
```

> [!IMPORTANT]
> exe **没有代码签名**，分发时要向用户说明：
> - 首次双击可能出现「Windows 已保护你的电脑」（SmartScreen），点「更多信息」→「仍要运行」；
> - 从浏览器/聊天工具下载的 zip 带网络标记，解压后仍可能被拦，先执行
>   `powershell -Command "Unblock-File .\bili-sync-rs.exe"`，或右键文件属性里勾选“解除锁定”；
> - 正式对外分发建议购买代码签名证书（EV 证书可免 SmartScreen 警告），否则每个用户都要手动放行。

### 可选：MSVC ABI（cargo-xwin，未实测）

官方 release 用 MSVC 构建。Linux 上没有 `link.exe` 和 Windows SDK，直接
`--target x86_64-pc-windows-msvc` 会链接失败；可以用 [`cargo-xwin`](https://github.com/rust-cross/cargo-xwin)
下载 MSVC CRT/SDK 并用 lld 链接（**本仓库尚未实测**）：

```bash
sudo apt-get install -y clang lld
rustup target add x86_64-pc-windows-msvc
cargo install cargo-xwin --locked
cargo xwin build --release --locked --target x86_64-pc-windows-msvc
# 产物：target/x86_64-pc-windows-msvc/release/bili-sync-rs.exe
```

注意 `cargo-xwin` 首次会从 Microsoft 下载 SDK/CRT（数百 MB，国内网络可能很慢），
它与 gnu 产物是不同 ABI；除非需要与官方产物完全一致，否则优先用上面的 MinGW 方案。

### 踩坑记录

| 现象 | 原因 | 解决 |
| --- | --- | --- |
| `apt-get update` 超过 5 分钟无进展 | 默认 `archive.ubuntu.com` 在国内很慢 | 换阿里云源（实测 Release 文件 1.4 MB/s） |
| 安装依赖时 SSH 断开：`kex_exchange_identification: Connection reset` 或 banner 超时 | WSL 内存不足触发 OOM，sshd 被杀 | 先加 4 GB swap；安装/编译用 `nohup` 后台执行，SSH 断连不影响任务 |
| bun 官方安装脚本卡在下载 | 脚本从 GitHub Releases 拉二进制 | 改用 npmmirror 的 `@oven/bun-linux-x64` 平台包，解压出二进制即可 |
| `cargo build` 卡在 `Updating git repository` | `rust-embed-for-web` 是 GitHub git 依赖 | 只针对该仓库配置 ghfast 的 `insteadOf`（见上文），不要全局替换 GitHub |
| Windows 上 `winget` 报 `InternetOpenUrl() failed 0x80072efd` | Windows 侧网络连不上 winget 源 | 在 WSL 内下载，再复制到 Windows 目录并放进 PATH |
| 启动报 `ffmpeg 不存在或无法执行` | Windows 侧未安装 ffmpeg 或不在 PATH | 见上文；也可用 `--ffmpeg-path` 指定路径 |
| 若产物依赖 `libgcc_s_seh-1.dll` / `libwinpthread-1.dll`（本次未出现） | MinGW 运行时被动态链接 | 加 `-C target-feature=+crt-static` 重新构建，或随包分发这两个 DLL |
| `error: linker 'cc' not found` / `x86_64-w64-mingw32-gcc: not found` | 没设 `CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER`，或 MinGW 没装 / `~/.cargo/bin` 不在 PATH | 按「链接器配置」设置；`sudo apt-get install gcc-mingw-w64-x86-64` |
| `powershell.exe: command not found`（SSH 会话） | 非交互 PATH 不含 WindowsPowerShell 目录 | 用绝对路径 `/mnt/c/Windows/System32/WindowsPowerShell/v1.0/powershell.exe` |
| `netstat.exe -ano` 无任何输出 | 非交互 SSH 会话下不可靠（实测） | 用 `Get-NetTCPConnection` 查端口，用 `tasklist` 找进程（见上文） |
| `zip: command not found` | WSL 最小安装不带 zip | 用 `python3 -m zipfile -c` / `tar`，或 `sudo apt-get install zip` |
| 双击 exe 出现「Windows 已保护你的电脑」 | 产物未做代码签名 + 下载带来的网络标记 | 「更多信息」→「仍要运行」；`Unblock-File`；正式分发需代码签名证书 |
| 托盘壳 `--check` 在 cmd/PowerShell 里没有任何输出 | release 版是 Windows 子系统（GUI），没有控制台 | 重定向输出到文件，或构建带控制台的 debug 版 |
| 托盘壳编译报 `no method named with_icon_templated` / `unused import: Instant` | 用了 macOS / Unix 专属 API 或导入 | 平台相关代码用 `#[cfg(target_os = "...")]` / `#[cfg(unix)]` 分流（已修复） |

### 桌面托盘壳（desktop-tray）

托盘壳是独立 workspace（`desktop-tray/`，不在仓库根 workspace 里），需要和主程序**分别**交叉编译。
它运行时会查找**同目录**的 `bili-sync-rs.exe` 作为 sidecar。

```bash
cd ~/bili-sync/desktop-tray
export PATH="$HOME/.cargo/bin:$PATH"
export CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER=x86_64-w64-mingw32-gcc
export CARGO_BUILD_JOBS=2
cargo build --release --locked --target x86_64-pc-windows-gnu
# 产物：target/x86_64-pc-windows-gnu/release/bili-sync-tray.exe（PE32+ GUI，约 720 KB）
```

验证（均为实际执行过的命令）：

```bash
cd ~/bili-sync/desktop-tray
file target/x86_64-pc-windows-gnu/release/bili-sync-tray.exe
# PE32+ executable for MS Windows 5.02 (GUI), x86-64

x86_64-w64-mingw32-objdump -p target/x86_64-pc-windows-gnu/release/bili-sync-tray.exe \
  | grep -i "DLL Name" | sort -u      # 只应有 Windows 系统 DLL

# release 是 GUI 子系统、没有控制台：功能自检请构建 debug 版（带控制台）
cargo build --target x86_64-pc-windows-gnu
./target/x86_64-pc-windows-gnu/debug/bili-sync-tray.exe --version     # bili-sync-tray 0.1.0
./target/x86_64-pc-windows-gnu/debug/bili-sync-tray.exe --check -p 12345
```

打包时两个 exe 必须在同一目录，分发目录建议长这样：

```
bili-sync-tray-windows-x86_64/
├── bili-sync-tray.exe    # 托盘壳，PE32+ GUI
├── bili-sync-rs.exe      # 与前端同 commit 的主程序
└── README.txt            # ffmpeg / SmartScreen / 参数说明
```

托盘在 Windows 上的行为：

- 双击 `bili-sync-tray.exe` → 通知区出现图标，自动拉起同目录的 `bili-sync-rs.exe`，管理页就绪后打开 `http://127.0.0.1:12345`；
- 点图标 → 菜单里有「打开管理页 / 打开数据目录 / 打开日志 / 启动 / 停止 / 重启 / 退出托盘」；
- 退出请用菜单，**不要直接叉掉进程**：菜单退出会先停止它拉起的服务（Windows 下是 TerminateProcess，SQLite WAL 可恢复但会丢当前下载进度）；
- 如果托盘被任务管理器强杀，下次启动会按端口探测「接管」还在跑的服务，不会重复拉起。

### 与官方 CI 的差异

- 官方 release 的 Windows 产物使用 `x86_64-pc-windows-msvc`（GitHub 的 `windows-latest` runner）；本节方式使用 `x86_64-pc-windows-gnu`（MinGW）。两者功能一致，gnu 产物本次实测只依赖 Windows 系统 DLL，可直接拷贝运行。
- 若需要与官方完全一致的 MSVC ABI，可在 Linux 上使用 `cargo-xwin`（需额外安装 clang/lld 并下载 Windows SDK/CRT，本仓库尚未实测）。

## 参考：一次真实构建记录（macOS）

以下记录来自 macOS（Apple Silicon）上的实际构建，供排查耗时与依赖问题时参考：

| 项目 | 数值 |
| --- | --- |
| 版本 | v2.11.1 |
| 工具链 | rustc / cargo 1.98.0、Bun 1.4.4、Apple clang 21 |
| 依赖规模 | Cargo.lock 507 个包、前端 336 个包 |
| 前端构建 | 约 50 秒（含依赖安装） |
| 依赖下载 | 约 15 分钟（受网络影响最大） |
| 后端编译 | 约 1 分 17 秒 |
| 产物 | `target/release/bili-sync-rs`，约 19 MB，Mach-O arm64 |
| 磁盘占用 | `target/` 约 950 MB，依赖缓存约 880 MB |

## 参考：WSL2 交叉编译 Windows 实测记录（2026-11-18）

| 项目 | 数值 |
| --- | --- |
| 构建机 | WSL2 内 Ubuntu 26.04 LTS，2 核 / 3.8 GB 内存 + 5 GB swap |
| 工具链 | rustc / cargo 1.98.0、MinGW-w64 gcc 13、Bun 1.4.4 |
| 产物 | `bili-sync-rs.exe` 17 MB、`bili-sync-tray.exe` 722 KB，均只依赖 Windows 系统 DLL |
| 增量构建 | 依赖已编译后：主程序约 47 秒、托盘壳约 8 秒（首次依赖编译另需数分钟） |
| target 占用 | 主程序 `target/` 1.2 GB（其中 `x86_64-pc-windows-gnu` 860 MB）；托盘壳 `target/` 354 MB |
| 验证 | `file` / `objdump` 检查 PE 与 DLL；WSL interop 下运行 `--version` 与 `--check` 通过 |
