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

- **Linux 静态包**：`rustup target add x86_64-unknown-linux-musl`，并准备 musl 交叉工具链，再执行 `cargo build --target x86_64-unknown-linux-musl --release`。
- **Windows exe**：在 Linux（WSL2）上用 MinGW 交叉编译，步骤见下一节。
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
> `%LOCALAPPDATA%\Microsoft\WindowsApps\`（该目录已在用户 PATH 中，放进去即生效）。
> 也可以在启动时用 `--ffmpeg-path <路径>` 显式指定 ffmpeg。

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

### 与官方 CI 的差异

- 官方 release 的 Windows 产物使用 `x86_64-pc-windows-msvc`（GitHub 的 `windows-latest` runner）；本节方式使用 `x86_64-pc-windows-gnu`（MinGW）。两者功能一致，gnu 产物本次实测只依赖 Windows 系统 DLL，可直接拷贝运行。
- 若需要与官方完全一致的 MSVC ABI，可在 Linux 上使用 `cargo-xwin`（需额外安装 clang/lld 并下载 Windows SDK/CRT，本仓库尚未实测）。

## 参考：一次真实构建记录

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
