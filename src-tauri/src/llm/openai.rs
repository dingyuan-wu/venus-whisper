//! OpenAI Chat Completions 流式客户端（SSE）。LiteLLM / Ollama / OpenAI 通用。

use super::{ChatRequest, EventStream, LlmClient, LlmEvent, ToolCall};
use crate::error::{Error, Result};
use async_trait::async_trait;
use futures_util::StreamExt;
use serde::Deserialize;
use std::collections::BTreeMap;

pub struct OpenAiClient {
    http: reqwest::Client,
    base_url: String,
    api_key: String,
}

impl OpenAiClient {
    pub fn new(base_url: &str, api_key: &str) -> Self {
        Self {
            http: reqwest::Client::new(),
            base_url: base_url.trim_end_matches('/').to_string(),
            api_key: api_key.to_string(),
        }
    }
}

#[async_trait]
impl LlmClient for OpenAiClient {
    async fn chat(&self, req: ChatRequest) -> Result<EventStream> {
        let mut body = serde_json::json!({
            "model": req.model,
            "messages": req.messages,
            "stream": true,
        });
        if !req.tools.is_empty() {
            body["tools"] = serde_json::Value::Array(req.tools);
        }
        let mut http = self
            .http
            .post(format!("{}/chat/completions", self.base_url))
            .json(&body);
        if !self.api_key.is_empty() {
            http = http.bearer_auth(&self.api_key);
        }
        let resp = http.send().await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(Error::Msg(format!(
                "LLM HTTP {status}: {}",
                text.chars().take(500).collect::<String>()
            )));
        }

        let (tx, rx) = tokio::sync::mpsc::channel::<Result<LlmEvent>>(64);
        let mut bytes = resp.bytes_stream();
        tokio::spawn(async move {
            let mut asm = Assembler::default();
            let mut buf = String::new();
            while let Some(chunk) = bytes.next().await {
                let chunk = match chunk {
                    Ok(c) => c,
                    Err(e) => {
                        let _ = tx.send(Err(e.into())).await;
                        return;
                    }
                };
                buf.push_str(&String::from_utf8_lossy(&chunk));
                while let Some(pos) = buf.find('\n') {
                    let line = buf[..pos].trim_end_matches('\r').to_string();
                    buf.drain(..=pos);
                    for ev in asm.feed_line(&line) {
                        if tx.send(Ok(ev)).await.is_err() {
                            return;
                        }
                    }
                }
            }
            for ev in asm.finish() {
                if tx.send(Ok(ev)).await.is_err() {
                    return;
                }
            }
        });
        Ok(
            futures_util::stream::unfold(rx, |mut rx| async { rx.recv().await.map(|e| (e, rx)) })
                .boxed(),
        )
    }
}

#[derive(Deserialize)]
struct Chunk {
    #[serde(default)]
    choices: Vec<Choice>,
}
#[derive(Deserialize)]
struct Choice {
    #[serde(default)]
    delta: Delta,
    #[serde(default)]
    finish_reason: Option<String>,
}
#[derive(Deserialize, Default)]
struct Delta {
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    tool_calls: Vec<DeltaToolCall>,
}
#[derive(Deserialize)]
struct DeltaToolCall {
    #[serde(default)]
    index: usize,
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    function: DeltaFunction,
}
#[derive(Deserialize, Default)]
struct DeltaFunction {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    arguments: Option<String>,
}

/// 把 SSE 行拼成事件。tool_calls 的 arguments 分片按 index 累积，收到 finish_reason 或流结束时整体吐出。
#[derive(Default)]
pub struct Assembler {
    pending: BTreeMap<usize, ToolCall>,
    flushed: bool,
}

