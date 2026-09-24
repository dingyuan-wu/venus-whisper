import { Button, Card, Input, Notification, Radio } from "animal-island-ui";
import { useEffect, useState, type ReactElement } from "react";
import { api, type ApprovalMode, type Config } from "../api";
import { useStore } from "../store";
import { ArrowLeft, Check, Flower, Folder, Globe, Key } from "./icons";

const MODE_OPTIONS: { value: ApprovalMode; label: ReactElement }[] = [
  { value: "ask", label: <span><strong>询问</strong><small>文件写入和 Shell 都需要确认</small></span> },
  { value: "auto-edit", label: <span><strong>自动编辑</strong><small>自动修改文件，Shell 仍需确认</small></span> },
  { value: "full-auto", label: <span><strong>全自动</strong><small>workspace 内自动执行所有操作</small></span> },
];

export function SettingsView() {
  const { settings, persona, themes, saveSettings, setTheme, setView } = useStore();
  const [cfg, setCfg] = useState<Config | null>(settings);
  const [text, setText] = useState(persona);
  const [saving, setSaving] = useState(false);

  useEffect(() => setCfg(settings), [settings]);
  useEffect(() => setText(persona), [persona]);

  if (!cfg) return null;
  const llm = (k: keyof Config["llm"], v: string) => setCfg({ ...cfg, llm: { ...cfg.llm, [k]: v } });
  const agent = <K extends keyof Config["agent"]>(k: K, v: Config["agent"][K]) => setCfg({ ...cfg, agent: { ...cfg.agent, [k]: v } });

  const save = async () => {
    setSaving(true);
    try {
      await saveSettings(cfg, text);
      Notification.success("设置已保存，下一条消息生效");
    } catch (e) {
      Notification.error({ message: "保存失败", description: String(e) });
    } finally {
      setSaving(false);
    }
  };

  return (
    <main className="settings-shell">
      <header className="settings-header">
        <Button icon={<ArrowLeft />} onClick={() => setView("chat")}>
          返回对话
        </Button>
        <div>
          <h1>设置</h1>
          <p>配置会在每次新任务开始时重新读取，也可以直接编辑 ~/.venus-whisper 下的文件。</p>
        </div>
        <Button type="text" icon={<Folder />} onClick={() => api.openConfigDir()}>
          打开配置目录
        </Button>
      </header>

      <div className="settings-form">
        <section className="settings-section">
          <div className="section-heading">
            <span className="section-icon"><Globe size={18} /></span>
            <div>
              <h2>模型连接</h2>
              <p>OpenAI 兼容端点，适用于 LiteLLM、Ollama 与 OpenAI。</p>
            </div>
          </div>
          <Card className="form-card">
            <label className="field field-wide">
              <span>API 端点</span>
              <Input value={cfg.llm.base_url} onChange={(e) => llm("base_url", e.target.value)} placeholder="http://localhost:4000/v1" />
            </label>
            <label className="field">
              <span>模型</span>
              <Input value={cfg.llm.model} onChange={(e) => llm("model", e.target.value)} placeholder="gpt-4o" />
            </label>
            <label className="field">
              <span>API Key</span>
              <Input type="password" prefix={<Key />} value={cfg.llm.api_key} onChange={(e) => llm("api_key", e.target.value)} placeholder="sk-…" />
            </label>
          </Card>
        </section>

        <section className="settings-section">
          <div className="section-heading">
            <span className="section-icon section-icon-amber"><Folder size={18} /></span>
            <div>
              <h2>Agent 行为</h2>
              <p>控制工作目录、审批方式与单次任务的最大循环次数。新会话使用这里的 workspace。</p>
            </div>
          </div>
          <Card className="form-card">
            <label className="field field-wide">
              <span>Workspace</span>
              <Input prefix={<Folder />} value={cfg.agent.workspace} onChange={(e) => agent("workspace", e.target.value)} placeholder="~/projects" />
            </label>
            <div className="field field-wide">
              <span>审批模式</span>
              <Radio
                direction="vertical"
                options={MODE_OPTIONS}
                value={cfg.agent.approval_mode}
                onChange={(v) => agent("approval_mode", v as ApprovalMode)}
              />
            </div>
            <label className="field">
              <span>最大轮次</span>
              <Input
                type="number"
                min={1}
                max={200}
                value={String(cfg.agent.max_turns)}
                onChange={(e) => agent("max_turns", Math.max(1, Number(e.target.value) || 1))}
              />
            </label>
          </Card>
        </section>

        <section className="settings-section">
          <div className="section-heading">
            <span className="section-icon"><Flower size={18} /></span>
            <div>
              <h2>外观</h2>
              <p>点击即时生效。主题是 ~/.venus-whisper/themes/ 下的 TOML 文件，复制一份改颜色就是新主题。</p>
            </div>
          </div>
          <div className="theme-grid" role="radiogroup" aria-label="主题">
            {!themes.length && <p className="panel-hint">未找到主题文件，点右上角"打开配置目录"检查 themes/。</p>}
            {themes.map((t) => {
              const active = cfg.ui.theme === t.id;
              const swatch = [t.colors.accent, t.colors.side, t.colors.ink].filter(Boolean);
              return (
                <Card
                  key={t.id}
                  hoverable
                  role="radio"
                  aria-checked={active}
                  tabIndex={0}
                  className={`theme-card ${active ? "theme-active" : ""}`}
                  onClick={() => {
                    setCfg({ ...cfg, ui: { ...cfg.ui, theme: t.id } });
                    void setTheme(t.id);
                  }}
                  onKeyDown={(e) => {
                    if (e.key === "Enter" || e.key === " ") {
                      e.preventDefault();
                      setCfg({ ...cfg, ui: { ...cfg.ui, theme: t.id } });
                      void setTheme(t.id);
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
        </section>

        <section className="settings-section">
          <div className="section-heading">
            <span className="section-icon section-icon-coral"><Flower size={18} /></span>
            <div>
              <h2>Persona</h2>
              <p>作为每次对话的 system prompt 注入，对应 persona.md。</p>
            </div>
          </div>
          <Card className="form-card">
            <label className="field field-wide">
              <span>persona.md</span>
              <textarea className="persona" value={text} onChange={(e) => setText(e.target.value)} rows={8} />
            </label>
          </Card>
        </section>

        <div className="settings-actions">
          <Button type="primary" icon={<Check />} loading={saving} onClick={save}>
            保存设置
          </Button>
        </div>
      </div>
    </main>
  );
}
