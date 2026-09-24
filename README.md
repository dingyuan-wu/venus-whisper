# Venus Whisper

跨平台的本地 LLM Agent 助手：能对话、能在指定目录内读写文件和执行命令，写操作按审批模式确认。Tauri 2 + Rust 内核 + React 前端，所有数据留在本机。

虚拟形象（Live2D）与情绪反应引擎是后续版本的能力，v1 只预留了接口，见 [docs/design/架构设计.md](docs/design/架构设计.md)。

![系统架构](docs/design/asset/architecture.png)

## 功能

- 多会话聊天，流式输出，Markdown 渲染
- 七个内置工具：`read_file` `list_dir` `grep` `web_fetch` `write_file` `edit_file` `run_shell`
- 三档审批模式：`ask`（写文件和 shell 都问）/ `auto-edit`（写文件自动，shell 问）/ `full-auto`
- 所有工具限制在 workspace 目录内，路径越界直接拒绝
- 审批卡上的"总是允许"会把命令前缀持久化到配置，按 workspace 分组
- OpenAI 兼容接口，LiteLLM / Ollama / OpenAI 都能接
- 右侧栏显示当前上下文占用百分比；调试模式下逐次显示 token 用量并拆分输入构成（人格 / 技能 / 输出约定 / 工具定义 / 历史 / 本次消息 / 工具往返；总数优先取服务端 usage，构成为本地估算）
- 系统托盘 + 全局快捷键呼出（macOS `⌘⇧Space`，其他平台 `Ctrl+Shift+Space`）

## 快速开始

