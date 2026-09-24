//! Agent 循环：LLM 流式调用 → tool_calls 判定 → 策略审批 → 执行 → 结果回灌，直到无 tool_calls。

use crate::agent::policy::{self, Verdict};
use crate::agent::tools::{self, Tool, ToolCtx};
use crate::config::{self, Config};
use crate::error::Result;
use crate::events;
use crate::llm::openai::OpenAiClient;
use crate::llm::{ChatRequest, LlmClient, LlmEvent, ToolCall};
use crate::store::Store;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tokio::sync::oneshot;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Decision {
    Allow,
    Deny,
    Always,
}

/// 事件出口。生产环境是 tauri AppHandle，测试用收集器。
pub trait Emit: Send + Sync {
    fn emit_json(&self, event: &str, payload: Value);
}

impl Emit for tauri::AppHandle {
    fn emit_json(&self, event: &str, payload: Value) {
        let _ = tauri::Emitter::emit(self, event, payload);
    }
}

pub struct Agent {
    pub store: Arc<Store>,
    pub config_dir: PathBuf,
    /// 测试注入；None 时每轮按配置新建 OpenAI 客户端
    pub llm_override: Option<Arc<dyn LlmClient>>,
    tools: Vec<Box<dyn Tool>>,
    approvals: Mutex<HashMap<String, oneshot::Sender<Decision>>>,
    cancels: Mutex<HashMap<String, Arc<AtomicBool>>>,
}

impl Agent {
    pub fn new(
        store: Arc<Store>,
        config_dir: PathBuf,
        llm_override: Option<Arc<dyn LlmClient>>,
    ) -> Self {
        Self {
            store,
            config_dir,
            llm_override,
            tools: tools::all(),
            approvals: Mutex::new(HashMap::new()),
            cancels: Mutex::new(HashMap::new()),
        }
    }

    /// 前端回应审批。审批表内部以 `session_id/call_id` 为键。
    pub fn approve(&self, session_id: &str, call_id: &str, decision: Decision) -> bool {
        match self
            .approvals
            .lock()
            .unwrap()
            .remove(&format!("{session_id}/{call_id}"))
        {
            Some(tx) => tx.send(decision).is_ok(),
            None => false,
        }
    }

    /// 取消会话当前运行：置位标志，并把该会话挂起的审批全部 Deny。
    pub fn cancel(&self, session_id: &str) {
        if let Some(flag) = self.cancels.lock().unwrap().get(session_id) {
            flag.store(true, Ordering::SeqCst);
        }
        let pending: Vec<_> = self
            .approvals
            .lock()
            .unwrap()
            .keys()
            .filter(|k| k.starts_with(&format!("{session_id}/")))
            .cloned()
            .collect();
        for k in pending {
            if let Some(tx) = self.approvals.lock().unwrap().remove(&k) {
                let _ = tx.send(Decision::Deny);
            }
        }
    }

    #[cfg(test)]
    pub fn is_running(&self, session_id: &str) -> bool {
        self.cancels.lock().unwrap().contains_key(session_id)
    }

    /// 处理一条用户消息，直到模型给出最终回复或出错。所有进展通过 `emit` 发出。
    pub async fn run(self: Arc<Self>, emit: Arc<dyn Emit>, session_id: String, text: String) {
        let cancel = Arc::new(AtomicBool::new(false));
        {
            let mut c = self.cancels.lock().unwrap();
            if c.contains_key(&session_id) {
                emit.emit_json(
                    events::ERROR,
                    json!(events::Error {
                        session_id,
                        message: "该会话正在运行中".into()
                    }),
                );
                return;
            }
            c.insert(session_id.clone(), cancel.clone());
        }
        let res = self.run_inner(&*emit, &session_id, &text, &cancel).await;
        self.cancels.lock().unwrap().remove(&session_id);
        if let Err(e) = res {
            emit.emit_json(
                events::ERROR,
                json!(events::Error {
                    session_id,
                    message: e.to_string()
                }),
            );
        }
    }

