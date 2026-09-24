//! Tauri 命令。前端只通过这里和 Rust 通信。

use crate::agent::runner::{Agent, Decision};
use crate::config::{self, Config};
use crate::error::Result;
use crate::persona::{self, Persona};
use crate::skills::{self, Skill};
use crate::store::{Message, Session, Store};
use crate::theme::{self, Theme};
use std::sync::Arc;
use tauri::{AppHandle, State};

#[tauri::command]
pub fn get_settings() -> Result<Config> {
    config::load(&config::dir())
}

#[tauri::command]
pub fn set_settings(cfg: Config) -> Result<()> {
    config::save(&config::dir(), &cfg)
}

/// 用户头像：~/.venus-whisper/user.<ext>，没有则 None
#[tauri::command]
pub fn get_user_avatar() -> Option<String> {
    persona::local_avatar(&config::dir().join("user"))
}

#[tauri::command]
pub fn list_personas() -> Result<Vec<Persona>> {
    persona::list(&config::dir())
}

/// 把内置人格复制到本地，返回本地 id
#[tauri::command]
pub fn sync_persona(id: String, overwrite: bool) -> Result<String> {
    persona::sync_to_local(&config::dir(), &id, overwrite)
}

/// 不传 id 时读内置默认；file 可选 "bible" 读 character_bible.md
#[tauri::command]
pub fn get_persona(id: Option<String>, file: Option<String>) -> Result<String> {
    persona::load_file(
        &config::dir(),
        id.as_deref().unwrap_or(persona::DEFAULT_ID),
        file.as_deref().unwrap_or("prompt"),
    )
}

/// 只能写本地 md 人格
#[tauri::command]
pub fn set_persona(id: Option<String>, text: String) -> Result<()> {
    persona::save_prompt(
        &config::dir(),
        id.as_deref().unwrap_or(persona::DEFAULT_ID),
        &text,
    )
}

#[tauri::command]
pub fn list_themes() -> Result<Vec<Theme>> {
    theme::load_all(&config::dir())
}

#[tauri::command]
pub fn open_config_dir(app: AppHandle) -> Result<()> {
    use tauri_plugin_opener::OpenerExt;
    let dir = config::dir();
    std::fs::create_dir_all(&dir)?;
    app.opener()
        .open_path(dir.to_string_lossy(), None::<&str>)
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn list_sessions(store: State<Arc<Store>>) -> Result<Vec<Session>> {
    store.list_sessions()
}

#[tauri::command]
pub fn create_session(store: State<Arc<Store>>, title: Option<String>) -> Result<Session> {
    let cfg = config::load(&config::dir())?;
    store.create_session(
        title.as_deref().unwrap_or("新会话"),
        &cfg.agent.workspace_path().to_string_lossy(),
    )
}

#[tauri::command]
pub fn rename_session(store: State<Arc<Store>>, id: String, title: String) -> Result<()> {
    store.rename_session(&id, &title)
}

#[tauri::command]
pub fn delete_session(store: State<Arc<Store>>, id: String) -> Result<()> {
    store.delete_session(&id)
}

#[tauri::command]
pub fn get_messages(store: State<Arc<Store>>, session_id: String) -> Result<Vec<Message>> {
    store.get_messages(&session_id)
}

#[tauri::command]
pub fn send_message(app: AppHandle, agent: State<Arc<Agent>>, session_id: String, text: String) {
    let agent = agent.inner().clone();
    tauri::async_runtime::spawn(agent.run(Arc::new(app), session_id, text));
}

#[tauri::command]
pub fn approve_tool(
    agent: State<Arc<Agent>>,
    session_id: String,
    call_id: String,
    decision: Decision,
) -> bool {
    agent.approve(&session_id, &call_id, decision)
}

#[tauri::command]
pub fn cancel_run(agent: State<Arc<Agent>>, session_id: String) {
    agent.cancel(&session_id)
}

#[tauri::command]
pub fn list_skills() -> Result<Vec<Skill>> {
    skills::list(&config::dir())
}

#[tauri::command]
pub fn get_skill(id: String) -> Result<String> {
    skills::load(&config::dir(), &id)
}

/// 只能写本地 skill
#[tauri::command]
pub fn set_skill(id: String, text: String) -> Result<()> {
    skills::save(&config::dir(), &id, &text)
}

#[tauri::command]
pub fn sync_skill(id: String, overwrite: bool) -> Result<String> {
    skills::sync_to_local(&config::dir(), &id, overwrite)
}

/// 会话当前上下文占用估算（系统提示词 + 历史）
#[tauri::command]
pub fn estimate_context(
    store: State<Arc<Store>>,
    session_id: String,
) -> Result<crate::events::Usage> {
    crate::agent::runner::estimate_context(&store, &config::dir(), &session_id)
}
