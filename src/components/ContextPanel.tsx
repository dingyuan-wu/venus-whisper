// 右侧会话信息栏：常驻、只读。当前可操作目录、审批模式、模型、本目录已放行命令。
import { Card, Tag } from "animal-island-ui";
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
  const { sessions, currentId, settings, lastReaction } = useStore();
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
        <Card className="model-card">
          <strong>{settings?.llm.model ?? "—"}</strong>
          <span title={settings?.llm.base_url}>{settings?.llm.base_url ?? ""}</span>
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