    async fn run_inner(
        &self,
        emit: &dyn Emit,
        session_id: &str,
        text: &str,
        cancel: &AtomicBool,
    ) -> Result<()> {
        let session = self
            .store
            .get_session(session_id)?
            .ok_or_else(|| format!("会话不存在: {session_id}"))?;
        let workspace = PathBuf::from(&session.workspace);
        self.store.add_message(
            session_id,
            "user",
            &json!({"role": "user", "content": text}),
        )?;

        let cfg0 = config::load(&self.config_dir)?;
        let max_turns = cfg0.agent.max_turns.max(1);
        let base_len = self.store.get_messages(session_id)?.len(); // 历史 + 本次用户消息
        let tool_defs = tools::openai_defs(&self.tools);
        let tools_tokens =
            crate::tokens::estimate(&serde_json::to_string(&tool_defs).unwrap_or_default());

        for call_no in 1..=max_turns {
            if cancel.load(Ordering::SeqCst) {
                return Err("已取消".into());
            }
            let cfg = config::load(&self.config_dir)?;
            let persona = crate::persona::load_active(&self.config_dir, &cfg)?;
            let skills = crate::skills::load_enabled(&self.config_dir, &cfg);
            let llm = self.llm(&cfg);

            let system = system_prompt(&persona, &skills);
            let system_tokens = crate::tokens::estimate(&system) + tools_tokens;
            let persona_tokens = crate::tokens::estimate(&persona);
            let skills_tokens: u32 = skills
                .iter()
                .map(|(n, b)| crate::tokens::estimate(n) + crate::tokens::estimate(b))
                .sum();
            let convention_tokens =
                crate::tokens::estimate(&system).saturating_sub(persona_tokens + skills_tokens);
            let mut messages = vec![json!({"role": "system", "content": system})];
            messages.extend(
                self.store
                    .get_messages(session_id)?
                    .into_iter()
                    .map(|m| m.content),
            );

            let model = cfg
                .llm
                .active_model()
                .ok_or("没有配置任何模型，请先在设置里添加")?
                .model
                .clone();
            let prompt_estimate = crate::tokens::estimate_messages(&messages);
            let context_window = cfg
                .llm
                .active_model()
                .map(|m| m.context_window)
                .unwrap_or(0);
            // messages[0] 是 system；之后依次为历史、本次用户消息、本轮工具往返
            let body = &messages[1..];
            let hist_n = base_len.saturating_sub(1).min(body.len());
            let (history_msgs, rest) = body.split_at(hist_n);
            let (current_msgs, run_msgs) = rest.split_at(1.min(rest.len()));
            let breakdown = events::Breakdown {
                persona: persona_tokens,
                skills: skills_tokens,
                convention: convention_tokens,
                tools: tools_tokens,
                history: crate::tokens::estimate_messages(history_msgs),
                current: crate::tokens::estimate_messages(current_msgs),
                run: crate::tokens::estimate_messages(run_msgs),
            };
            let mut stream = llm
                .chat(ChatRequest {
                    model,
                    messages,
                    tools: tool_defs.clone(),
                })
                .await?;

            let mut usage: Option<(u32, u32)> = None;
            let mut full = String::new();
            let mut emitted = 0usize;
            let mut calls: Vec<ToolCall> = vec![];
            while let Some(ev) = stream.next().await {
                if cancel.load(Ordering::SeqCst) {
                    return Err("已取消".into());
                }
                match ev? {
                    LlmEvent::Delta(t) => {
                        full.push_str(&t);
                        let safe = safe_emit_len(&full);
                        if safe > emitted {
                            emit.emit_json(
                                events::DELTA,
                                json!(events::Delta {
                                    session_id: session_id.into(),
                                    text: full[emitted..safe].to_string()
                                }),
                            );
                            emitted = safe;
                        }
                    }
                    LlmEvent::ToolCall(c) => calls.push(c),
                    LlmEvent::Usage {
                        prompt_tokens,
                        completion_tokens,
                    } => usage = Some((prompt_tokens, completion_tokens)),
                    LlmEvent::Done => break,
                }
            }
            // 用量：优先服务端；没有就本地估算（含工具调用参数）
            let completion_estimate = crate::tokens::estimate(&full)
                + calls
                    .iter()
                    .map(|c| crate::tokens::estimate(&c.arguments) + 8)
                    .sum::<u32>();
            let (prompt_tokens, completion_tokens, estimated) = match usage {
                Some((p, c)) => (p, c, false),
                None => (prompt_estimate, completion_estimate, true),
            };
            emit.emit_json(
                events::USAGE,
                json!(events::Usage {
                    session_id: session_id.into(),
                    call: call_no,
                    prompt_tokens,
                    completion_tokens,
                    system_tokens,
                    estimated,
                    context_window,
                    breakdown,
                }),
            );
            let (clean, reaction) = strip_meta(&full);
            if clean.len() > emitted {
                emit.emit_json(
                    events::DELTA,
                    json!(events::Delta {
                        session_id: session_id.into(),
                        text: clean[emitted..].to_string()
                    }),
                );
            }

            if calls.is_empty() {
                let m = self.store.add_message(
                    session_id,
                    "assistant",
                    &json!({"role": "assistant", "content": clean}),
                )?;
                if let Some(r) = reaction {
                    let r = events::Reaction {
                        session_id: session_id.into(),
                        ..r
                    }
                    .sanitized();
                    emit.emit_json(events::REACTION, json!(r));
                }
                emit.emit_json(
                    events::DONE,
                    json!(events::Done {
                        session_id: session_id.into(),
                        message_id: m.id
                    }),
                );
                return Ok(());
            }

            // 带 tool_calls 的 assistant 消息
            let tool_calls_json: Vec<Value> = calls
                .iter()
                .map(|c| json!({"id": c.id, "type": "function", "function": {"name": c.name, "arguments": c.arguments}}))
                .collect();
            self.store.add_message(
                session_id,
                "assistant",
                &json!({"role": "assistant", "content": if clean.is_empty() { Value::Null } else { Value::String(clean.clone()) }, "tool_calls": tool_calls_json}),
            )?;

            for call in calls {
                if cancel.load(Ordering::SeqCst) {
                    return Err("已取消".into());
                }
                let (output, ok, ms) = self
                    .execute(emit, session_id, &workspace, &call, &cfg)
                    .await;
                emit.emit_json(
                    events::TOOL_RESULT,
                    json!(events::ToolResult {
                        session_id: session_id.into(),
                        call_id: call.id.clone(),
                        name: call.name.clone(),
                        args: serde_json::from_str(&call.arguments)
                            .unwrap_or(Value::String(call.arguments.clone())),
                        output: output.clone(),
                        ok,
                        duration_ms: ms,
                    }),
                );
                self.store.add_message(
                    session_id,
                    "tool",
                    &json!({"role": "tool", "tool_call_id": call.id, "content": output}),
                )?;
            }
        }
        Err(format!("达到最大轮次 {max_turns}，已停止").into())
    }

