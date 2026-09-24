import { listen } from "@tauri-apps/api/event";
import { Notification } from "animal-island-ui";
import { create } from "zustand";
import { api, type Config, type Message, type Persona, type Session, type Skill, type Theme } from "./api";
import {
  EVENTS,
  type AgentDelta,
  type AgentDone,
  type AgentError,
  type AgentReaction,
  type AgentToolRequest,
  type AgentToolResult,
  type AgentUsage,
  type ApprovalDecision,
} from "./events";

export type ToolStatus = "pending" | "running" | "done" | "failed" | "denied";

export interface ToolBlock {
  kind: "tool";
  call_id: string;
  name: string;
  args: Record<string, unknown>;
  status: ToolStatus;
  output?: string;
  duration_ms?: number;
}
export interface TextSegment {
  kind: "text";
  text: string;
}
export type Segment = TextSegment | ToolBlock;

/** 一次运行中正在流式生成的 assistant 回合 */
export interface Live {
  running: boolean;
  segments: Segment[];
}

interface State {
  view: "chat" | "settings";
  sidebarOpen: boolean;
  sessions: Session[];
  currentId: string | null;
  messages: Record<string, Message[]>;
  live: Record<string, Live>;
  /** 每会话最近一次运行的逐次调用用量（debug 模式展示） */
  usage: Record<string, AgentUsage[]>;
  /** 每会话当前上下文占用（最后一次调用的 prompt+completion，或 estimate_context 的估算） */
  context: Record<string, AgentUsage>;
  settings: Config | null;
  /** 当前选中人格的 prompt 文本 */
  persona: string;
  personas: Persona[];
  skills: Skill[];
  userAvatar: string | null;
  themes: Theme[];
  lastReaction: AgentReaction | null;

  setView: (v: State["view"]) => void;
  setSidebarOpen: (v: boolean) => void;
  loadSessions: () => Promise<void>;
  selectSession: (id: string) => Promise<void>;
  newSession: () => Promise<void>;
  deleteSession: (id: string) => Promise<void>;
  send: (text: string) => Promise<void>;
  approve: (callId: string, decision: ApprovalDecision) => Promise<void>;
  cancel: () => Promise<void>;
  loadSettings: () => Promise<void>;
  saveSettings: (cfg: Config, persona: string) => Promise<void>;
  /** 切换人格：写配置，并加载该人格的 prompt 文本 */
  selectPersona: (id: string) => Promise<void>;
  /** 内置 → 本地副本，成功后刷新列表并选中本地副本 */
  syncPersona: (id: string, overwrite: boolean) => Promise<void>;
  /** 切换当前模型，立即落盘 */
  setActiveModel: (id: string) => Promise<void>;
  /** 内置 skill → 本地副本，成功后刷新列表 */
  syncSkill: (id: string, overwrite: boolean) => Promise<void>;
  /** 启用/停用某个 skill，立即落盘 */
  toggleSkill: (id: string, on: boolean) => Promise<void>;
  setTheme: (id: string) => Promise<void>;
}

/** 把主题里的颜色写成 CSS 变量；未提供的键清掉，回落到 themes.css 的默认值。 */
function applyTheme(theme: Theme | undefined) {
  const root = document.documentElement;
  for (const name of Array.from(root.style)) {
    if (name.startsWith("--vw-") || name.startsWith("--code-")) root.style.removeProperty(name);
  }
  if (!theme) return;
  root.dataset.theme = theme.id;
  for (const [k, v] of Object.entries(theme.colors)) root.style.setProperty(`--vw-${k}`, v);
  for (const [k, v] of Object.entries(theme.code)) root.style.setProperty(`--code-${k}`, v);
}

const emptyLive = (): Live => ({ running: true, segments: [] });

