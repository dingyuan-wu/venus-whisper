import { Button, Card, Input, Modal, Notification, Radio, Switch, Tabs, Tag, Tooltip } from "animal-island-ui";
import { useEffect, useMemo, useState, type ReactElement } from "react";
import { api, type ApprovalMode, type Config, type ModelEntry, type Persona, type Skill } from "../api";
import { lineDiff } from "../diff";
import { AssistantText } from "./Blocks";
import { useStore } from "../store";
import { ArrowLeft, Bulb, Check, Flower, Folder, Key, Plus, Trash, User } from "./icons";

type Group = "llm" | "agent" | "appearance" | "persona" | "skills" | "profile";

const GROUPS: { id: Group; label: string; tip: string }[] = [
  { id: "llm", label: "模型连接", tip: "可以配置多个 OpenAI 兼容端点（LiteLLM、Ollama、OpenAI），聊天时在右侧栏切换当前使用的模型。" },
  { id: "agent", label: "Agent 行为", tip: "控制工作目录、审批方式与单次任务的最大循环次数。新会话使用这里的 workspace。" },
  { id: "appearance", label: "外观", tip: "点击即时生效。主题是 ~/.venus-whisper/themes/ 下的 TOML 文件，复制一份改颜色就是新主题。" },
  {
    id: "skills",
    label: "Skills",
    tip: "Markdown 技能文档。启用后内容追加进 system prompt。内置 skill 随应用发布、只读，可同步到本地后修改；本地 skill 来自 ~/.venus-whisper/skills/。",
  },
  { id: "profile", label: "个人", tip: "聊天里你这一侧显示的名字和头像。头像放在 ~/.venus-whisper/user.png（也可 jpg / webp / gif），保存文件后自动刷新。" },
  {
    id: "persona",
    label: "Persona",
    tip: "作为每次对话的 system prompt 注入。内置人格随应用发布、只读，可同步到本地后修改；本地人格来自 ~/.venus-whisper/personas/ 下的 .md 文件或角色目录。",
  },
];

const MODE_OPTIONS: { value: ApprovalMode; label: ReactElement }[] = [
  { value: "ask", label: <span><strong>询问</strong><small>文件写入和 Shell 都需要确认</small></span> },
  { value: "auto-edit", label: <span><strong>自动编辑</strong><small>自动修改文件，Shell 仍需确认</small></span> },
  { value: "full-auto", label: <span><strong>全自动</strong><small>workspace 内自动执行所有操作</small></span> },
];

/** 面板标题：说明默认隐藏，悬浮灯泡显示。 */
function PaneTitle({ group }: { group: Group }) {
  const g = GROUPS.find((x) => x.id === group)!;
  return (
    <h2 className="pane-title">
      {g.label}
      <Tooltip title={<span className="section-tip">{g.tip}</span>} placement="bottom-start">
        <span className="tip-icon" tabIndex={0} aria-label="说明">
          <Bulb size={16} />
        </span>
      </Tooltip>
    </h2>
  );
}

