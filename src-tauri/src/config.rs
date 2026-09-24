//! 用户配置：`~/.venus-whisper/config.toml` + `persona.md`。
//! 用户可在设置页改，也可直接手改文件；每次循环启动前重读，另有目录监听通知前端刷新。

use crate::error::Result;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::Duration;

pub const CONFIG_FILE: &str = "config.toml";
/// 旧版默认人格文件，仅用于首次启动迁移到 personas/venus.md
pub const PERSONA_FILE: &str = "persona.md";

pub const DEFAULT_CONFIG: &str = r#"# Venus Whisper 配置文件。可直接编辑，保存后下一条消息即生效。

[llm]
# 当前使用的模型，对应下面某个 [[llm.models]] 的 id
active = "default"

# 可以配置多个模型，使用时在右侧栏或设置页选择
[[llm.models]]
id = "default"
name = "默认"
# OpenAI 兼容端点：LiteLLM / Ollama / OpenAI 皆可
base_url = "http://localhost:4000/v1"
model = "gpt-4o"
api_key = ""
# 上下文窗口（token），用于右侧栏显示占用百分比；0 表示未知
context_window = 128000

[agent]
# 工具只能在这个目录内读写和执行命令
workspace = "~"
# ask | auto-edit | full-auto
approval_mode = "ask"
max_turns = 30
# 人格：内置写 builtin/<名字>，本地写 personas/ 下的文件名或角色目录名
persona = "builtin/venus"
# 启用的 skill，内容会追加进 system prompt；内置写 builtin/<名字>，本地写 skills/ 下的文件名
skills = []

[user]
# 聊天里显示的名字；头像放同目录的 user.png（或 jpg / webp / gif）
name = "你"

[ui]
# 调试模式：在对话中显示每次调用的输入 / 输出 / 系统提示词 token
debug = false
# themes/ 目录下的文件名，内置 mint | peach | sky，可自行添加
theme = "mint"

# 审批卡点"总是允许"后会追加到这里，按 workspace 分组，可手删
# [[agent.allow]]
# workspace = "/path/to/project"
# commands = ["git status", "pnpm test"]
"#;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Config {
    pub llm: LlmConfig,
    pub agent: AgentConfig,
    #[serde(default)]
    pub ui: UiConfig,
    #[serde(default)]
    pub user: UserConfig,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UserConfig {
    #[serde(default = "default_user_name")]
    pub name: String,
}

impl Default for UserConfig {
    fn default() -> Self {
        Self {
            name: default_user_name(),
        }
    }
}

fn default_user_name() -> String {
    "你".into()
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UiConfig {
    #[serde(default = "default_theme")]
    pub theme: String,
    /// 调试模式：对话中显示每次调用的 token 用量
    #[serde(default)]
    pub debug: bool,
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            theme: default_theme(),
            debug: false,
        }
    }
}

fn default_theme() -> String {
    "mint".into()
}

