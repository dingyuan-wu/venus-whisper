use super::{arg_str, truncate, Tool, ToolCtx};
use crate::error::{Error, Result};
use async_trait::async_trait;
use serde_json::{json, Value};
use std::time::Duration;

pub struct RunShell;

#[async_trait]
impl Tool for RunShell {
    fn name(&self) -> &'static str {
        "run_shell"
    }
    fn description(&self) -> &'static str {
        "在 workspace 目录下执行 shell 命令，返回合并的 stdout/stderr 与退出码。默认 60 秒超时。"
    }
    fn schema(&self) -> Value {
        json!({"type":"object","properties":{
            "command":{"type":"string"},
            "timeout_secs":{"type":"integer","description":"默认 60"}
        },"required":["command"]})
    }
    async fn run(&self, args: Value, ctx: &ToolCtx) -> Result<String> {
        let command = arg_str(&args, "command")?;
        let timeout = args
            .get("timeout_secs")
            .and_then(|v| v.as_u64())
            .unwrap_or(60);
        let mut cmd = if cfg!(windows) {
            let mut c = tokio::process::Command::new("cmd");
            c.args(["/C", command]);
            c
        } else {
            let mut c = tokio::process::Command::new("sh");
            c.args(["-c", command]);
            c
        };
        cmd.current_dir(&ctx.workspace)
            .kill_on_drop(true)
            .stdin(std::process::Stdio::null());
        let fut = cmd.output();
        let out = tokio::time::timeout(Duration::from_secs(timeout), fut)
            .await
            .map_err(|_| Error::Msg(format!("命令超时（{timeout}s），已终止")))??;
        let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
        if !out.stderr.is_empty() {
            if !text.is_empty() {
                text.push('\n');
            }
            text.push_str("[stderr]\n");
            text.push_str(&String::from_utf8_lossy(&out.stderr));
        }
        text.push_str(&format!("\n[exit {}]", out.status.code().unwrap_or(-1)));
        Ok(truncate(text))
    }
}

#[cfg(test)]
mod tests {
    use super::super::temp_ws;
    use super::*;

    #[tokio::test]
    async fn runs_in_workspace_and_reports_exit() {
        let ws = temp_ws();
        let ctx = ToolCtx {
            workspace: ws.clone(),
        };
        let out = RunShell
            .run(json!({"command":"pwd && echo err >&2 && exit 3"}), &ctx)
            .await
            .unwrap();
        assert!(out.starts_with(&ws.to_string_lossy().to_string()), "{out}");
        assert!(out.contains("[stderr]\nerr"));
        assert!(out.ends_with("[exit 3]"));
    }

    #[tokio::test]
    async fn timeout_kills_child() {
        let ws = temp_ws();
        let ctx = ToolCtx { workspace: ws };
        let t = std::time::Instant::now();
        let err = RunShell
            .run(json!({"command":"sleep 5","timeout_secs":1}), &ctx)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("超时"));
        assert!(t.elapsed() < Duration::from_secs(3));
    }
}
