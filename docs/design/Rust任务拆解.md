# Rust 侧任务拆解（v1）

依据 [架构设计](架构设计.md)。按依赖顺序排列，T1、T2、T3、T4、T7 互不依赖可并行；T6 是收口点。

| # | 任务 | 依赖 | 产出文件 | 规模 |
|---|---|---|---|---|
| T0 | 工程脚手架 | — | `src-tauri/`、`src/` 骨架 | S |
| T1 | 配置文件 + 监听 | T0 | `config.rs` | S |
| T2 | SQLite 存储 + 版本化迁移 | T0 | `store.rs` | S |
| T3 | OpenAI 兼容流式客户端 | T0 | `llm/openai.rs` | M |
| T4 | 工具集 + 路径沙箱 | T0 | `agent/tools/{mod,fs,shell,web}.rs` | M |
| T5 | 审批策略 + 持久化 allowlist | T1、T4 | `agent/policy.rs` | S |
| T6 | Agent 循环 + 命令 + 事件 + 契约对照测试 | T1–T5 | `agent/loop.rs`、`commands.rs`、`events.rs` | L |
| T7 | 托盘 + 全局快捷键 | T0 | `tray.rs` | S |
| T8 | 权限与窗口配置 | T6 | `tauri.conf.json`、`capabilities/` | S |

## T0 工程脚手架

- `pnpm create tauri-app`，模板 React + TypeScript + Vite，包管理 pnpm
- 前端只保留一个能 `invoke` 的空页面
- Cargo 依赖一次加齐：tokio、reqwest（stream + rustls）、serde、serde_json、toml、rusqlite（bundled）、futures-util、thiserror、dirs、uuid、notify、regex
- 检查：`pnpm tauri dev` 弹出窗口

## T1 配置文件 + 监听

- 目录 `~/.venus-whisper/`，文件 `config.toml`、`persona.md`
- 结构体

  ```rust
  struct Config { llm: LlmConfig, agent: AgentConfig }
  struct LlmConfig { base_url, model, api_key }
  struct AgentConfig { workspace, approval_mode, max_turns, allow: Vec<AllowEntry> }
  struct AllowEntry { workspace, commands: Vec<String> }
  ```

- 缺文件时写带注释的默认模板
- 每次循环启动时重读；另用 `notify` 监听目录，debounce 300 ms 后 emit `config:changed`（无 payload），前端设置页收到后重新 `get_settings`
- 命令：`get_settings`、`set_settings`、`get_persona`、`set_persona`、`open_config_dir`
- 检查：单测，默认模板 → 解析 → 序列化 → 再解析一致；`api_key` 在 `get_settings` 返回里原样给前端（用户自管，不脱敏）

## T2 SQLite 存储 + 版本化迁移

- rusqlite 直连 `~/.venus-whisper/data.db`
- 迁移用 `PRAGMA user_version` + 有序 SQL 数组，启动时从当前版本逐条执行到最新，零依赖

  ```rust
  const MIGRATIONS: &[&str] = &[
      // v1
      "CREATE TABLE sessions(...); CREATE TABLE messages(...); CREATE TABLE settings(...);",
      // v2 追加在此
  ];
  ```

- 三张表照设计文档；`messages.content_json` 存 OpenAI 消息格式
- 命令：`list_sessions`、`create_session`、`delete_session`、`get_messages`
- 检查：单测用内存库，(1) 从 `user_version = 0` 跑到最新后再跑一次不报错、版本号不变；(2) 写一条含 `tool_calls` 的消息再读回 JSON 相等

## T3 OpenAI 兼容流式客户端

- `trait LlmClient { async fn chat(&self, req: ChatRequest) -> Result<BoxStream<LlmEvent>> }`，事件 `Delta(String)`、`ToolCall { id, name, arguments }`、`Done`
- 真实实现解析 SSE；按 `tool_calls[i].index` 拼装分片到达的 `function.arguments`，收到 `finish_reason` 后统一吐出 `ToolCall`
- trait 只为 T6 能注入假客户端
- 检查：单测喂录好的 SSE 文本（arguments 拆三片），断言拼出的 `ToolCall` 完整；另一段纯文本 SSE 断言 Delta 顺序

## T4 工具集 + 路径沙箱

