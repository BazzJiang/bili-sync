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
- **Docker**：`Dockerfile` 需要已构建好的 `bili-sync-rs-Linux-*.tar.gz`，不是从源码直接构建的捷径，请使用 `just build-docker`。

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