/** 右侧编辑区：prompt / character_bible / 与内置对比 三个页签。只读内容渲染 Markdown，本地 md 可编辑并预览。 */
function PersonaEditor({
  persona,
  text,
  onText,
  isActive,
  onUse,
  onSync,
}: {
  persona?: Persona;
  text: string;
  onText: (t: string) => void;
  isActive: boolean;
  onUse: () => void;
  onSync: () => void;
}) {
  const [tab, setTab] = useState("prompt");
  const [preview, setPreview] = useState(false);
  const [bible, setBible] = useState<string | null>(null);
  const [origin, setOrigin] = useState<string | null>(null);
  // 只有当前使用中的本地 md 人格才可编辑，浏览态只读，避免改了没保存就切走
  const editable = !!persona && persona.source === "local" && persona.kind === "md" && isActive;

  useEffect(() => {
    setTab("prompt");
    setBible(null);
    setOrigin(null);
    if (!persona) return;
    if (persona.has_bible) api.getPersona(persona.id, "bible").then(setBible).catch(() => setBible(null));
    if (persona.origin) api.getPersona(persona.origin).then(setOrigin).catch(() => setOrigin(null));
  }, [persona?.id, persona?.has_bible, persona?.origin]);

  if (!persona) return null;
  const items = [
    {
      key: "prompt",
      label: persona.kind === "role" ? "prompt.md" : `${persona.name}.md`,
      children: (
        <div className="persona-doc">
          {editable && (
            <div className="doc-toolbar">
              <Button size="small" type={preview ? "text" : "default"} onClick={() => setPreview(false)}>编辑</Button>
              <Button size="small" type={preview ? "default" : "text"} onClick={() => setPreview(true)}>预览</Button>
            </div>
          )}
          {editable && !preview ? (
            <textarea className="persona" value={text} onChange={(e) => onText(e.target.value)} />
          ) : (
            <Card className="doc-card"><AssistantText text={text} /></Card>
          )}
        </div>
      ),
    },
    ...(persona.has_bible
      ? [{ key: "bible", label: "character_bible.md", children: <Card className="doc-card">{bible == null ? <small className="field-hint">加载中…</small> : <AssistantText text={bible} />}</Card> }]
      : []),
    ...(persona.origin
      ? [{
          key: "diff",
          label: persona.modified ? "与内置对比 · 有差异" : "与内置对比",
          children: origin == null ? <small className="field-hint">加载中…</small> : <DiffView oldText={origin} newText={text} />,
        }]
      : []),
  ];
  return (
    <div className="persona-editor">
      <div className="persona-meta">
        {isActive && <Tag size="small" color="app-teal" variant="solid">当前使用</Tag>}
        <Tag size="small" color={persona.source === "builtin" ? "app-yellow" : "default"}>
          {persona.source === "builtin" ? "内置" : "本地"}
        </Tag>
        <Tag size="small" color={persona.kind === "role" ? "purple" : "default"}>{persona.kind === "role" ? "角色目录" : "Markdown"}</Tag>
        {persona.source === "local" && (
          <span className="persona-path" title={persona.path}>{persona.path}</span>
        )}
        <span className="meta-actions">
          {persona.source === "builtin" && (
            <Button size="small" onClick={onSync}>
              {persona.synced ? "重新同步到本地" : "同步到本地"}
            </Button>
          )}
          {!isActive && (
            <Button size="small" type="primary" icon={<Check />} onClick={onUse}>
              设为当前人格
            </Button>
          )}
        </span>
      </div>
      <Tabs items={items} activeKey={tab} onChange={setTab} leafAnimation={false} />
      {persona.source === "builtin" && <small className="field-hint">内置人格只读。同步到本地后会得到一份可编辑副本，本地副本与内置项互不影响。</small>}
      {!isActive && persona.source === "local" && persona.kind === "md" && <small className="field-hint">浏览态只读，设为当前人格后可编辑。</small>}
      {persona.source === "local" && persona.kind === "role" && <small className="field-hint">角色目录由 skill 生成，请直接编辑其中的 prompt.md。</small>}
    </div>
  );
}