export const useStore = create<State>((set, get) => ({
  view: "chat",
  sidebarOpen: false,
  sessions: [],
  currentId: null,
  messages: {},
  live: {},
  usage: {},
  context: {},
  settings: null,
  persona: "",
  personas: [],
  skills: [],
  userAvatar: null,
  themes: [],
  lastReaction: null,

  setView: (view) => set({ view }),
  setSidebarOpen: (sidebarOpen) => set({ sidebarOpen }),

  loadSessions: async () => {
    const sessions = await api.listSessions();
    set({ sessions });
    if (!get().currentId && sessions[0]) await get().selectSession(sessions[0].id);
  },

  selectSession: async (id) => {
    set({ currentId: id, sidebarOpen: false, view: "chat" });
    const msgs = await api.getMessages(id);
    set((s) => ({ messages: { ...s.messages, [id]: msgs } }));
    api.estimateContext(id).then((u) => set((s) => (s.context[id]?.call ? s : { context: { ...s.context, [id]: u } }))).catch(() => {});
  },

  newSession: async () => {
    const s = await api.createSession();
    set((st) => ({ sessions: [s, ...st.sessions], messages: { ...st.messages, [s.id]: [] } }));
    await get().selectSession(s.id);
  },

  deleteSession: async (id) => {
    await api.deleteSession(id);
    const sessions = get().sessions.filter((s) => s.id !== id);
    set({ sessions, currentId: null });
    if (sessions[0]) await get().selectSession(sessions[0].id);
  },

  send: async (text) => {
    let id = get().currentId;
    if (!id) {
      await get().newSession();
      id = get().currentId!;
    }
    const session = get().sessions.find((s) => s.id === id);
    // 乐观插入用户消息；agent:done 后会用落库数据整体替换
    const optimistic: Message = {
      id: `local-${Date.now()}`,
      session_id: id,
      role: "user",
      content: { role: "user", content: text },
      created_at: Date.now(),
    };
    set((s) => ({
      messages: { ...s.messages, [id!]: [...(s.messages[id!] ?? []), optimistic] },
      live: { ...s.live, [id!]: emptyLive() },
      usage: { ...s.usage, [id!]: [] },
    }));
    if (session && session.title === "新会话") {
      const title = text.replace(/\s+/g, " ").slice(0, 24);
      api.renameSession(id, title).catch(() => {});
      set((s) => ({ sessions: s.sessions.map((x) => (x.id === id ? { ...x, title } : x)) }));
    }
    await api.sendMessage(id, text);
  },

  approve: async (callId, decision) => {
    const id = get().currentId;
    if (!id) return;
    await api.approveTool(id, callId, decision);
    patchTool(set, id, callId, (t) => ({ ...t, status: decision === "deny" ? "denied" : "running" }));
  },

  cancel: async () => {
    const id = get().currentId;
    if (id) await api.cancelRun(id);
  },

  loadSettings: async () => {
    const [settings, themes, personas, skills, userAvatar] = await Promise.all([
      api.getSettings(),
      api.listThemes(),
      api.listPersonas(),
      api.listSkills().catch(() => []),
      api.getUserAvatar().catch(() => null),
    ]);
    const persona = await api.getPersona(settings.agent.persona).catch(() => api.getPersona());
    applyTheme(themes.find((t) => t.id === settings.ui?.theme) ?? themes[0]);
    set({ settings, persona, personas, skills, themes, userAvatar });
  },

  saveSettings: async (cfg, persona) => {
    await api.setSettings(cfg);
    const active = get().personas.find((p) => p.id === cfg.agent.persona);
    if (persona !== get().persona && active?.kind === "md" && active.source === "local") await api.setPersona(persona, cfg.agent.persona);
    applyTheme(get().themes.find((t) => t.id === cfg.ui.theme));
    set({ settings: cfg, persona });
  },

  selectPersona: async (id) => {
    const cfg = get().settings;
    if (!cfg) return;
    const next = { ...cfg, agent: { ...cfg.agent, persona: id } };
    const persona = await api.getPersona(id);
    set({ settings: next, persona });
    await api.setSettings(next);
  },

  syncPersona: async (id, overwrite) => {
    const localId = await api.syncPersona(id, overwrite);
    const personas = await api.listPersonas();
    set({ personas });
    await get().selectPersona(localId);
  },

  setActiveModel: async (id) => {
    const cfg = get().settings;
    if (!cfg) return;
    const next = { ...cfg, llm: { ...cfg.llm, active: id } };
    set({ settings: next });
    await api.setSettings(next);
  },

  syncSkill: async (id, overwrite) => {
    await api.syncSkill(id, overwrite);
    set({ skills: await api.listSkills() });
  },

  toggleSkill: async (id, on) => {
    const cfg = get().settings;
    if (!cfg) return;
    const cur = cfg.agent.skills.filter((s) => s !== id);
    const next = { ...cfg, agent: { ...cfg.agent, skills: on ? [...cur, id] : cur } };
    set({ settings: next });
    await api.setSettings(next);
  },

  /** 切主题即时生效并落盘，不等"保存设置"。 */
  setTheme: async (id) => {
    const cfg = get().settings;
    if (!cfg) return;
    const next = { ...cfg, ui: { ...cfg.ui, theme: id } };
    applyTheme(get().themes.find((t) => t.id === id));
    set({ settings: next });
    await api.setSettings(next);
  },
}));