环境要求：Rust 1.80+、Node 20+、pnpm，以及 [Tauri 2 的系统依赖](https://tauri.app/start/prerequisites/)。

```bash
pnpm install
pnpm tauri dev
```

首次启动会在 `~/.venus-whisper/` 生成默认配置，填好端点和 Key 即可对话：

```toml
# ~/.venus-whisper/config.toml
[llm]
active = "litellm"                       # 当前使用的模型 id，也可在右侧栏切换

[[llm.models]]                           # 可以配多个，OpenAI 兼容端点即可
id       = "litellm"
name     = "LiteLLM · gpt-4o"
base_url = "http://localhost:4000/v1"
model    = "gpt-4o"
api_key  = "sk-..."

[[llm.models]]
id       = "local"
name     = "本机 Gemma"
base_url = "http://localhost:8000/v1"
model    = "gemma-4-31b-it-8bit"
api_key  = ""
context_window = 32000                   # 上下文窗口，右侧栏据此显示占用百分比

[user]
name = "你"                              # 聊天里你这一侧的名字，头像放同目录 user.png

[ui]
theme = "mint"
debug = false                            # 调试模式：对话中显示每次调用的输入 / 输出 / 系统提示词 token

[agent]
workspace     = "~/projects"            # 工具只能在这个目录内活动
approval_mode = "ask"                    # ask | auto-edit | full-auto
max_turns     = 30
persona       = "builtin/venus"          # 人格
skills        = ["builtin/role-replication-skill-v2"]  # 启用的 skill，内容追加进 system prompt
```

旧版扁平的 `[llm] base_url / model / api_key` 写法仍能读取，会自动包成一个 id 为 `default` 的模型。

人格分两类。**内置**人格来自仓库的 `src-tauri/personas/`，编译进应用、只读，id 形如 `builtin/venus`；**本地**人格是 `~/.venus-whisper/personas/` 下的文件，可编辑，id 就是文件名或目录名。设置页里选中内置项可以"同步到本地"，得到一份可改的副本。本地副本与内置逐行对比：内容一致时列表只显示本地那份，改过的标"已修改"并提供差异视图；角色目录的 `character_bible.md` 也会渲染出来。

`[agent] persona` 选择人格，默认 `builtin/venus`；本地人格写 `coder`（对应 `coder.md`）或角色目录名。角色目录是 `skills/role-replication-skill-v2.md` 这份 skill 的产物，结构如下，只需要 `prompt.md`，其余文件可选：

```
~/.venus-whisper/personas/kaguya/
├── role.json            # 可选：name / description 显示在设置页
├── prompt.md            # 必需：作为 system prompt
├── avatar.png           # 可选：头像（png/jpg/webp/gif），聊天里替换当前人格的头像与名字
├── character_bible.md   # 可选
└── research/ …          # 可选
```

角色目录在设置页只读，要改请直接编辑目录内文件。

`[ui] theme` 指向 `themes/` 目录下的文件名。主题文件长这样，键名对应界面里的 CSS 变量 `--vw-<key>` 和 `--code-<key>`，缺的键回落默认值：

```toml
# ~/.venus-whisper/themes/mine.toml
name = "我的紫罗兰"
description = "随便写"

[colors]
accent = "#8b5cf6"
accent-dark = "#6d3fd9"
side = "#f1ecff"
ink = "#3b2a6b"

[code]
keyword = "#8b5cf6"
```

## 项目结构

```
src/                 前端（React 18 + TypeScript + animal-island-ui），纯 UI，无业务逻辑
  api.ts             Rust 命令的类型化封装
  store.ts           zustand 状态 + agent:* 事件订阅
  events.ts          与 src-tauri/src/events.rs 手工对照的事件类型
  components/        Sidebar / ChatView / Blocks / SettingsView
src-tauri/           Rust 内核
  src/agent/         循环 runner.rs、审批 policy.rs、工具 tools/
  src/llm/           OpenAI 兼容流式客户端
  src/config.rs      ~/.venus-whisper 配置读写与监听
  src/store.rs       SQLite 会话与消息
  src/tray.rs        托盘与全局快捷键
docs/design/         架构设计、Rust 任务拆解、archify 图源
docs/prd/            产品需求（含 v2 虚拟形象反应系统）
demo/                聊天页的独立设计稿，不参与构建
```

前后端只通过 Tauri 命令和事件通信：前端 `invoke` 调 `src-tauri/src/commands.rs` 里的函数，Rust 用 `agent:delta` / `agent:tool_request` / `agent:tool_result` / `agent:done` / `agent:error` 等事件推回进展。

## 开发命令

```bash
pnpm tauri dev        # 开发运行
pnpm tauri build      # 打包
pnpm test:rust        # cargo test
pnpm lint:rust        # cargo clippy -D warnings
pnpm fmt              # cargo fmt
pnpm exec tsc --noEmit
```

Rust 侧有 25 个单测，其中一条契约测试会解析 `src/events.ts`，保证事件字段与 Rust 不漂移。

## 数据位置

| 路径 | 内容 |
|---|---|
| `~/.venus-whisper/config.toml` | 端点、模型、Key、workspace、审批模式、allowlist |
| `~/.venus-whisper/personas/` | 本地人格：`<name>.md` 简单人格（同名 `<name>.png` 为头像），或角色目录 `<slug>/`（读 `prompt.md`、`role.json`、`avatar.png`）。内置人格编译在应用里，不落盘，设置页可"同步到本地"得到可编辑副本 |
| `~/.venus-whisper/skills/` | 本地 skill（Markdown）。内置 skill 编译在应用里，与人格同样可"同步到本地"、对比差异；启用的 skill 写在 `[agent] skills` |
| `~/.venus-whisper/themes/*.toml` | 外观主题，内置 mint / peach / sky，复制一份改颜色即新主题，保存即生效 |
| `~/.venus-whisper/data.db` | SQLite：会话与消息，消息按 OpenAI 格式存 JSON |
| `~/.venus-whisper/user.png` | 你的头像（也可 jpg / webp / gif）；显示名在 `config.toml` 的 `[user] name` |
| `~/.venus-whisper/window.json` | 主窗口位置、尺寸、最大化状态，退出时自动写入 |

API Key 以明文放在配置文件里，由用户自行管理。

## 许可证

代码为 [MIT](LICENSE)。UI 组件库 [animal-island-ui](https://github.com/guokaigdg/animal-island-ui) 为 CC BY-NC 4.0，仅限非商用；商用需替换组件层。