/** Skill 查看/编辑：内置只读渲染，本地可编辑并预览，有同名内置时提供对比。 */
function SkillEditor({ skill, enabled, onToggle, onSync }: { skill: Skill; enabled: boolean; onToggle: (on: boolean) => void; onSync: () => void }) {
  const [tab, setTab] = useState("doc");
  const [preview, setPreview] = useState(false);
  const [text, setText] = useState("");
  const [saved, setSaved] = useState("");
  const [origin, setOrigin] = useState<string | null>(null);
  const editable = skill.source === "local";

  useEffect(() => {
    setTab("doc");
    setOrigin(null);
    api.getSkill(skill.id).then((t) => { setText(t); setSaved(t); }).catch(() => setText(""));
    if (skill.origin) api.getSkill(skill.origin).then(setOrigin).catch(() => setOrigin(null));
  }, [skill.id, skill.origin]);

  const save = async () => {
    try {
      await api.setSkill(skill.id, text);
      setSaved(text);
      Notification.success({ message: "skill 已保存", duration: 3 });
    } catch (e) {
      Notification.error({ message: "保存失败", description: String(e) });
    }
  };

  const items = [
    {
      key: "doc",
      label: `${skill.name}.md`,
      children: (
        <div className="persona-doc">
          {editable && (
            <div className="doc-toolbar">
              <Button size="small" type={preview ? "text" : "default"} onClick={() => setPreview(false)}>编辑</Button>
              <Button size="small" type={preview ? "default" : "text"} onClick={() => setPreview(true)}>预览</Button>
              {text !== saved && (
                <Button size="small" type="primary" icon={<Check />} className="toolbar-right" onClick={save}>保存 skill</Button>
              )}
            </div>
          )}
          {editable && !preview ? (
            <textarea className="persona" value={text} onChange={(e) => setText(e.target.value)} />
          ) : (
            <Card className="doc-card"><AssistantText text={text} /></Card>
          )}
        </div>
      ),
    },
    ...(skill.origin
      ? [{
          key: "diff",
          label: skill.modified ? "与内置对比 · 有差异" : "与内置对比",
          children: origin == null ? <small className="field-hint">加载中…</small> : <DiffView oldText={origin} newText={text} />,
        }]
      : []),
  ];
  return (
    <div className="persona-editor">
      <div className="persona-meta">
        {enabled && <Tag size="small" color="app-teal" variant="solid">已启用</Tag>}
        <Tag size="small" color={skill.source === "builtin" ? "app-yellow" : "default"}>
          {skill.source === "builtin" ? "内置" : "本地"}
        </Tag>
        {skill.source === "local" && <span className="persona-path" title={skill.path}>{skill.path}</span>}
        <span className="meta-actions">
          {skill.source === "builtin" && (
            <Button size="small" onClick={onSync}>
              {skill.synced ? "重新同步到本地" : "同步到本地"}
            </Button>
          )}
          <span className="skill-switch">
            <span>{enabled ? "随每次对话注入" : "启用"}</span>
            <Switch size="small" checked={enabled} onChange={onToggle} />
          </span>
        </span>
      </div>
      <Tabs items={items} activeKey={tab} onChange={setTab} leafAnimation={false} />
    </div>
  );
}

/** 行级差异：红色为内置有本地没有，绿色为本地新增。 */
function DiffView({ oldText, newText }: { oldText: string; newText: string }) {
  const lines = lineDiff(oldText, newText);
  const changed = lines.filter((l) => l.kind !== "same").length;
  if (!changed) return <Card className="doc-card"><small className="field-hint">与内置完全一致。</small></Card>;
  return (
    <Card className="doc-card diff-card">
      <small className="field-hint">左侧标记：− 内置有、本地删掉；+ 本地新增。共 {changed} 行差异。</small>
      <pre className="diff-lines">
        {lines.map((l, i) => (
          <span key={i} className={`diff-${l.kind}`}>
            {l.kind === "add" ? "+ " : l.kind === "del" ? "− " : "  "}
            {l.text}
            {"\n"}
          </span>
        ))}
      </pre>
    </Card>
  );
}

