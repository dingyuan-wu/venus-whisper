// 右侧会话信息栏：常驻、只读。当前可操作目录、审批模式、模型、本目录已放行命令。
import { Button, Card, Tag } from "animal-island-ui";
import { useStore } from "../store";
import { CopyButton } from "./CopyButton";
import { Folder } from "./icons";

const MODE_LABEL: Record<string, string> = {
  ask: "询问：写文件和 Shell 都确认",
  "auto-edit": "自动编辑：只确认 Shell",
  "full-auto": "全自动：目录内不再确认",
};

/** 配置里的 allow 条目可能写 `~`，会话里存的是展开后的绝对路径。 */
function sameWorkspace(entry: string, ws: string): boolean {
  if (entry === ws) return true;
  return entry.startsWith("~") && entry.length > 1 && ws.endsWith(entry.slice(1));
}

export function ContextPanel() {
  const { sessions, currentId, settings, lastReaction, context, setActiveModel } = useStore();
  const active = settings?.llm.models.find((m) => m.id === settings.llm.active) ?? settings?.llm.models[0];
  const ctx = currentId ? context[currentId] : undefined;
  const ctxWindow = active?.context_window ?? 0;
  const used = ctx ? ctx.prompt_tokens + ctx.completion_tokens : 0;
  const pct = ctxWindow > 0 ? Math.min(100, Math.round((used / ctxWindow) * 100)) : null;
  const session = sessions.find((s) => s.id === currentId);
  const ws = session?.workspace ?? settings?.agent.workspace ?? "";
  const allow = settings?.agent.allow.find((e) => sameWorkspace(e.workspace, ws))?.commands ?? [];
  return (
    <aside className="context-panel" aria-label="会话信息">
      <section>
        <h3 className="panel-label">当前可操作目录</h3>
        <Card className="workspace-card">
          <Folder />
          <span className="workspace-path" title={ws}>
            {ws || "未设置"}
          </span>
          <CopyButton text={ws} label="复制路径" />
        </Card>
        <p className="panel-hint">工具只能在这个目录内读写与执行命令，越界会被拒绝。目录在设置页修改，对新会话生效。</p>
      </section>

      <section>
        <h3 className="panel-label">审批模式</h3>
        <Card>{settings ? MODE_LABEL[settings.agent.approval_mode] ?? settings.agent.approval_mode : "—"}</Card>
      </section>

      <section>
        <h3 className="panel-label">模型</h3>
        {settings && settings.llm.models.length > 1 && (
          <div className="model-list" role="radiogroup" aria-label="切换模型">
            {settings.llm.models.map((m) => (
              <Button
                key={m.id}
                size="small"
                block
                type={m.id === active?.id ? "primary" : "text"}
                role="radio"
                aria-checked={m.id === active?.id}
                onClick={() => setActiveModel(m.id)}
              >
                {m.name || m.id}
              </Button>
            ))}
          </div>
        )}
        <Card className="model-card">
          <strong>{active?.model ?? "未配置模型"}</strong>
          <span title={active?.base_url}>{active?.base_url ?? "在设置里添加"}</span>
        </Card>
      </section>

      <section>
        <h3 className="panel-label">上下文占用</h3>
        <Card className="ctx-card">
          {pct == null ? (
            <span className="panel-hint">在模型设置里填上下文窗口后显示百分比</span>
          ) : (
            <>
              <div className="ctx-row">
                <strong className={pct >= 90 ? "ctx-danger" : pct >= 70 ? "ctx-warn" : ""}>{pct}%</strong>
                <span>
                  {used.toLocaleString()} / {ctxWindow.toLocaleString()} tokens{ctx?.estimated ? "（估算）" : ""}
                </span>
              </div>
              <div className="ctx-bar" role="progressbar" aria-valuenow={pct} aria-valuemin={0} aria-valuemax={100}>
                <i style={{ width: `${pct}%` }} className={pct >= 90 ? "ctx-danger" : pct >= 70 ? "ctx-warn" : ""} />
              </div>
            </>
          )}
        </Card>
      </section>

      <section>
        <h3 className="panel-label">本目录已放行的命令</h3>
        {allow.length ? (
          <div className="allow-list">
            {allow.map((c) => (
              <Tag key={c} size="small" color="app-teal">
                {c}
              </Tag>
            ))}
          </div>
        ) : (
          <p className="panel-hint">审批卡上点"总是允许"后会出现在这里；要移除请编辑 config.toml。</p>
        )}
      </section>

      {lastReaction && (
        <section className="panel-foot">
          <h3 className="panel-label">最近反应</h3>
          <div className="allow-list">
            <Tag size="small" variant="outlined">{lastReaction.emotion}</Tag>
            <Tag size="small" variant="outlined">{lastReaction.intent}</Tag>
            <Tag size="small" variant="outlined">{lastReaction.suggested_reaction}</Tag>
          </div>
        </section>
      )}
    </aside>
  );
}
