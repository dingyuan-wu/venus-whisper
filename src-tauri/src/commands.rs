//! Tauri 命令。前端只通过这里和 Rust 通信。

use crate::agent::runner::{Agent, Decision};
use crate::config::{self, Config};
use crate::error::Result;
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

#[tauri::command]
pub fn get_persona() -> Result<String> {
    config::load_persona(&config::dir())
}

#[tauri::command]
pub fn set_persona(text: String) -> Result<()> {
    config::save_persona(&config::dir(), &text)
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