export function SettingsView() {
  const { settings, persona, personas, skills, themes, userAvatar, saveSettings, setTheme, selectPersona, syncPersona, syncSkill, toggleSkill, setView } =
    useStore();
  const [confirmSync, setConfirmSync] = useState<string | null>(null);
  const [confirmSkillSync, setConfirmSkillSync] = useState<string | null>(null);
  const [modelId, setModelId] = useState<string | null>(null);
  const [skillId, setSkillId] = useState<string | null>(null);
  const [personaId, setPersonaId] = useState<string | null>(null);
  const [personaText, setPersonaText] = useState<string | null>(null);
  const [group, setGroup] = useState<Group>("llm");
  const [cfg, setCfg] = useState<Config | null>(settings);
  const [text, setText] = useState(persona);
  const [saving, setSaving] = useState(false);

  useEffect(() => setCfg(settings), [settings]);
  useEffect(() => setText(persona), [persona]);

  // 未保存项计数：主题与人格选择即时生效，不计入
  const dirty = useMemo(() => {
    if (!cfg || !settings) return 0;
    let n = 0;
    if (JSON.stringify(cfg.llm) !== JSON.stringify(settings.llm)) n++;
    if (cfg.agent.workspace !== settings.agent.workspace) n++;
    if (cfg.agent.approval_mode !== settings.agent.approval_mode) n++;
    if (cfg.agent.max_turns !== settings.agent.max_turns) n++;
    if (JSON.stringify(cfg.agent.allow) !== JSON.stringify(settings.agent.allow)) n++;
    if (cfg.user.name !== settings.user.name) n++;
    if (cfg.ui.debug !== settings.ui.debug) n++;
    if (text !== persona) n++;
    return n;
  }, [cfg, settings, text, persona]);

  if (!cfg) return null;
  const activePersona = personas.find((p) => p.id === cfg.agent.persona);
  // 右侧正在查看的人格：默认是当前使用的；点列表只切换查看，不改配置
  const viewId = personaId ?? cfg.agent.persona;
  const viewPersona = personas.find((p) => p.id === viewId) ?? activePersona;
  const viewIsActive = viewPersona?.id === cfg.agent.persona;
  const curModelId = modelId ?? cfg.llm.active;
  const curModel = cfg.llm.models.find((m) => m.id === curModelId) ?? cfg.llm.models[0];
  const patchModel = (id: string, patch: Partial<ModelEntry>) =>
    setCfg({ ...cfg, llm: { ...cfg.llm, models: cfg.llm.models.map((m) => (m.id === id ? { ...m, ...patch } : m)) } });
  const addModel = () => {
    let n = cfg.llm.models.length + 1;
    while (cfg.llm.models.some((m) => m.id === `model-${n}`)) n++;
    const id = `model-${n}`;
    const entry: ModelEntry = { id, name: `模型 ${n}`, base_url: "http://localhost:4000/v1", model: "", api_key: "", context_window: 128000 };
    setCfg({ ...cfg, llm: { ...cfg.llm, models: [...cfg.llm.models, entry], active: cfg.llm.active || id } });
    setModelId(id);
  };
  const removeModel = (id: string) => {
    const models = cfg.llm.models.filter((m) => m.id !== id);
    const active = cfg.llm.active === id ? models[0]?.id ?? "" : cfg.llm.active;
    setCfg({ ...cfg, llm: { models, active } });
    setModelId(active || null);
  };
  const curSkill = skills.find((s) => s.id === skillId) ?? skills[0];
  const agent = <K extends keyof Config["agent"]>(k: K, v: Config["agent"][K]) => setCfg({ ...cfg, agent: { ...cfg.agent, [k]: v } });
  const allowEntry = cfg.agent.allow.find((e) => e.workspace === cfg.agent.workspace);

  const save = async () => {
    setSaving(true);
    try {
      await saveSettings(cfg, text);
      Notification.success({ message: "设置已保存，下一条消息生效", duration: 3 });
    } catch (e) {
      Notification.error({ message: "保存失败", description: String(e) });
    } finally {
      setSaving(false);
    }
  };
  const discard = () => {
    setCfg(settings);
    setText(persona);
  };

  return (
    <main className="settings-shell">
      <header className="settings-header">
        <Button icon={<ArrowLeft />} onClick={() => setView("chat")}>
          返回对话
        </Button>
        <h1>设置</h1>
        <span className="config-path">~/.venus-whisper · 手改文件同样生效</span>
        <Button type="text" icon={<Folder />} onClick={() => api.openConfigDir()}>
          打开配置目录
        </Button>
      </header>

      <div className="settings-body">
        <nav className="settings-nav" aria-label="设置分组">
          {GROUPS.map((g) => (
            <Button key={g.id} block type={g.id === group ? "default" : "text"} className="nav-item" onClick={() => setGroup(g.id)}>
              {g.label}
            </Button>
          ))}
        </nav>

        <section className="settings-pane">
          <PaneTitle group={group} />

          {group === "llm" && (
            <div className="persona-layout">
              <div className="persona-list" role="listbox" aria-label="模型">
                <div className="persona-group">
                  <span className="persona-group-label">已配置</span>
                  {cfg.llm.models.map((m) => {
                    const isActive = m.id === cfg.llm.active;
                    const isCur = m.id === curModel?.id;
                    return (
                      <Button
                        key={m.id}
                        block
                        type={isActive ? "primary" : isCur ? "default" : "text"}
                        className="persona-item"
                        role="option"
                        aria-selected={isCur}
                        onClick={() => setModelId(m.id)}
                      >
                        <span className="persona-name">{m.name || m.id}</span>
                        {isActive && (
                          <Tag size="small" color="app-teal" variant="solid">
                            使用中
                          </Tag>
                        )}
                      </Button>
                    );
                  })}
                  <Button type="dashed" block icon={<Plus />} onClick={addModel}>
                    添加模型
                  </Button>
                </div>
              </div>
              {curModel ? (
                <div className="persona-editor">
                  <div className="persona-meta">
                    {curModel.id === cfg.llm.active && <Tag size="small" color="app-teal" variant="solid">当前使用</Tag>}
                    <span className="persona-path">id: {curModel.id}</span>
                    <span className="meta-actions">
                      <Button size="small" danger icon={<Trash />} onClick={() => removeModel(curModel.id)} disabled={cfg.llm.models.length <= 1}>
                        删除
                      </Button>
                      {curModel.id !== cfg.llm.active && (
                        <Button size="small" type="primary" icon={<Check />} onClick={() => setCfg({ ...cfg, llm: { ...cfg.llm, active: curModel.id } })}>
                          设为当前使用
                        </Button>
                      )}
                    </span>
                  </div>
                  <Card className="form-card">
                    <label className="field field-wide">
                      <span>显示名</span>
                      <Input value={curModel.name} onChange={(e) => patchModel(curModel.id, { name: e.target.value })} placeholder="本机 Gemma" />
                    </label>
                    <label className="field field-wide">
                      <span>API 端点</span>
                      <Input value={curModel.base_url} onChange={(e) => patchModel(curModel.id, { base_url: e.target.value })} placeholder="http://localhost:4000/v1" />
                    </label>
                    <label className="field">
                      <span>模型</span>
                      <Input value={curModel.model} onChange={(e) => patchModel(curModel.id, { model: e.target.value })} placeholder="gpt-4o" />
                    </label>
                    <label className="field">
                      <span>API Key</span>
                      <Input type="password" prefix={<Key />} value={curModel.api_key} onChange={(e) => patchModel(curModel.id, { api_key: e.target.value })} placeholder="sk-…" />
                    </label>
                    <label className="field">
                      <span>上下文窗口（tokens）</span>
                      <Input
                        type="number"
                        min={0}
                        value={String(curModel.context_window ?? 0)}
                        onChange={(e) => patchModel(curModel.id, { context_window: Math.max(0, Number(e.target.value) || 0) })}
                        placeholder="128000"
                      />
                      <small className="field-hint">右侧栏据此显示上下文占用百分比，0 表示未知。</small>
                    </label>
                  </Card>
                </div>
              ) : (
                <p className="field-hint">还没有模型，点"添加模型"。</p>
              )}
            </div>
          )}

          {group === "agent" && (
            <div className="pane-fields">
              <label className="field">
                <span>Workspace</span>
                <Input prefix={<Folder />} value={cfg.agent.workspace} onChange={(e) => agent("workspace", e.target.value)} placeholder="~/projects" />
                <small className="field-hint">工具只能在这个目录内读写与执行命令，对新会话生效。</small>
              </label>
              <div className="field">
                <span>审批模式</span>
                <Card>
                  <Radio direction="vertical" options={MODE_OPTIONS} value={cfg.agent.approval_mode} onChange={(v) => agent("approval_mode", v as ApprovalMode)} />
                </Card>
              </div>
              <div className="row-2">
                <label className="field">
                  <span>最大轮次</span>
                  <Input type="number" min={1} max={200} value={String(cfg.agent.max_turns)} onChange={(e) => agent("max_turns", Math.max(1, Number(e.target.value) || 1))} />
                </label>
                <div className="field">
                  <span>调试模式</span>
                  <span className="skill-switch">
                    <Switch checked={cfg.ui.debug} onChange={(v) => setCfg({ ...cfg, ui: { ...cfg.ui, debug: v } })} />
                    <span>{cfg.ui.debug ? "在对话中显示每次调用的 token 用量" : "关闭"}</span>
                  </span>
                </div>
                <div className="field">
                  <span>本目录已放行的命令</span>
                  {allowEntry?.commands.length ? (
                    <div className="allow-list">
                      {allowEntry.commands.map((c) => (
                        <Tag
                          key={c}
                          size="small"
                          color="app-teal"
                          closable
                          onClose={() =>
                            agent(
                              "allow",
                              cfg.agent.allow.map((e) => (e === allowEntry ? { ...e, commands: e.commands.filter((x) => x !== c) } : e)),
                            )
                          }
                        >
                          {c}
                        </Tag>
                      ))}
                    </div>
                  ) : (
                    <small className="field-hint">审批卡上点"总是允许"后会出现在这里。</small>
                  )}
                </div>
              </div>
            </div>
          )}

          {group === "skills" && (
            <div className="persona-layout">
              <div className="persona-list" role="listbox" aria-label="Skills">
                {(["builtin", "local"] as const).map((src) => {
                  const items = skills.filter((k) => k.source === src && !(src === "builtin" && k.identical));
                  if (!items.length && src === "builtin") return null;
                  return (
                    <div key={src} className="persona-group">
                      <span className="persona-group-label">{src === "builtin" ? "内置" : "本地"}</span>
                      {!items.length && <small className="field-hint">还没有本地 skill，从内置项同步一份或在目录中新建。</small>}
                      {items.map((k) => {
                        const enabled = cfg.agent.skills.includes(k.id);
                        const isCur = k.id === curSkill?.id;
                        return (
                          <Button
                            key={k.id}
                            block
                            type={enabled ? "primary" : isCur ? "default" : "text"}
                            className="persona-item"
                            role="option"
                            aria-selected={isCur}
                            onClick={() => setSkillId(k.id)}
                          >
                            <span className="persona-name">{k.title || k.name}</span>
                            {enabled && (
                              <Tag size="small" color="app-teal" variant="solid">
                                已启用
                              </Tag>
                            )}
                            {k.source === "local" && k.origin && (
                              <Tag size="small" color={k.modified ? "app-yellow" : "default"}>
                                {k.modified ? "已修改" : "同步"}
                              </Tag>
                            )}
                          </Button>
                        );
                      })}
                    </div>
                  );
                })}
              </div>
              {curSkill && (
                <SkillEditor
                  skill={curSkill}
                  enabled={cfg.agent.skills.includes(curSkill.id)}
                  onToggle={(on) => {
                    const cur = cfg.agent.skills.filter((x) => x !== curSkill.id);
                    setCfg({ ...cfg, agent: { ...cfg.agent, skills: on ? [...cur, curSkill.id] : cur } });
                    void toggleSkill(curSkill.id, on);
                  }}
                  onSync={() => (curSkill.synced ? setConfirmSkillSync(curSkill.id) : void syncSkill(curSkill.id, false))}
                />
              )}
            </div>
          )}

          {group === "profile" && (
            <div className="pane-fields">
              <label className="field">
                <span>显示名</span>
                <Input value={cfg.user.name} onChange={(e) => setCfg({ ...cfg, user: { ...cfg.user, name: e.target.value } })} placeholder="你" />
              </label>
              <div className="field">
                <span>头像</span>
                <Card className="profile-avatar-card">
                  <span className="avatar avatar-user profile-avatar">{userAvatar ? <img className="avatar-img" src={userAvatar} alt="" /> : <User />}</span>
                  <div>
                    <p className="field-hint">{userAvatar ? "已读取 ~/.venus-whisper/user.*" : "尚未设置。把图片命名为 user.png 放到配置目录即可，支持 png / jpg / webp / gif。"}</p>
                    <Button size="small" icon={<Folder />} onClick={() => api.openConfigDir()}>打开配置目录</Button>
                  </div>
                </Card>
              </div>
            </div>
          )}

          {group === "appearance" && (
            <div className="theme-grid" role="radiogroup" aria-label="主题">
              {!themes.length && <p className="field-hint">未找到主题文件，点右上角"打开配置目录"检查 themes/。</p>}
              {themes.map((t) => {
                const active = cfg.ui.theme === t.id;
                const swatch = [t.colors.accent, t.colors.side, t.colors.ink].filter(Boolean);
                const pick = () => {
                  setCfg({ ...cfg, ui: { ...cfg.ui, theme: t.id } });
                  void setTheme(t.id);
                };
                return (
                  <Card
                    key={t.id}
                    hoverable
                    role="radio"
                    aria-checked={active}
                    tabIndex={0}
                    className={`theme-card ${active ? "theme-active" : ""}`}
                    onClick={pick}
                    onKeyDown={(e) => {
                      if (e.key === "Enter" || e.key === " ") {
                        e.preventDefault();
                        pick();
                      }
                    }}
                  >
                    <span className="theme-swatch">
                      {swatch.map((c, i) => (
                        <i key={i} style={{ background: c }} />
                      ))}
                    </span>
                    <strong>{t.name}</strong>
                    <small>{t.description || t.id}</small>
                    {active && <Check className="theme-check" />}
                  </Card>
                );
              })}
            </div>
          )}

          {group === "persona" && (
            <div className="persona-layout">
              <div className="persona-list" role="listbox" aria-label="人格">
                {(["builtin", "local"] as const).map((src) => {
                  // 与本地副本完全一致的内置项不再展示，本地那份代表它
                  const items = personas.filter((p) => p.source === src && !(src === "builtin" && p.identical));
                  if (!items.length && src === "builtin") return null;
                  return (
                    <div key={src} className="persona-group">
                      <span className="persona-group-label">{src === "builtin" ? "内置" : "本地"}</span>
                      {!items.length && <small className="field-hint">还没有本地人格，从内置项同步一份或在目录中新建。</small>}
                      {items.map((p) => (
                        <Button
                          key={p.id}
                          block
                          type={p.id === cfg.agent.persona ? "primary" : p.id === viewPersona?.id ? "default" : "text"}
                          className="persona-item"
                          role="option"
                          aria-selected={p.id === viewPersona?.id}
                          onClick={() => {
                            setPersonaId(p.id);
                            setPersonaText(null);
                            api.getPersona(p.id).then(setPersonaText).catch(() => setPersonaText(""));
                          }}
                        >
                          <span className="persona-thumb">{p.avatar ? <img src={p.avatar} alt="" /> : <Flower />}</span>
                          <span className="persona-name">{p.name}</span>
                          {p.id === cfg.agent.persona && (
                            <Tag size="small" color="app-teal" variant="solid">
                              使用中
                            </Tag>
                          )}
                          {p.kind === "role" && (
                            <Tag size="small" color="purple">
                              角色
                            </Tag>
                          )}
                          {p.source === "local" && p.origin && (
                            <Tag size="small" color={p.modified ? "app-yellow" : "default"}>
                              {p.modified ? "已修改" : "同步"}
                            </Tag>
                          )}
                        </Button>
                      ))}
                    </div>
                  );
                })}
              </div>
              <PersonaEditor
                persona={viewPersona}
                text={viewIsActive ? text : personaText ?? ""}
                onText={viewIsActive ? setText : setPersonaText}
                isActive={viewIsActive}
                onUse={() => {
                  if (!viewPersona) return;
                  setCfg({ ...cfg, agent: { ...cfg.agent, persona: viewPersona.id } });
                  setPersonaId(null);
                  void selectPersona(viewPersona.id);
                }}
                onSync={() => viewPersona && (viewPersona.synced ? setConfirmSync(viewPersona.id) : void syncPersona(viewPersona.id, false))}
              />
            </div>
          )}

          <Modal
            open={!!confirmSync}
            typewriter={false}
            title="覆盖本地副本？"
            onClose={() => setConfirmSync(null)}
            footer={
              <>
                <Button onClick={() => setConfirmSync(null)}>取消</Button>
                <Button
                  type="primary"
                  danger
                  onClick={() => {
                    const id = confirmSync;
                    setConfirmSync(null);
                    if (id) void syncPersona(id, true);
                  }}
                >
                  覆盖
                </Button>
              </>
            }
          >
            本地已有同名人格，重新同步会用内置内容覆盖你的修改。
          </Modal>

          <Modal
            open={!!confirmSkillSync}
            typewriter={false}
            title="覆盖本地副本？"
            onClose={() => setConfirmSkillSync(null)}
            footer={
              <>
                <Button onClick={() => setConfirmSkillSync(null)}>取消</Button>
                <Button
                  type="primary"
                  danger
                  onClick={() => {
                    const id = confirmSkillSync;
                    setConfirmSkillSync(null);
                    if (id) void syncSkill(id, true);
                  }}
                >
                  覆盖
                </Button>
              </>
            }
          >
            本地已有同名 skill，重新同步会用内置内容覆盖你的修改。
          </Modal>

          <footer className={`pane-foot ${dirty ? "" : "pane-foot-clean"}`}>
            {dirty > 0 && <span className="dirty-note">有 {dirty} 项未保存</span>}
            {dirty > 0 && <Button onClick={discard}>放弃</Button>}
            <Button type="primary" icon={<Check />} loading={saving} disabled={!dirty} onClick={save}>
              保存设置
            </Button>
          </footer>
        </section>
      </div>
    </main>
  );
}