    fn llm(&self, cfg: &Config) -> Arc<dyn LlmClient> {
        match &self.llm_override {
            Some(l) => l.clone(),
            None => {
                let m = cfg.llm.active_model();
                Arc::new(OpenAiClient::new(
                    m.map(|m| m.base_url.as_str()).unwrap_or(""),
                    m.map(|m| m.api_key.as_str()).unwrap_or(""),
                ))
            }
        }
    }

    /// 策略判定 → 可能等待审批 → 执行。返回 (输出, 是否成功, 耗时 ms)。
    async fn execute(
        &self,
        emit: &dyn Emit,
        session_id: &str,
        workspace: &Path,
        call: &ToolCall,
        cfg: &Config,
    ) -> (String, bool, u64) {
        let started = std::time::Instant::now();
        let args: Value = match serde_json::from_str(&call.arguments) {
            Ok(v) => v,
            Err(e) => return (format!("error: 参数不是合法 JSON: {e}"), false, 0),
        };
        let Some(tool) = self.tools.iter().find(|t| t.name() == call.name) else {
            return (format!("error: 未知工具 {}", call.name), false, 0);
        };

        let allowed = cfg.agent.allowed_commands_for(workspace);
        if policy::check(cfg.agent.approval_mode, &call.name, &args, allowed) == Verdict::Ask {
            let (tx, rx) = oneshot::channel();
            let key = format!("{session_id}/{}", call.id);
            self.approvals.lock().unwrap().insert(key.clone(), tx);
            emit.emit_json(
                events::TOOL_REQUEST,
                json!(events::ToolRequest {
                    session_id: session_id.into(),
                    call_id: call.id.clone(),
                    name: call.name.clone(),
                    args: args.clone()
                }),
            );
            let decision = rx.await.unwrap_or(Decision::Deny);
            self.approvals.lock().unwrap().remove(&key);
            match decision {
                Decision::Deny => return ("denied by user".into(), false, ms(started)),
                Decision::Always => {
                    if call.name == "run_shell" {
                        if let Some(cmd) = args.get("command").and_then(|v| v.as_str()) {
                            let _ = self.persist_allow(workspace, &policy::command_prefix(cmd));
                        }
                    }
                }
                Decision::Allow => {}
            }
        }

        let ctx = ToolCtx {
            workspace: workspace.to_path_buf(),
        };
        match tool.run(args, &ctx).await {
            Ok(out) => (out, true, ms(started)),
            Err(e) => (format!("error: {e}"), false, ms(started)),
        }
    }

