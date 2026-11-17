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
- 前端：Svelte 5 + Tailwind 4 + bits-ui/layerchart，组件在 `web/src/lib/components`。注意现有页面大多仍是 **legacy 语法**（`export let` / `$:` / `$store`），新代码保持一致，不要混用 runes（`$state` / `$derived`）。
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

## 前端 UX 改版记录（分支 `feat/frontend-ui`，尚未提交）

本轮对 `web/` 管理页做了一次体验改造，功能说明已同步到 [`docs/frontend.md`](docs/frontend.md)，这里记录实现层面的改动与约束，供后续 agent 参考。

### 改动清单

| 区域 | 文件 | 说明 |
| --- | --- | --- |
| 新增页面 | `web/src/routes/activity/+page.svelte` | 活动：任务状态 + 等待中 + 最近入库，15s 自动刷新 |
| 新增页面 | `web/src/routes/wanted/+page.svelte` | 失败视频汇总，支持逐条/批量重置 |
| 新增组件 | `web/src/lib/components/video-list-row.svelte` | 视频列表行（进度、来源、状态、行内操作、可选勾选框） |
| 新增组件 | `web/src/lib/components/command-palette.svelte` | ⌘K 命令面板：导航 + 快捷动作 + 视频搜索 |
| 布局 | `web/src/routes/+layout.svelte` | 挂载命令面板与 header 触发按钮 |
| 侧边栏 | `web/src/lib/components/app-sidebar.svelte` | 新增「活动」「失败视频」入口，补上当前路由高亮 |
| 仪表盘 | `web/src/routes/+page.svelte` | 顶部 4 个关键指标卡（总数/失败/等待/近七日新增） |
| 视频列表 | `web/src/routes/videos/+page.svelte` | 筛选面板 + 条件 chips + 自动刷新 + 批量勾选 + 一次性加载全部 |
| 日志 | `web/src/routes/logs/+page.svelte` | 级别过滤、关键字搜索、暂停（缓冲）、清空、复制 |
| 设置 | `web/src/routes/settings/+page.svelte` | 9 个分区 + 左侧目录高亮 + 吸底保存条 + 未保存提示 |
| 视频源 | `web/src/routes/video-sources/+page.svelte` | 表格改为与视频页一致的 grid 列表（仅展示层） |
| 过滤器 store | `web/src/lib/stores/filter.ts` | 移除 `currentPage`（视频页不再分页） |
| 状态编辑器 | `web/src/lib/components/filtered-status-editor.svelte` | 新增 `titleOverride` / `descriptionOverride`，用于批量勾选场景 |
| 后端 | `crates/bili_sync/src/api/request.rs`、`routes/videos/mod.rs` | 批量接口新增 `ids: Option<Vec<i32>>` |

### 接口契约（前后端需配套发布）

- 批量重置/批量编辑接口（`POST /api/videos/reset-status`、`POST /api/videos/update-status`）新增可选 `ids: number[]`：传了按 id 精确匹配，不传保持原有「按筛选条件 / 全部」行为。
  **旧后端不认识 `ids` 会退化成对全部/筛选视频操作，因此前后端必须同版本发布。**
- 视频列表「一次性加载全部」的做法：请求固定 `page=0&page_size=1000000`。后端只有在 `page` 与 `page_size` **同时存在**时才走分页，且 `page_size` 没有上限校验，因此这等价于不分页，不需要改后端。

### Svelte 5 legacy 语法的三个坑（本轮实际踩过）

1. **`$:` 语句/模板通过函数间接读取响应式变量不会触发更新**：依赖分析是按词法做的。必须把变量显式传进函数，例如 `$: navItemClass(id, activeSection)`、`$: filterChips = getFilterChips($appStateStore)`；模板里尽量直接写 `selectedIds.includes(...)` 而不是调用 `isSelected(...)`。
2. **`SvelteSet` / `SvelteMap` 在 legacy 组件里不能可靠触发重渲染**（勾选状态这类场景），用普通数组 + 不可变更新（`selectedIds = [...selectedIds, id]`）最稳；`SvelteMap` 仅用于已有代码中的只读映射。
3. **bits-ui 的受控组件**（`Switch` / `Checkbox`）要用 `checked` + `onCheckedChange` 同步父状态，只 `bind:checked` 不会让父组件感知变化。

### 其它约定

- 后台自动刷新必须保留旧列表：只在 `loading && !videosData` 时显示「加载中」，否则每次轮询都会闪屏。
- 新增页面记得设置 `<title>`、`setBreadcrumb`、以及 `app-sidebar.svelte` 的入口与高亮。
- 列表页表头与行共用同一套 `md:grid-cols-[...]` 模板才能保证列对齐，改列宽时两处一起改。
- 本轮浏览器验证使用了一个**未入库**的临时 mock 后端（`/tmp` 下的 aiohttp 脚本，提供 `/api/videos`、`/api/dashboard`、`/api/ws` 推送），需要复现时按 `ApiResponse` 结构自行搭建；也可以直接用 release 二进制 + 临时配置目录（`-c /tmp/xxx`）验证。登录 token 在 `<配置目录>/data.sqlite` 的 `config` 表内，可用
  `sqlite3 <配置目录>/data.sqlite "select json_extract(data,'\$.auth_token') from config where id=1;"` 查询。
- 验证命令：`cd web && bun run check && bun run lint && bun run build`；后端改动另跑 `cargo check`、`cargo clippy -- -D warnings`、`cargo test`（本机沙箱下 `~/.cargo` 不可写，需要 `CARGO_HOME=/tmp/bili-sync-cargo`）。

### 尚未完成（可作为后续方向）

- 移动端底部导航、筛选抽屉化
- 视频详情页的信息层级重做
- 失败视频页也支持批量勾选
