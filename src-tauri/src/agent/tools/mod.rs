//! 工具集。所有路径经 `resolve_in_workspace` 校验，越界直接报错。

mod fs;
mod shell;
mod web;

use crate::error::{Error, Result};
use async_trait::async_trait;
use std::path::{Path, PathBuf};

/// 单次工具输出上限，超出截断，防止撑爆上下文。
pub const MAX_OUTPUT: usize = 16 * 1024;

pub struct ToolCtx {
    pub workspace: PathBuf,
}

#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &'static str;
    fn description(&self) -> &'static str;
    /// 参数的 JSON Schema
    fn schema(&self) -> serde_json::Value;
    async fn run(&self, args: serde_json::Value, ctx: &ToolCtx) -> Result<String>;
}

pub fn all() -> Vec<Box<dyn Tool>> {
    vec![
        Box::new(fs::ReadFile),
        Box::new(fs::ListDir),
        Box::new(fs::Grep),
        Box::new(fs::WriteFile),
        Box::new(fs::EditFile),
        Box::new(shell::RunShell),
        Box::new(web::WebFetch),
    ]
}

/// 只读工具（策略层恒放行）。
pub fn is_read_only(name: &str) -> bool {
    matches!(name, "read_file" | "list_dir" | "grep" | "web_fetch")
}

/// OpenAI `tools` 字段。
pub fn openai_defs(tools: &[Box<dyn Tool>]) -> Vec<serde_json::Value> {
    tools
        .iter()
        .map(|t| {
            serde_json::json!({
                "type": "function",
                "function": { "name": t.name(), "description": t.description(), "parameters": t.schema() }
            })
        })
        .collect()
}

/// 把用户给的路径解析到 workspace 内的绝对路径。路径不存在时以已存在的父目录做 canonicalize。
pub fn resolve_in_workspace(workspace: &Path, rel: &str) -> Result<PathBuf> {
    let ws = workspace
        .canonicalize()
        .map_err(|e| Error::Msg(format!("workspace {} 不可用: {e}", workspace.display())))?;
    let raw = if Path::new(rel).is_absolute() {
        PathBuf::from(rel)
    } else {
        ws.join(rel)
    };
    let resolved = match raw.canonicalize() {
        Ok(p) => p,
        Err(_) => {
            let parent = raw
                .parent()
                .ok_or_else(|| Error::Msg(format!("非法路径: {rel}")))?;
            let name = raw
                .file_name()
                .ok_or_else(|| Error::Msg(format!("非法路径: {rel}")))?;
            parent
                .canonicalize()
                .map_err(|_| Error::Msg(format!("父目录不存在: {}", parent.display())))?
                .join(name)
        }
    };
    if !resolved.starts_with(&ws) {
        return Err(Error::Msg(format!(
            "拒绝：路径 {rel} 超出 workspace {}",
            ws.display()
        )));
    }
    Ok(resolved)
}

pub fn truncate(s: String) -> String {
    if s.len() <= MAX_OUTPUT {
        return s;
    }
    let mut cut = MAX_OUTPUT;
    while !s.is_char_boundary(cut) {
        cut -= 1;
    }
    format!("{}\n\n[... 输出已截断，共 {} 字节 ...]", &s[..cut], s.len())
}

pub(super) fn arg_str<'a>(args: &'a serde_json::Value, key: &str) -> Result<&'a str> {
    args.get(key)
        .and_then(|v| v.as_str())
        .ok_or_else(|| Error::Msg(format!("缺少参数 {key}")))
}

#[cfg(test)]
pub(crate) fn temp_ws() -> PathBuf {
    let p = std::env::temp_dir().join(format!("vw-ws-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&p).unwrap();
    p.canonicalize().unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sandbox_rejects_dotdot_and_absolute_outside() {
        let ws = temp_ws();
        assert!(resolve_in_workspace(&ws, "../x").is_err());
        assert!(resolve_in_workspace(&ws, "/etc/passwd").is_err());
        assert!(resolve_in_workspace(&ws, "a/../../x").is_err());
        let ok = resolve_in_workspace(&ws, "new.txt").unwrap();
        assert_eq!(ok, ws.join("new.txt"));
        assert!(
            resolve_in_workspace(&ws, "nodir/new.txt").is_err(),
            "父目录不存在应报错"
        );
    }

    #[cfg(unix)]
    #[test]
    fn sandbox_rejects_symlink_escape() {
        let ws = temp_ws();
        std::os::unix::fs::symlink("/", ws.join("root")).unwrap();
        assert!(resolve_in_workspace(&ws, "root/etc/passwd").is_err());
    }

    #[test]
    fn truncate_keeps_char_boundary() {
        let s = "好".repeat(MAX_OUTPUT);
        let t = truncate(s);
        assert!(t.contains("输出已截断"));
        assert!(t.len() < MAX_OUTPUT + 100);
    }
}