/// 多模型配置。旧版扁平写法（llm.base_url / model / api_key）读入时自动包成一个 id 为 default 的条目。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(from = "LlmConfigRaw")]
pub struct LlmConfig {
    pub active: String,
    pub models: Vec<ModelEntry>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelEntry {
    pub id: String,
    #[serde(default)]
    pub name: String,
    pub base_url: String,
    pub model: String,
    #[serde(default)]
    pub api_key: String,
    /// 上下文窗口 token 数，0 = 未知
    #[serde(default)]
    pub context_window: u32,
}

#[derive(Deserialize)]
struct LlmConfigRaw {
    #[serde(default)]
    active: Option<String>,
    #[serde(default)]
    models: Vec<ModelEntry>,
    // 旧格式
    #[serde(default)]
    base_url: Option<String>,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    api_key: Option<String>,
}

impl From<LlmConfigRaw> for LlmConfig {
    fn from(r: LlmConfigRaw) -> Self {
        let mut models = r.models;
        if models.is_empty() {
            if let (Some(base_url), Some(model)) = (r.base_url, r.model) {
                models.push(ModelEntry {
                    id: "default".into(),
                    name: "默认".into(),
                    base_url,
                    model,
                    api_key: r.api_key.unwrap_or_default(),
                    context_window: 0,
                });
            }
        }
        let active = r
            .active
            .filter(|a| models.iter().any(|m| &m.id == a))
            .or_else(|| models.first().map(|m| m.id.clone()))
            .unwrap_or_else(|| "default".into());
        Self { active, models }
    }
}

impl LlmConfig {
    /// 当前选中的模型；配置为空时返回 None。
    pub fn active_model(&self) -> Option<&ModelEntry> {
        self.models
            .iter()
            .find(|m| m.id == self.active)
            .or_else(|| self.models.first())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentConfig {
    pub workspace: String,
    #[serde(default)]
    pub approval_mode: ApprovalMode,
    #[serde(default = "default_max_turns")]
    pub max_turns: u32,
    #[serde(default = "default_persona")]
    pub persona: String,
    /// 启用的 skill id 列表
    #[serde(default)]
    pub skills: Vec<String>,
    #[serde(default)]
    pub allow: Vec<AllowEntry>,
}

fn default_max_turns() -> u32 {
    30
}

fn default_persona() -> String {
    "builtin/venus".into()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ApprovalMode {
    #[default]
    Ask,
    AutoEdit,
    FullAuto,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AllowEntry {
    pub workspace: String,
    pub commands: Vec<String>,
}

impl Default for Config {
    fn default() -> Self {
        toml::from_str(DEFAULT_CONFIG).expect("DEFAULT_CONFIG must parse")
    }
}

impl AgentConfig {
    /// workspace 展开 `~`，返回绝对路径。
    pub fn workspace_path(&self) -> PathBuf {
        expand_home(&self.workspace)
    }

    /// 指定 workspace 的命令前缀白名单。
    pub fn allowed_commands_for(&self, ws: &Path) -> &[String] {
        self.allow
            .iter()
            .find(|e| expand_home(&e.workspace) == ws)
            .map(|e| e.commands.as_slice())
            .unwrap_or(&[])
    }

    /// 把命令前缀加进指定 workspace 的白名单，已存在则不重复。
    pub fn add_allowed_command_for(&mut self, ws: &Path, prefix: &str) {
        let entry = match self
            .allow
            .iter_mut()
            .find(|e| expand_home(&e.workspace) == ws)
        {
            Some(e) => e,
            None => {
                self.allow.push(AllowEntry {
                    workspace: ws.to_string_lossy().into_owned(),
                    commands: vec![],
                });
                self.allow.last_mut().unwrap()
            }
        };
        if !entry.commands.iter().any(|c| c == prefix) {
            entry.commands.push(prefix.to_string());
        }
    }
}

pub fn expand_home(p: &str) -> PathBuf {
    if let Some(rest) = p.strip_prefix('~') {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest.trim_start_matches('/'));
        }
    }
    PathBuf::from(p)
}

/// 配置目录 `~/.venus-whisper`。
pub fn dir() -> PathBuf {
    dirs::home_dir().expect("home dir").join(".venus-whisper")
}

/// 读配置；目录或文件不存在时写入默认模板。文件存在但解析失败时返回错误，不覆盖用户文件。
pub fn load(dir: &Path) -> Result<Config> {
    let path = ensure_file(dir, CONFIG_FILE, DEFAULT_CONFIG)?;
    Ok(toml::from_str(&std::fs::read_to_string(path)?)?)
}

/// 写配置。注意：toml 序列化会丢掉用户写的注释。
pub fn save(dir: &Path, cfg: &Config) -> Result<()> {
    std::fs::create_dir_all(dir)?;
    std::fs::write(dir.join(CONFIG_FILE), toml::to_string_pretty(cfg)?)?;
    Ok(())
}

fn ensure_file(dir: &Path, name: &str, default: &str) -> Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let path = dir.join(name);
    if !path.exists() {
        std::fs::write(&path, default)?;
    }
    Ok(path)
}

/// 递归监听配置目录（含 themes/），任一文件变化后 debounce 300ms 调用 `on_change`。
/// 监听器随返回的 handle 存活；丢弃 handle 即停止监听。
pub fn watch(
    dir: &Path,
    on_change: impl Fn() + Send + 'static,
) -> Result<notify::RecommendedWatcher> {
    use notify::{RecursiveMode, Watcher};
    std::fs::create_dir_all(dir)?;
    let (tx, rx) = std::sync::mpsc::channel::<()>();
    let mut watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        if res.is_ok() {
            let _ = tx.send(());
        }
    })
    .map_err(|e| e.to_string())?;
    watcher
        .watch(dir, RecursiveMode::Recursive)
        .map_err(|e| e.to_string())?;
    std::thread::spawn(move || {
        while rx.recv().is_ok() {
            // 吞掉 300ms 内的连发事件
            while rx.recv_timeout(Duration::from_millis(300)).is_ok() {}
            on_change();
        }
    });
    Ok(watcher)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_template_roundtrips() {
        let a = Config::default();
        let s = toml::to_string_pretty(&a).unwrap();
        let b: Config = toml::from_str(&s).unwrap();
        assert_eq!(a, b);
        assert_eq!(a.agent.approval_mode, ApprovalMode::Ask);
        assert_eq!(a.agent.max_turns, 30);
        assert_eq!(a.ui.theme, "mint");
        assert_eq!(a.user.name, "你");
        assert!(!a.ui.debug);
        assert_eq!(a.llm.models[0].context_window, 128000);
        // 旧配置没有 [ui] 段也能解析
        let old: Config = toml::from_str(&DEFAULT_CONFIG.replace("[ui]", "[ui_old]")).unwrap();
        assert_eq!(old.ui.theme, "mint");
    }

