# AGENTS.md

本文件面向在本仓库工作的 AI agent，说明构建方式、代码约定与提交规范。人类贡献者同样适用。

## 项目速览

bili-sync 是面向 NAS 的哔哩哔哩同步工具：Rust 后端（axum + sea-orm + tokio）把视频、弹幕、NFO 写入本地媒体库，SvelteKit 前端在构建时嵌入二进制。

| 目录 | 内容 |
| --- | --- |
| `crates/bili_sync/` | 主程序：配置、B 站 API、下载器、任务调度、HTTP API |
| `crates/bili_sync_entity/` | SeaORM 实体与自定义类型 |
| `crates/bili_sync_migration/` | 数据库迁移 |
| `web/` | SvelteKit 前端，产物 `web/build` 会被嵌入二进制 |
| `docs/` | VitePress 中文文档 |

## 构建（顺序不能反）

```bash
cd web && bun install --frozen-lockfile && bun run build && cd ..
cargo build --release          # 产物：target/release/bili-sync-rs
```

- **必须先构建前端**：`rust-embed-for-web` 在编译期读取 `web/build`，跳过会导致管理页 404。
- 沙箱内 `~/.cargo` 不可写时：`export CARGO_HOME=/tmp/bili-sync-cargo`。
- 仓库内的 `.cargo/config.toml` 是本地镜像配置，**已被 gitignore，不要删除、也不要提交**。
- 增量构建直接 `cargo build --release`，**不要执行 `cargo clean`**。
- 只改前端时：`bun run build` 后再 `cargo build`（`build.rs` 监听 `web/build`）。

完整说明见 [`docs/build.md`](docs/build.md)。

## 验证

```bash
cargo check                      # 快速类型检查
cargo clippy -- -D warnings      # CI 会执行
cargo test                       # 39 个测试，其中 7 个 #[ignore] 需要真实网络
cargo +nightly fmt --check       # 需要 nightly，rustfmt.toml 使用了 unstable 配置
cd web && bun run lint && bun run check
```

冒烟测试（用临时配置目录，避免污染真实配置）：

```bash
./target/release/bili-sync-rs -c /tmp/bili-sync-test
curl -s --compressed http://127.0.0.1:12345/    # 不带 Accept-Encoding 会返回 406
curl -s http://127.0.0.1:12345/api/config        # 期望 401
```

## 代码约定

- 注释与日志使用中文，解释「为什么」而不是复述代码。
- 错误处理：`anyhow::Result` + `context`；B 站接口错误用 `BiliError`，`-352`、HTTP 412/403 视为风控，需要中止整轮任务。
- 下载状态是 `u32` 位域（`crates/bili_sync/src/utils/status.rs`），每个子任务占 3 bit。新增子任务时同步更新 `VideoStatus` / `PageStatus` 及重置逻辑。
- 数据库：新增迁移文件并在 `crates/bili_sync_migration/src/lib.rs` 注册，**不要修改已发布的迁移**。
- 配置存于数据库 `config` 表，经 `VersionedConfig` 热更新；修改结构时保证反序列化兼容（`#[serde(default)]`）。
- 前端：Svelte 5（runes）+ Tailwind 4 + bits-ui/layerchart，组件在 `web/src/lib/components`。
- API 路由在 `crates/bili_sync/src/api/routes/`，响应统一使用 `ApiResponse`。
- 新增文档页要在 `docs/.vitepress/config.mts` 的侧边栏登记。

## 提交规范

- 中文 conventional commits：`feat: 支持 xxx`、`fix: 修复 xxx`、`docs:`、`chore:`。
- 模板见 `.gitmessage`，`git commit` 不带 `-m` 时自动套用。
- 换行符统一 LF（`.gitattributes`），不要提交 CRLF 造成的整文件 diff。

## 不要提交

`target/`、`web/build/`、`*.sqlite*`、`.cargo/config.toml`、`.DS_Store`。

## 已知问题

- release 构建可能打印 `stripping debug info with rust-objcopy failed`，不影响产物运行。
- 版本号带 `-dirty` 表示工作区有未提交改动。
- release 模式下前端响应要求客户端支持 gzip/br，否则返回 406。

## 工作流建议

1. 动手前先读 `docs/build.md`，确认构建与镜像配置。
2. 小步修改、小步提交，提交前至少跑 `cargo check` 与相关测试。
3. 遇到网络或依赖问题先查镜像配置，不要直接重试整轮下载。
