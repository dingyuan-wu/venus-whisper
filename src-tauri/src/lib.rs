mod agent;
mod commands;
mod config;
mod error;
mod events;
mod llm;
mod store;
mod theme;
mod tray;
mod window_state;

use tauri::{Emitter, Manager};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tray::shortcut_plugin())
        .setup(|app| {
            tray::setup(app.handle())?;
            // 首次启动写出默认配置与人格模板，方便用户直接手改
            config::load(&config::dir())?;
            config::load_persona(&config::dir())?;
            theme::ensure_builtin(&config::dir())?;
            if let Some(win) = app.get_webview_window("main") {
                window_state::restore(&win, &config::dir());
                window_state::track(&win, config::dir());
            }
            let handle = app.handle().clone();
            let watcher = config::watch(&config::dir(), move || {
                let _ = handle.emit(events::CONFIG_CHANGED, ());
            })?;
            app.manage(watcher); // 保持监听器存活
            let store = std::sync::Arc::new(store::Store::open(&config::dir().join("data.db"))?);
            app.manage(store.clone());
            app.manage(std::sync::Arc::new(agent::runner::Agent::new(
                store,
                config::dir(),
                None,
            )));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_settings,
            commands::set_settings,
            commands::get_persona,
            commands::set_persona,
            commands::open_config_dir,
            commands::list_themes,
            commands::list_sessions,
            commands::create_session,
            commands::rename_session,
            commands::delete_session,
            commands::get_messages,
            commands::send_message,
            commands::approve_tool,
            commands::cancel_run,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