- `trait Tool { fn name(); fn description(); fn schema() -> serde_json::Value; async fn run(&self, args, ctx: &ToolCtx) -> Result<String> }`，`ToolCtx { workspace: PathBuf }`
- 七个实现：`read_file`、`list_dir`、`grep`、`write_file`、`edit_file`、`run_shell`、`web_fetch`
- `resolve_in_workspace(workspace, path)`：join 后 canonicalize，检查以 workspace 的 canonical 路径为前缀；新建文件时对父目录 canonicalize
- `run_shell`：tokio 子进程，`cwd = workspace`，默认 60 s 超时，stdout + stderr 合并截断 16 KB
- `edit_file`：`old` 必须恰好出现一次
- `grep`：用 `regex` crate 逐文件扫，跳过二进制和 `.git/`，不引 ripgrep
- 检查：单测，`../` 和符号链接逃逸被拒；`edit_file` 出现两次报错；`run_shell` 跑 `sleep 5` 配 1 s 超时返回超时错误且不残留子进程

## T5 审批策略 + 持久化 allowlist

- `fn check(mode, call: &ToolCall, allow: &[String]) -> Verdict::{Allow, Ask}`
  - 只读工具、`web_fetch` 恒 Allow
  - `write_file` / `edit_file`：`ask` → Ask，其余 Allow
  - `run_shell`：`full-auto` → Allow；否则命令以 allowlist 任一前缀开头 → Allow，否则 Ask
  - 路径越界由 T4 沙箱直接报错，策略层不处理
- "总是允许"：`approve_tool(decision = always)` 时把命令前缀写入 `config.toml` 的 `agent.allow` 中当前 workspace 那条，落盘后下一轮循环重读即生效
- 前缀取法：命令按空白切分，取前两个 token（如 `git status`、`pnpm test`）；单 token 命令取一个
- 检查：单测跑 模式 × 工具 真值表；`always` 写入后重读 config 能命中

## T6 Agent 循环 + 命令 + 事件 + 契约对照测试

- `send_message(session_id, text)` spawn tokio 任务立即返回
- 循环体

  ```
  loop (最多 max_turns):
    config = 重读
    messages = system(persona) + 历史 + 本轮
    stream = llm.chat(messages, tools)      正文实时 emit agent:delta
    if 无 tool_calls: 剥离 <meta> → agent:done + agent:reaction → 落库 → break
    for call in tool_calls:
      match policy.check(...):
        Allow => 执行
        Ask   => emit agent:tool_request → await oneshot → allow/always 执行，deny 返回 "denied by user"
      messages.push(tool 消息); emit agent:tool_result
  ```

- 审批等待：`Mutex<HashMap<call_id, oneshot::Sender<Decision>>>`，`approve_tool` 取出发送；`always` 额外调 T5 写 allowlist
- 取消：每会话 `Arc<AtomicBool>`，每轮开始和每个工具前检查；`cancel_run` 置位并 emit `agent:error { message: "cancelled" }`
- `<meta>` 用正则剥离；`suggested_reaction` 不在 `ACTIONS` 常量内回落 `idle_breath`
- `events.rs`：全部事件 payload 用 serde 派生，带 `session_id`
- **契约对照测试**：单测把每个事件 payload 的示例值 `serde_json::to_value`，取出字段名集合，与解析 `src/events.ts` 中对应 interface 的字段名集合比对，不一致即失败。TS 侧解析用最简正则（`interface X { a: ...; b: ...; }`），不引 TS 解析器
- 检查：假 `LlmClient` 第一轮返回 `read_file` 调用、第二轮返回纯文本，断言事件序列 delta → tool_result → delta → done，落库两条 assistant 一条 tool；契约对照测试通过

## T7 托盘 + 全局快捷键

- `tauri-plugin-global-shortcut`，默认 `Cmd/Ctrl+Shift+Space` 切换主窗
- 托盘菜单：显示、设置、退出
- 检查：手动

## T8 权限与窗口配置

- `capabilities/default.json` 只开 `core:event`、`core:window`、global-shortcut 和自定义命令
- `tauri.conf.json` 预写 `pet` 窗口（`transparent`、`decorations: false`、`alwaysOnTop`、`skipTaskbar`、`visible: false`），macOS 开 `macOSPrivateApi`
- 检查：`pnpm tauri build` 通过

## 有意不做的

- 不引 tauri-specta，用 T6 的契约对照测试防漂移
- 不引 refinery / sqlx-migrate，迁移用 `user_version`
- 不引 ripgrep，`grep` 工具用 `regex` 逐文件扫
- MCP、记忆、语音不碰
