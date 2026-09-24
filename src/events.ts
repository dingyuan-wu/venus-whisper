// 与 src-tauri/src/events.rs 手工对照；Rust 侧有契约测试保证字段一致。

export const EVENTS = {
  delta: "agent:delta",
  toolRequest: "agent:tool_request",
  toolResult: "agent:tool_result",
  done: "agent:done",
  error: "agent:error",
  reaction: "agent:reaction",
  configChanged: "config:changed",
  openSettings: "ui:open_settings",
} as const;

export interface AgentDelta {
  session_id: string;
  text: string;
}

export interface AgentToolRequest {
  session_id: string;
  call_id: string;
  name: string;
  args: unknown;
}

export interface AgentToolResult {
  session_id: string;
  call_id: string;
  name: string;
  args: unknown;
  output: string;
  ok: boolean;
  duration_ms: number;
}

export interface AgentDone {
  session_id: string;
  message_id: string;
}

export interface AgentError {
  session_id: string;
  message: string;
}

export interface AgentReaction {
  session_id: string;
  emotion: string;
  intent: string;
  suggested_reaction: string;
  confidence: number;
}

export type ApprovalDecision = "allow" | "deny" | "always";
