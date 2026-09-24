// Rust 命令的类型化封装。命令签名见 src-tauri/src/commands.rs。
import { invoke } from "@tauri-apps/api/core";
import type { ApprovalDecision } from "./events";

export type ApprovalMode = "ask" | "auto-edit" | "full-auto";
/** ~/.venus-whisper/themes/<id>.toml，见 src-tauri/src/theme.rs */
export interface Theme {
  id: string;
  name: string;
  description: string;
  colors: Record<string, string>;
  code: Record<string, string>;
}

export interface Config {
  llm: { base_url: string; model: string; api_key: string };
  agent: {
    workspace: string;
    approval_mode: ApprovalMode;
    max_turns: number;
    allow: { workspace: string; commands: string[] }[];
  };
  ui: { theme: string };
}

export interface Session {
  id: string;
  title: string;
  workspace: string;
  created_at: number;
}

/** content 是 OpenAI 消息格式的原始 JSON */
export interface Message {
  id: string;
  session_id: string;
  role: "user" | "assistant" | "tool" | "system";
  content: Record<string, unknown>;
  created_at: number;
}

export const api = {
  getSettings: () => invoke<Config>("get_settings"),
  setSettings: (cfg: Config) => invoke<void>("set_settings", { cfg }),
  getPersona: () => invoke<string>("get_persona"),
  setPersona: (text: string) => invoke<void>("set_persona", { text }),
  openConfigDir: () => invoke<void>("open_config_dir"),
  listThemes: () => invoke<Theme[]>("list_themes"),

  listSessions: () => invoke<Session[]>("list_sessions"),
  createSession: (title?: string) => invoke<Session>("create_session", { title }),
  renameSession: (id: string, title: string) => invoke<void>("rename_session", { id, title }),
  deleteSession: (id: string) => invoke<void>("delete_session", { id }),
  getMessages: (sessionId: string) => invoke<Message[]>("get_messages", { sessionId }),

  sendMessage: (sessionId: string, text: string) => invoke<void>("send_message", { sessionId, text }),
  approveTool: (sessionId: string, callId: string, decision: ApprovalDecision) =>
    invoke<boolean>("approve_tool", { sessionId, callId, decision }),
  cancelRun: (sessionId: string) => invoke<void>("cancel_run", { sessionId }),
};
