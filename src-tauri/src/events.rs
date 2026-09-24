//! Rust → 前端事件。字段名与 `src/events.ts` 手工对照，由下方契约测试保证一致。

use serde::Serialize;

pub const DELTA: &str = "agent:delta";
pub const TOOL_REQUEST: &str = "agent:tool_request";
pub const TOOL_RESULT: &str = "agent:tool_result";
pub const DONE: &str = "agent:done";
pub const ERROR: &str = "agent:error";
pub const REACTION: &str = "agent:reaction";
pub const CONFIG_CHANGED: &str = "config:changed";
/// 托盘菜单"设置"：让前端打开设置页
pub const OPEN_SETTINGS: &str = "ui:open_settings";

#[derive(Debug, Clone, Serialize)]
pub struct Delta {
    pub session_id: String,
    pub text: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ToolRequest {
    pub session_id: String,
    pub call_id: String,
    pub name: String,
    pub args: serde_json::Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct ToolResult {
    pub session_id: String,
    pub call_id: String,
    pub name: String,
    pub args: serde_json::Value,
    pub output: String,
    pub ok: bool,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct Done {
    pub session_id: String,
    pub message_id: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Error {
    pub session_id: String,
    pub message: String,
}

/// v2 钩子：模型正文末尾 `<meta>` 里的反应建议。
#[derive(Debug, Clone, Serialize, serde::Deserialize, PartialEq)]
pub struct Reaction {
    #[serde(default)]
    pub session_id: String,
    #[serde(default = "neutral")]
    pub emotion: String,
    #[serde(default = "unknown")]
    pub intent: String,
    #[serde(default = "idle")]
    pub suggested_reaction: String,
    #[serde(default)]
    pub confidence: f32,
}
fn neutral() -> String {
    "neutral".into()
}
fn unknown() -> String {
    "unknown".into()
}
fn idle() -> String {
    "idle_breath".into()
}

/// PRD 定义的动作闭集；`suggested_reaction` 不在其中时回落 `idle_breath`。
pub const ACTIONS: &[&str] = &[
    "idle_breath",
    "blink",
    "look_at_cursor",
    "wave",
    "nod",
    "shake_head",
    "happy_jump",
    "confused_scratch",
    "thinking_spin",
    "listening_lean",
    "sad_slump",
    "angry_puff",
    "surprised_jump",
    "sleep_yawn",
    "sleep",
    "work_focus",
    "celebrate",
    "fail_glitch",
    "night_moon",
    "scared_hide",
    "love_heart",
    "bored_stretch",
];

impl Reaction {
    pub fn sanitized(mut self) -> Self {
        if !ACTIONS.contains(&self.suggested_reaction.as_str()) {
            self.suggested_reaction = "idle_breath".into();
        }
        self.confidence = self.confidence.clamp(0.0, 1.0);
        self
    }
}

#[cfg(test)]
mod contract {
    //! 把每个 payload 的字段名与 src/events.ts 里同名 interface 对照，防止手写漂移。
    use super::*;
    use std::collections::{BTreeMap, BTreeSet};

    fn ts_interfaces() -> BTreeMap<String, BTreeSet<String>> {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../src/events.ts");
        let src = std::fs::read_to_string(path).expect("src/events.ts 必须存在");
        let re_if = regex::Regex::new(r"(?s)export interface (\w+)\s*\{(.*?)\}").unwrap();
        let re_field = regex::Regex::new(r"(?m)^\s*(\w+)\??\s*:").unwrap();
        re_if
            .captures_iter(&src)
            .map(|c| {
                (
                    c[1].to_string(),
                    re_field
                        .captures_iter(&c[2])
                        .map(|f| f[1].to_string())
                        .collect(),
                )
            })
            .collect()
    }

    fn keys<T: Serialize>(v: &T) -> BTreeSet<String> {
        serde_json::to_value(v)
            .unwrap()
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect()
    }

    #[test]
    fn rust_payloads_match_ts_interfaces() {
        let ts = ts_interfaces();
        let s = || "s".to_string();
        let samples: Vec<(&str, BTreeSet<String>)> = vec![
            (
                "AgentDelta",
                keys(&Delta {
                    session_id: s(),
                    text: s(),
                }),
            ),
            (
                "AgentToolRequest",
                keys(&ToolRequest {
                    session_id: s(),
                    call_id: s(),
                    name: s(),
                    args: serde_json::Value::Null,
                }),
            ),
            (
                "AgentToolResult",
                keys(&ToolResult {
                    session_id: s(),
                    call_id: s(),
                    name: s(),
                    args: serde_json::Value::Null,
                    output: s(),
                    ok: true,
                    duration_ms: 0,
                }),
            ),
            (
                "AgentDone",
                keys(&Done {
                    session_id: s(),
                    message_id: s(),
                }),
            ),
            (
                "AgentError",
                keys(&Error {
                    session_id: s(),
                    message: s(),
                }),
            ),
            (
                "AgentReaction",
                keys(&Reaction {
                    session_id: s(),
                    emotion: s(),
                    intent: s(),
                    suggested_reaction: s(),
                    confidence: 0.0,
                }),
            ),
        ];
        for (name, rust_keys) in samples {
            let ts_keys = ts
                .get(name)
                .unwrap_or_else(|| panic!("events.ts 缺少 interface {name}"));
            assert_eq!(&rust_keys, ts_keys, "interface {name} 字段不一致");
        }
    }

    #[test]
    fn ts_event_names_match() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../src/events.ts");
        let src = std::fs::read_to_string(path).unwrap();
        for n in [
            DELTA,
            TOOL_REQUEST,
            TOOL_RESULT,
            DONE,
            ERROR,
            REACTION,
            CONFIG_CHANGED,
            OPEN_SETTINGS,
        ] {
            assert!(
                src.contains(&format!("\"{n}\"")),
                "events.ts 缺少事件名 {n}"
            );
        }
    }

    #[test]
    fn reaction_sanitizes_unknown_action() {
        let r: Reaction = serde_json::from_str(
            r#"{"emotion":"happy","suggested_reaction":"backflip","confidence":7}"#,
        )
        .unwrap();
        let r = r.sanitized();
        assert_eq!(r.suggested_reaction, "idle_breath");
        assert_eq!(r.confidence, 1.0);
        assert_eq!(r.intent, "unknown");
    }
}