    #[test]
    fn load_creates_defaults_and_save_reloads() {
        let tmp = std::env::temp_dir().join(format!("vw-cfg-{}", uuid::Uuid::new_v4()));
        let cfg = load(&tmp).unwrap();
        assert!(tmp.join(CONFIG_FILE).exists());
        assert_eq!(cfg, Config::default());

        let mut cfg2 = cfg.clone();
        cfg2.llm.models[0].api_key = "sk-test".into();
        cfg2.agent.workspace = "/tmp/proj".into();
        cfg2.agent
            .add_allowed_command_for(Path::new("/tmp/proj"), "git status");
        cfg2.agent
            .add_allowed_command_for(Path::new("/tmp/proj"), "git status");
        save(&tmp, &cfg2).unwrap();
        let cfg3 = load(&tmp).unwrap();
        assert_eq!(cfg3, cfg2);
        assert_eq!(
            cfg3.agent.allowed_commands_for(Path::new("/tmp/proj")),
            ["git status"]
        );
        assert!(cfg3
            .agent
            .allowed_commands_for(Path::new("/other"))
            .is_empty());
        std::fs::remove_dir_all(&tmp).unwrap();
    }

    #[test]
    fn legacy_flat_llm_config_is_wrapped() {
        let old = r#"
[llm]
base_url = "http://localhost:8000/v1"
model = "gemma"
api_key = "k"
[agent]
workspace = "~"
"#;
        let c: Config = toml::from_str(old).unwrap();
        assert_eq!(c.llm.active, "default");
        assert_eq!(c.llm.models.len(), 1);
        assert_eq!(c.llm.active_model().unwrap().model, "gemma");
        assert_eq!(c.llm.active_model().unwrap().context_window, 0);
        assert!(c.agent.skills.is_empty());

        let multi = r#"
[llm]
active = "b"
[[llm.models]]
id = "a"
base_url = "http://a/v1"
model = "ma"
[[llm.models]]
id = "b"
base_url = "http://b/v1"
model = "mb"
[agent]
workspace = "~"
"#;
        let c: Config = toml::from_str(multi).unwrap();
        assert_eq!(c.llm.active_model().unwrap().model, "mb");
        // active 指向不存在的 id 时回落第一个
        let c: Config =
            toml::from_str(&multi.replace("active = \"b\"", "active = \"zzz\"")).unwrap();
        assert_eq!(c.llm.active_model().unwrap().id, "a");
        // 序列化后再读回保持多模型结构
        let s = toml::to_string_pretty(&c).unwrap();
        let c2: Config = toml::from_str(&s).unwrap();
        assert_eq!(c2.llm.models.len(), 2);
    }

    #[test]
    fn approval_mode_kebab_case() {
        let c: Config =
            toml::from_str(&DEFAULT_CONFIG.replace("\"ask\"", "\"full-auto\"")).unwrap();
        assert_eq!(c.agent.approval_mode, ApprovalMode::FullAuto);
    }
}
