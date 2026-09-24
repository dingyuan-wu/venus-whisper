//! LLM 客户端抽象。只有 OpenAI 兼容一种协议；trait 的存在是为了让循环能注入假客户端测试。

pub mod openai;

use crate::error::Result;
use async_trait::async_trait;
use futures_util::stream::BoxStream;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    /// 原始 JSON 字符串，由工具层再解析
    pub arguments: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum LlmEvent {
    Delta(String),
    ToolCall(ToolCall),
    /// 服务端返回的用量（需 stream_options.include_usage，多数 OpenAI 兼容端支持）
    Usage {
        prompt_tokens: u32,
        completion_tokens: u32,
    },
    Done,
}

#[derive(Debug, Clone, Serialize)]
pub struct ChatRequest {
    pub model: String,
    /// OpenAI 消息格式
    pub messages: Vec<serde_json::Value>,
    /// OpenAI tools 定义；为空则不发送 tools 字段
    pub tools: Vec<serde_json::Value>,
}

pub type EventStream = BoxStream<'static, Result<LlmEvent>>;

#[async_trait]
pub trait LlmClient: Send + Sync {
    async fn chat(&self, req: ChatRequest) -> Result<EventStream>;
}