    fn persist_allow(&self, workspace: &Path, prefix: &str) -> Result<()> {
        let mut cfg = config::load(&self.config_dir)?;
        cfg.agent.add_allowed_command_for(workspace, prefix);
        config::save(&self.config_dir, &cfg)
    }
}

fn ms(t: std::time::Instant) -> u64 {
    t.elapsed().as_millis() as u64
}

const META_OPEN: &str = "<meta>";
const META_CLOSE: &str = "</meta>";

/// 估算某会话下一次调用的上下文占用（系统提示词 + 历史），供右侧栏在没有新调用时显示。
pub fn estimate_context(
    store: &Store,
    config_dir: &Path,
    session_id: &str,
) -> Result<events::Usage> {
    let cfg = config::load(config_dir)?;
    let persona = crate::persona::load_active(config_dir, &cfg)?;
    let skills = crate::skills::load_enabled(config_dir, &cfg);
    let system = system_prompt(&persona, &skills);
    let tool_defs = tools::openai_defs(&tools::all());
    let tools_tokens =
        crate::tokens::estimate(&serde_json::to_string(&tool_defs).unwrap_or_default());
    let system_tokens = crate::tokens::estimate(&system) + tools_tokens;
    let persona_tokens = crate::tokens::estimate(&persona);
    let skills_tokens: u32 = skills
        .iter()
        .map(|(n, b)| crate::tokens::estimate(n) + crate::tokens::estimate(b))
        .sum();
    let history: Vec<Value> = store
        .get_messages(session_id)?
        .into_iter()
        .map(|m| m.content)
        .collect();
    let history_tokens = crate::tokens::estimate_messages(&history);
    Ok(events::Usage {
        session_id: session_id.into(),
        call: 0,
        prompt_tokens: system_tokens + history_tokens,
        completion_tokens: 0,
        system_tokens,
        estimated: true,
        context_window: cfg
            .llm
            .active_model()
            .map(|m| m.context_window)
            .unwrap_or(0),
        breakdown: events::Breakdown {
            persona: persona_tokens,
            skills: skills_tokens,
            convention: crate::tokens::estimate(&system)
                .saturating_sub(persona_tokens + skills_tokens),
            tools: tools_tokens,
            history: history_tokens,
            current: 0,
            run: 0,
        },
    })
}

fn system_prompt(persona: &str, skills: &[(String, String)]) -> String {
    let mut out = persona.to_string();
    if !skills.is_empty() {
        out.push_str("\n\n## 可用技能\n以下是用户启用的技能文档，按需遵循。\n");
        for (name, body) in skills {
            out.push_str(&format!("\n### {name}\n{body}\n"));
        }
    }
    let persona = out;
    format!(
        "{persona}\n\n\
         ## 输出约定\n\
         当你不调用工具、给出最终回复时，在正文末尾另起一行追加：\n\
         {META_OPEN}{{\"emotion\":\"...\",\"intent\":\"...\",\"suggested_reaction\":\"...\",\"confidence\":0.0}}{META_CLOSE}\n\
         emotion ∈ happy|sad|angry|confused|excited|neutral|sleepy|scared；\
         intent ∈ greeting|question|praise|criticism|command|farewell|status|unknown；\
         suggested_reaction ∈ {}；confidence 为 0 到 1。正文里不要出现 {META_OPEN} 标签。",
        events::ACTIONS.join("|")
    )
}