type Set = (fn: (s: State) => Partial<State>) => void;

function patchLive(set: Set, sid: string, fn: (live: Live) => Live) {
  set((s) => ({ live: { ...s.live, [sid]: fn(s.live[sid] ?? emptyLive()) } }));
}

function patchTool(set: Set, sid: string, callId: string, fn: (t: ToolBlock) => ToolBlock) {
  patchLive(set, sid, (live) => ({
    ...live,
    segments: live.segments.map((seg) => (seg.kind === "tool" && seg.call_id === callId ? fn(seg) : seg)),
  }));
}

/** 订阅 Rust 事件，App 挂载时调用一次。 */
export function bindEvents() {
  const set = useStore.setState as Set;
  const unlisteners = [
    listen<AgentDelta>(EVENTS.delta, ({ payload }) =>
      patchLive(set, payload.session_id, (live) => {
        const last = live.segments[live.segments.length - 1];
        const segments =
          last?.kind === "text"
            ? [...live.segments.slice(0, -1), { kind: "text" as const, text: last.text + payload.text }]
            : [...live.segments, { kind: "text" as const, text: payload.text }];
        return { ...live, segments };
      }),
    ),
    listen<AgentToolRequest>(EVENTS.toolRequest, ({ payload }) =>
      patchLive(set, payload.session_id, (live) =>
        live.segments.some((s) => s.kind === "tool" && s.call_id === payload.call_id)
          ? live
          : {
              ...live,
              segments: [
                ...live.segments,
                { kind: "tool", call_id: payload.call_id, name: payload.name, args: asObj(payload.args), status: "pending" },
              ],
            },
      ),
    ),
    listen<AgentToolResult>(EVENTS.toolResult, ({ payload }) =>
      patchLive(set, payload.session_id, (live) => {
        const status: ToolStatus = payload.output === "denied by user" ? "denied" : payload.ok ? "done" : "failed";
        const done: ToolBlock = {
          kind: "tool",
          call_id: payload.call_id,
          name: payload.name,
          args: asObj(payload.args),
          status,
          output: payload.output,
          duration_ms: payload.duration_ms,
        };
        const exists = live.segments.some((s) => s.kind === "tool" && s.call_id === payload.call_id);
        return {
          ...live,
          segments: exists
            ? live.segments.map((s) => (s.kind === "tool" && s.call_id === payload.call_id ? done : s))
            : [...live.segments, done],
        };
      }),
    ),
    listen<AgentDone>(EVENTS.done, async ({ payload }) => {
      const msgs = await api.getMessages(payload.session_id);
      set((s) => {
        const live = { ...s.live };
        delete live[payload.session_id];
        return { messages: { ...s.messages, [payload.session_id]: msgs }, live };
      });
    }),
    listen<AgentError>(EVENTS.error, async ({ payload }) => {
      Notification.error({ message: "运行中断", description: payload.message });
      const msgs = await api.getMessages(payload.session_id).catch(() => null);
      set((s) => {
        const live = { ...s.live };
        delete live[payload.session_id];
        return { live, ...(msgs ? { messages: { ...s.messages, [payload.session_id]: msgs } } : {}) };
      });
    }),
    listen<AgentReaction>(EVENTS.reaction, ({ payload }) => set(() => ({ lastReaction: payload }))),
    listen<AgentUsage>(EVENTS.usage, ({ payload }) =>
      set((s) => ({
        usage: { ...s.usage, [payload.session_id]: [...(s.usage[payload.session_id] ?? []), payload] },
        context: { ...s.context, [payload.session_id]: payload },
      })),
    ),
    listen(EVENTS.configChanged, () => useStore.getState().loadSettings()),
    listen(EVENTS.openSettings, () => set(() => ({ view: "settings" }))),
  ];
  return () => {
    unlisteners.forEach((p) => p.then((un) => un()));
  };
}

function asObj(v: unknown): Record<string, unknown> {
  return v && typeof v === "object" ? (v as Record<string, unknown>) : {};
}
