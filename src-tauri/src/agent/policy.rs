//! 审批策略：决定一次工具调用是直接放行还是需要用户确认。路径越界由工具层沙箱直接报错，这里不管。

use crate::agent::tools::is_read_only;
use crate::config::ApprovalMode;
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Allow,
    Ask,
}

pub fn check(mode: ApprovalMode, tool: &str, args: &Value, allowed: &[String]) -> Verdict {
    if is_read_only(tool) {
        return Verdict::Allow;
    }
    match tool {
        "write_file" | "edit_file" => match mode {
            ApprovalMode::Ask => Verdict::Ask,
            _ => Verdict::Allow,
        },
        "run_shell" => {
            if mode == ApprovalMode::FullAuto {
                return Verdict::Allow;
            }
            let cmd = args
                .get("command")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim();
            if allowed
                .iter()
                .any(|p| cmd == p || cmd.starts_with(&format!("{p} ")))
            {
                Verdict::Allow
            } else {
                Verdict::Ask
            }
        }
        // 未知工具一律问
        _ => Verdict::Ask,
    }
}

/// "总是允许" 的前缀：取命令前两个空白分隔 token；单 token 取一个。
pub fn command_prefix(command: &str) -> String {
    command
        .split_whitespace()
        .take(2)
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use ApprovalMode::*;
    use Verdict::{Allow, Ask as NeedAsk};

    #[test]
    fn truth_table() {
        let none: &[String] = &[];
        let cases = [
            (Ask, "read_file", Allow),
            (Ask, "grep", Allow),
            (Ask, "web_fetch", Allow),
            (Ask, "write_file", NeedAsk),
            (AutoEdit, "write_file", Allow),
            (FullAuto, "edit_file", Allow),
            (Ask, "run_shell", NeedAsk),
            (AutoEdit, "run_shell", NeedAsk),
            (FullAuto, "run_shell", Allow),
            (FullAuto, "unknown_tool", NeedAsk),
        ];
        for (mode, tool, want) in cases {
            assert_eq!(
                check(mode, tool, &json!({"command":"ls"}), none),
                want,
                "{mode:?} {tool}"
            );
        }
    }

    #[test]
    fn allowlist_prefix_matches_whole_tokens() {
        let allowed = vec!["git status".to_string(), "ls".to_string()];
        let v = |c: &str| check(Ask, "run_shell", &json!({"command": c}), &allowed);
        assert_eq!(v("git status"), Allow);
        assert_eq!(v("git status --short"), Allow);
        assert_eq!(v("git statusx"), NeedAsk);
        assert_eq!(v("git push"), NeedAsk);
        assert_eq!(v("ls -la"), Allow);
        assert_eq!(v("lsof"), NeedAsk);
    }

    #[test]
    fn prefix_takes_two_tokens() {
        assert_eq!(command_prefix("git   status --short"), "git status");
        assert_eq!(command_prefix("ls"), "ls");
        assert_eq!(command_prefix("  pnpm test -- --watch"), "pnpm test");
    }
}
