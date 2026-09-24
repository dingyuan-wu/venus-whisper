// Rust 命令的类型化封装。命令签名见 src-tauri/src/commands.rs。
import { invoke } from "@tauri-apps/api/core";
import type { AgentUsage, ApprovalDecision } from "./events";

export type ApprovalMode = "ask" | "auto-edit" | "full-auto";
/** ~/.venus-whisper/themes/<id>.toml，见 src-tauri/src/theme.rs */
export interface Theme {
  id: string;
  name: string;
  description: string;
  colors: Record<string, string>;
  code: Record<string, string>;
}

export interface ModelEntry {
  id: string;
  name: string;
  base_url: string;
  model: string;
  api_key: string;
  /** 上下文窗口 token 数，0 = 未知 */
  context_window: number;
}

export interface Skill {
  id: string;
  name: string;
  title: string;
  source: "builtin" | "local";
  path: string;
  synced: boolean;
  identical: boolean;
  origin?: string | null;
  modified: boolean;
}

export interface Config {
  llm: { active: string; models: ModelEntry[] };
  agent: {
    workspace: string;
    approval_mode: ApprovalMode;
    max_turns: number;
    persona: string;
    skills: string[];
    allow: { workspace: string; commands: string[] }[];
  };
  ui: { theme: string; debug: boolean };
  user: { name: string };
}

export interface Persona {
  id: string;
  name: string;
  description: string;
  kind: "md" | "role";
  /** builtin：编译进应用，只读；local：~/.venus-whisper/personas/ */
  source: "builtin" | "local";
  path: string;
  has_bible: boolean;
  /** data URL */
  avatar?: string | null;
  /** 内置项：本地是否已有同名副本 */
  synced: boolean;
  /** 内置项：本地副本与内置完全一致 */
  identical: boolean;
  /** 本地项：对应的内置 id，用于 diff */
  origin?: string | null;
  /** 本地项：与内置不一致 */
  modified: boolean;
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
  getUserAvatar: () => invoke<string | null>("get_user_avatar"),
  estimateContext: (sessionId: string) => invoke<AgentUsage>("estimate_context", { sessionId }),
  listSkills: () => invoke<Skill[]>("list_skills"),
  getSkill: (id: string) => invoke<string>("get_skill", { id }),
  setSkill: (id: string, text: string) => invoke<void>("set_skill", { id, text }),
  syncSkill: (id: string, overwrite: boolean) => invoke<string>("sync_skill", { id, overwrite }),
  listPersonas: () => invoke<Persona[]>("list_personas"),
  /** 把内置人格复制到本地，返回本地 id */
  syncPersona: (id: string, overwrite: boolean) => invoke<string>("sync_persona", { id, overwrite }),
  /** file 可选 "bible" 读 character_bible.md */
  getPersona: (id?: string, file?: "prompt" | "bible") => invoke<string>("get_persona", { id, file }),
  setPersona: (text: string, id?: string) => invoke<void>("set_persona", { id, text }),
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