/// 正文中可以安全发给前端的长度：遇到 `<meta` 起就截住，末尾可能是 `<meta` 前缀的部分也先扣住，
/// 末尾空白也扣住（meta 前通常有换行，最终正文会 trim）。
fn safe_emit_len(full: &str) -> usize {
    let mut end = full.len();
    if let Some(i) = full.find("<meta") {
        end = i;
    } else {
        for k in (1.."<meta".len()).rev() {
            if full.ends_with(&"<meta"[..k]) {
                end = full.len() - k;
                break;
            }
        }
    }
    full[..end].trim_end().len()
}

/// 剥离末尾 `<meta>…</meta>`，返回 (正文, 解析出的反应)。
fn strip_meta(full: &str) -> (String, Option<events::Reaction>) {
    let Some(start) = full.rfind(META_OPEN) else {
        return (full.trim_end().to_string(), None);
    };
    let body = &full[start + META_OPEN.len()..];
    let json_part = body.find(META_CLOSE).map(|e| &body[..e]).unwrap_or(body);
    let reaction = serde_json::from_str::<events::Reaction>(json_part.trim()).ok();
    (full[..start].trim_end().to_string(), reaction)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::Result;
    use crate::llm::EventStream;
    use async_trait::async_trait;
    use std::collections::VecDeque;
    use std::time::Duration;

    struct FakeLlm {
        scripts: Mutex<VecDeque<Vec<LlmEvent>>>,
        requests: Mutex<Vec<ChatRequest>>,
    }
    #[async_trait]
    impl LlmClient for FakeLlm {
        async fn chat(&self, req: ChatRequest) -> Result<EventStream> {
            self.requests.lock().unwrap().push(req);
            let script = self
                .scripts
                .lock()
                .unwrap()
                .pop_front()
                .expect("script exhausted");
            Ok(futures_util::stream::iter(script.into_iter().map(Ok)).boxed())
        }
    }

    #[derive(Default)]
    struct Collector(Mutex<Vec<(String, Value)>>);
    impl Emit for Collector {
        fn emit_json(&self, event: &str, payload: Value) {
            self.0.lock().unwrap().push((event.to_string(), payload));
        }
    }
    impl Collector {
        fn names(&self) -> Vec<String> {
            self.0
                .lock()
                .unwrap()
                .iter()
                .map(|(n, _)| n.clone())
                .collect()
        }
        fn find(&self, name: &str) -> Option<Value> {
            self.0
                .lock()
                .unwrap()
                .iter()
                .find(|(n, _)| n == name)
                .map(|(_, p)| p.clone())
        }
    }

    fn tc(id: &str, name: &str, args: &str) -> LlmEvent {
        LlmEvent::ToolCall(ToolCall {
            id: id.into(),
            name: name.into(),
            arguments: args.into(),
        })
    }

    fn setup(
        mode: &str,
        scripts: Vec<Vec<LlmEvent>>,
    ) -> (Arc<Agent>, Arc<FakeLlm>, Arc<Collector>, String, PathBuf) {
        let cfg_dir = std::env::temp_dir().join(format!("vw-run-{}", uuid::Uuid::new_v4()));
        let mut cfg = config::load(&cfg_dir).unwrap();
        cfg.agent.approval_mode = toml::from_str::<Value>(&format!("m = \"{mode}\""))
            .ok()
            .and_then(|_| serde_json::from_value(json!(mode)).ok())
            .unwrap();
        config::save(&cfg_dir, &cfg).unwrap();
        let ws = tools::temp_ws();
        std::fs::write(ws.join("a.txt"), "hello").unwrap();
        let store = Arc::new(Store::open_in_memory().unwrap());
        let session = store.create_session("t", &ws.to_string_lossy()).unwrap();
        let llm = Arc::new(FakeLlm {
            scripts: Mutex::new(scripts.into()),
            requests: Mutex::new(vec![]),
        });
        let agent = Arc::new(Agent::new(store, cfg_dir.clone(), Some(llm.clone())));
        (
            agent,
            llm,
            Arc::new(Collector::default()),
            session.id,
            cfg_dir,
        )
    }

    #[tokio::test]
    async fn tool_round_trip_then_final_reply_with_meta() {
        let (agent, llm, col, sid, _) = setup(
            "ask",
            vec![
                vec![tc("c1", "read_file", r#"{"path":"a.txt"}"#), LlmEvent::Done],
                vec![
                    LlmEvent::Delta("文件内容是 hello".into()),
                    LlmEvent::Delta("\n<me".into()),
                    LlmEvent::Delta("ta>{\"emotion\":\"happy\",\"intent\":\"question\",\"suggested_reaction\":\"nod\",\"confidence\":0.9}</meta>".into()),
                    LlmEvent::Done,
                ],
            ],
        );
        agent
            .clone()
            .run(col.clone(), sid.clone(), "a.txt 里有什么".into())
            .await;

        let names: Vec<_> = col
            .names()
            .into_iter()
            .filter(|n| n != events::USAGE)
            .collect();
        assert_eq!(
            names,
            vec![
                events::TOOL_RESULT,
                events::DELTA,
                events::REACTION,
                events::DONE
            ]
        );
        let usages: Vec<Value> = col
            .0
            .lock()
            .unwrap()
            .iter()
            .filter(|(e, _)| e == events::USAGE)
            .map(|(_, p)| p.clone())
            .collect();
        assert_eq!(usages.len(), 2, "两次 LLM 调用两条用量");
        assert_eq!(usages[0]["call"], 1);
        assert_eq!(usages[1]["call"], 2);
        assert_eq!(usages[0]["estimated"], true, "假客户端没给 usage，走估算");
        assert!(
            usages[1]["prompt_tokens"].as_u64().unwrap()
                > usages[0]["prompt_tokens"].as_u64().unwrap(),
            "第二次上下文更长"
        );
        assert!(usages[1]["system_tokens"].as_u64().unwrap() > 0);
        let b0 = &usages[0]["breakdown"];
        let b1 = &usages[1]["breakdown"];
        assert!(b0["persona"].as_u64().unwrap() > 0 && b0["tools"].as_u64().unwrap() > 0);
        assert!(b0["current"].as_u64().unwrap() > 0, "本次用户消息");
        assert_eq!(b0["run"], 0, "第一次调用还没有工具往返");
        assert!(
            b1["run"].as_u64().unwrap() > 0,
            "第二次包含 tool_calls 与 tool 结果"
        );
        assert_eq!(b0["history"], 0, "新会话没有历史");
        let est = estimate_context(&agent.store, &agent.config_dir, &sid).unwrap();
        assert_eq!(est.call, 0);
        assert!(est.prompt_tokens >= est.system_tokens);
        let tr = col.find(events::TOOL_RESULT).unwrap();
        assert_eq!(tr["output"], "hello");
        assert_eq!(tr["ok"], true);
        assert_eq!(col.find(events::DELTA).unwrap()["text"], "文件内容是 hello");
        assert_eq!(
            col.find(events::REACTION).unwrap()["suggested_reaction"],
            "nod"
        );

        let msgs = agent.store.get_messages(&sid).unwrap();
        let roles: Vec<_> = msgs.iter().map(|m| m.role.as_str()).collect();
        assert_eq!(roles, ["user", "assistant", "tool", "assistant"]);
        assert_eq!(msgs[3].content["content"], "文件内容是 hello");

        let reqs = llm.requests.lock().unwrap();
        assert_eq!(reqs.len(), 2);
        assert_eq!(reqs[0].messages[0]["role"], "system");
        assert!(reqs[0].messages[0]["content"]
            .as_str()
            .unwrap()
            .contains("<meta>"));
        assert_eq!(reqs[1].messages.last().unwrap()["role"], "tool");
        assert_eq!(reqs[1].messages.last().unwrap()["content"], "hello");
        assert!(!agent.is_running(&sid));
    }

    #[tokio::test]
    async fn ask_mode_waits_for_approval_and_always_persists() {
        let (agent, _, col, sid, cfg_dir) = setup(
            "ask",
            vec![
                vec![
                    tc("c1", "run_shell", r#"{"command":"echo hi"}"#),
                    tc("c2", "write_file", r#"{"path":"b.txt","content":"x"}"#),
                    LlmEvent::Done,
                ],
                vec![LlmEvent::Delta("done".into()), LlmEvent::Done],
            ],
        );
        let handle = tokio::spawn(agent.clone().run(col.clone(), sid.clone(), "go".into()));

        let wait_request = |col: Arc<Collector>, n: usize| async move {
            for _ in 0..100 {
                let reqs: Vec<Value> = col
                    .0
                    .lock()
                    .unwrap()
                    .iter()
                    .filter(|(e, _)| e == events::TOOL_REQUEST)
                    .map(|(_, p)| p.clone())
                    .collect();
                if reqs.len() >= n {
                    return reqs[n - 1].clone();
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            panic!("no tool_request");
        };
        let r1 = wait_request(col.clone(), 1).await;
        assert_eq!(r1["name"], "run_shell");
        assert!(agent.approve(&sid, r1["call_id"].as_str().unwrap(), Decision::Always));

        let r2 = wait_request(col.clone(), 2).await;
        assert_eq!(r2["name"], "write_file");
        assert!(agent.approve(&sid, r2["call_id"].as_str().unwrap(), Decision::Deny));
        assert_eq!(r2["call_id"], "c2");
        handle.await.unwrap();

        let results: Vec<Value> = col
            .0
            .lock()
            .unwrap()
            .iter()
            .filter(|(e, _)| e == events::TOOL_RESULT)
            .map(|(_, p)| p.clone())
            .collect();
        assert!(results[0]["output"].as_str().unwrap().starts_with("hi\n"));
        assert_eq!(results[1]["ok"], false);
        assert_eq!(results[1]["output"], "denied by user");
        assert!(col.find(events::DONE).is_some());

        let cfg = config::load(&cfg_dir).unwrap();
        let ws = agent.store.get_session(&sid).unwrap().unwrap().workspace;
        assert_eq!(cfg.agent.allowed_commands_for(Path::new(&ws)), ["echo hi"]);
        assert!(!agent.approve(&sid, "nonexistent", Decision::Allow));
    }

    #[tokio::test]
    async fn cancel_denies_pending_approval_and_stops() {
        let (agent, _, col, sid, _) = setup(
            "ask",
            vec![vec![
                tc("c1", "run_shell", r#"{"command":"echo hi"}"#),
                LlmEvent::Done,
            ]],
        );
        let handle = tokio::spawn(agent.clone().run(col.clone(), sid.clone(), "go".into()));
        for _ in 0..100 {
            if col.find(events::TOOL_REQUEST).is_some() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        assert!(agent.is_running(&sid));
        agent.cancel(&sid);
        handle.await.unwrap();
        let names = col.names();
        assert_eq!(names.last().unwrap(), events::ERROR);
        assert!(col.find(events::ERROR).unwrap()["message"]
            .as_str()
            .unwrap()
            .contains("取消"));
        assert!(!agent.is_running(&sid));
    }

    #[test]
    fn safe_emit_and_strip_meta() {
        assert_eq!(safe_emit_len("hello"), 5);
        assert_eq!(safe_emit_len("hello<m"), 5);
        assert_eq!(safe_emit_len("hello \n<m"), 5);
        assert_eq!(safe_emit_len("hello<meta>{"), 5);
        assert_eq!(safe_emit_len("a<b"), 3);
        let (t, r) = strip_meta(
            "正文\n<meta>{\"emotion\":\"sad\",\"suggested_reaction\":\"sad_slump\"}</meta>",
        );
        assert_eq!(t, "正文");
        assert_eq!(r.unwrap().suggested_reaction, "sad_slump");
        let (t, r) = strip_meta("no meta here");
        assert_eq!(t, "no meta here");
        assert!(r.is_none());
    }
}
