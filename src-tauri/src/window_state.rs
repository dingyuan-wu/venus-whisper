//! 记住主窗口的位置、尺寸、最大化状态：`~/.venus-whisper/window.json`。
//! 改变后 500ms 内无新事件才落盘；启动时若记录的位置已不在任何显示器上则忽略位置只恢复尺寸。

use crate::error::Result;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tauri::{Manager, PhysicalPosition, PhysicalSize, Runtime, WebviewWindow, WindowEvent};

pub const FILE: &str = "window.json";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WindowState {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    #[serde(default)]
    pub maximized: bool,
}

/// 显示器的物理矩形
#[derive(Debug, Clone, Copy)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
}

/// 窗口左上角往里 (32,32) 的点落在某个显示器内才算可见，避免恢复到已拔掉的外接屏上。
pub fn position_visible(state: &WindowState, monitors: &[Rect]) -> bool {
    let (px, py) = (state.x + 32, state.y + 32);
    monitors.iter().any(|m| {
        px >= m.x
            && py >= m.y
            && (px as i64) < m.x as i64 + m.w as i64
            && (py as i64) < m.y as i64 + m.h as i64
    })
}

pub fn load(dir: &Path) -> Option<WindowState> {
    let text = std::fs::read_to_string(dir.join(FILE)).ok()?;
    serde_json::from_str(&text).ok()
}

pub fn save(dir: &Path, state: &WindowState) -> Result<()> {
    std::fs::create_dir_all(dir)?;
    std::fs::write(dir.join(FILE), serde_json::to_string_pretty(state)?)?;
    Ok(())
}

fn capture<R: Runtime>(win: &WebviewWindow<R>) -> Option<WindowState> {
    let maximized = win.is_maximized().ok()?;
    let pos = win.outer_position().ok()?;
    let size = win.inner_size().ok()?;
    if size.width < 200 || size.height < 200 {
        return None; // 最小化时会拿到 0 尺寸，不记
    }
    Some(WindowState {
        x: pos.x,
        y: pos.y,
        width: size.width,
        height: size.height,
        maximized,
    })
}

/// 启动时恢复。
pub fn restore<R: Runtime>(win: &WebviewWindow<R>, dir: &Path) {
    let Some(state) = load(dir) else { return };
    let monitors: Vec<Rect> = win
        .available_monitors()
        .unwrap_or_default()
        .iter()
        .map(|m| Rect {
            x: m.position().x,
            y: m.position().y,
            w: m.size().width,
            h: m.size().height,
        })
        .collect();
    let _ = win.set_size(PhysicalSize::new(state.width, state.height));
    if position_visible(&state, &monitors) {
        let _ = win.set_position(PhysicalPosition::new(state.x, state.y));
    } else {
        let _ = win.center();
    }
    if state.maximized {
        let _ = win.maximize();
    }
}

/// 监听移动/缩放，debounce 500ms 后落盘；关闭前立即落盘。
pub fn track<R: Runtime>(win: &WebviewWindow<R>, dir: PathBuf) {
    let gen = Arc::new(AtomicU64::new(0));
    let handle = win.clone();
    let dir = Arc::new(dir);
    win.on_window_event(move |ev| match ev {
        WindowEvent::Moved(_) | WindowEvent::Resized(_) => {
            let my = gen.fetch_add(1, Ordering::SeqCst) + 1;
            let (gen, handle, dir) = (gen.clone(), handle.clone(), dir.clone());
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(Duration::from_millis(500)).await;
                if gen.load(Ordering::SeqCst) == my {
                    if let Some(s) = capture(&handle) {
                        let _ = save(&dir, &s);
                    }
                }
            });
        }
        WindowEvent::CloseRequested { .. } => {
            if let Some(s) = capture(&handle) {
                let _ = save(&dir, &s);
            }
        }
        _ => {}
    });
}

/// 托盘"退出"等主动退出路径调用。
pub fn save_now<R: Runtime>(app: &tauri::AppHandle<R>, dir: &Path) {
    if let Some(win) = app.get_webview_window("main") {
        if let Some(s) = capture(&win) {
            let _ = save(dir, &s);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visibility_check_against_monitors() {
        let main = Rect {
            x: 0,
            y: 0,
            w: 2560,
            h: 1440,
        };
        let ext = Rect {
            x: 2560,
            y: -200,
            w: 1920,
            h: 1080,
        };
        let s = |x, y| WindowState {
            x,
            y,
            width: 1000,
            height: 700,
            maximized: false,
        };
        assert!(position_visible(&s(100, 100), &[main, ext]));
        assert!(position_visible(&s(3000, 0), &[main, ext]));
        assert!(
            !position_visible(&s(3000, 0), &[main]),
            "外接屏拔掉后不可见"
        );
        assert!(!position_visible(&s(-500, 100), &[main]));
        assert!(
            !position_visible(&s(2540, 100), &[main]),
            "左上角贴边但 (x+32) 已出屏"
        );
    }

    #[test]
    fn save_load_roundtrip_and_missing_file() {
        let d = std::env::temp_dir().join(format!("vw-win-{}", uuid::Uuid::new_v4()));
        assert!(load(&d).is_none());
        let s = WindowState {
            x: 10,
            y: 20,
            width: 1000,
            height: 700,
            maximized: true,
        };
        save(&d, &s).unwrap();
        assert_eq!(load(&d).unwrap(), s);
        std::fs::write(d.join(FILE), "{ broken").unwrap();
        assert!(load(&d).is_none(), "坏文件当作没有");
        std::fs::remove_dir_all(&d).unwrap();
    }
}