impl Assembler {
    pub fn feed_line(&mut self, line: &str) -> Vec<LlmEvent> {
        let Some(data) = line.strip_prefix("data:") else {
            return vec![];
        };
        let data = data.trim();
        if data.is_empty() || data == "[DONE]" {
            return vec![];
        }
        let Ok(chunk) = serde_json::from_str::<Chunk>(data) else {
            return vec![];
        };
        let mut out = vec![];
        for choice in chunk.choices {
            if let Some(text) = choice.delta.content.filter(|t| !t.is_empty()) {
                out.push(LlmEvent::Delta(text));
            }
            for tc in choice.delta.tool_calls {
                let entry = self.pending.entry(tc.index).or_insert_with(|| ToolCall {
                    id: String::new(),
                    name: String::new(),
                    arguments: String::new(),
                });
                if let Some(id) = tc.id {
                    entry.id = id;
                }
                if let Some(name) = tc.function.name {
                    entry.name.push_str(&name);
                }
                if let Some(args) = tc.function.arguments {
                    entry.arguments.push_str(&args);
                }
            }
            if choice.finish_reason.is_some() {
                out.extend(self.flush());
            }
        }
        out
    }

    pub fn finish(&mut self) -> Vec<LlmEvent> {
        let mut out = self.flush();
        out.push(LlmEvent::Done);
        out
    }

    fn flush(&mut self) -> Vec<LlmEvent> {
        if self.flushed {
            return vec![];
        }
        self.flushed = true;
        std::mem::take(&mut self.pending)
            .into_values()
            .map(|mut tc| {
                if tc.id.is_empty() {
                    tc.id = format!("call_{}", uuid::Uuid::new_v4().simple());
                }
                LlmEvent::ToolCall(tc)
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(sse: &str) -> Vec<LlmEvent> {
        let mut asm = Assembler::default();
        let mut out: Vec<LlmEvent> = sse.lines().flat_map(|l| asm.feed_line(l)).collect();
        out.extend(asm.finish());
        out
    }

    #[test]
    fn assembles_fragmented_tool_call() {
        let sse = r#"data: {"choices":[{"delta":{"role":"assistant","content":null,"tool_calls":[{"index":0,"id":"call_abc","type":"function","function":{"name":"read_file","arguments":""}}]},"finish_reason":null}]}

data: {"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"{\"pa"}}]},"finish_reason":null}]}

data: {"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"th\":\"a."}}]},"finish_reason":null}]}

data: {"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"txt\"}"}}]},"finish_reason":null}]}

data: {"choices":[{"delta":{},"finish_reason":"tool_calls"}]}

data: [DONE]
"#;
        let ev = run(sse);
        assert_eq!(
            ev,
            vec![
                LlmEvent::ToolCall(ToolCall {
                    id: "call_abc".into(),
                    name: "read_file".into(),
                    arguments: r#"{"path":"a.txt"}"#.into()
                }),
                LlmEvent::Done
            ]
        );
    }

    #[test]
    fn streams_text_deltas_in_order() {
        let sse = r#"data: {"choices":[{"delta":{"role":"assistant","content":""},"finish_reason":null}]}
data: {"choices":[{"delta":{"content":"你"},"finish_reason":null}]}
data: {"choices":[{"delta":{"content":"好"},"finish_reason":null}]}
: keep-alive comment
data: {"choices":[{"delta":{},"finish_reason":"stop"}]}
data: [DONE]
"#;
        assert_eq!(
            run(sse),
            vec![
                LlmEvent::Delta("你".into()),
                LlmEvent::Delta("好".into()),
                LlmEvent::Done
            ]
        );
    }

    #[test]
    fn two_parallel_tool_calls_keep_index_order() {
        let sse = r#"data: {"choices":[{"delta":{"tool_calls":[{"index":1,"id":"b","function":{"name":"list_dir","arguments":"{}"}}]},"finish_reason":null}]}
data: {"choices":[{"delta":{"tool_calls":[{"index":0,"id":"a","function":{"name":"read_file","arguments":"{}"}}]},"finish_reason":null}]}
"#;
        let ev = run(sse);
        assert_eq!(ev.len(), 3);
        assert!(matches!(&ev[0], LlmEvent::ToolCall(t) if t.id == "a"));
        assert!(matches!(&ev[1], LlmEvent::ToolCall(t) if t.id == "b"));
        assert_eq!(ev[2], LlmEvent::Done);
    }
}
