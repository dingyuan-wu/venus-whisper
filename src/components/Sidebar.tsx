import { Button, Input, Modal } from "animal-island-ui";
import { useMemo, useState } from "react";
import type { Session } from "../api";
import { useStore } from "../store";
import { Chat, Close, Flower, Plus, Search, Settings, Trash } from "./icons";

const DAY = 86_400_000;

function groupOf(s: Session): string {
  const age = Date.now() - s.created_at;
  if (age < DAY) return "今天";
  if (age < 7 * DAY) return "过去 7 天";
  return "更早";
}

export function fmtTime(ms: number): string {
  const d = new Date(ms);
  const age = Date.now() - ms;
  if (age < DAY) return d.toTimeString().slice(0, 5);
  if (age < 7 * DAY) return ["周日", "周一", "周二", "周三", "周四", "周五", "周六"][d.getDay()];
  return `${d.getMonth() + 1}/${d.getDate()}`;
}

export function Sidebar() {
  const { sessions, currentId, sidebarOpen, selectSession, newSession, deleteSession, setView, setSidebarOpen } = useStore();
  const [q, setQ] = useState("");
  const [pendingDelete, setPendingDelete] = useState<Session | null>(null);

  const groups = useMemo(() => {
    const query = q.trim().toLowerCase();
    const list = query ? sessions.filter((s) => s.title.toLowerCase().includes(query)) : sessions;
    const map = new Map<string, Session[]>();
    for (const s of list) map.set(groupOf(s), [...(map.get(groupOf(s)) ?? []), s]);
    return [...map.entries()];
  }, [sessions, q]);

  return (
    <aside className={`sidebar ${sidebarOpen ? "sidebar-open" : ""}`} aria-label="会话导航">
      <div className="brand-row">
        <span className="brand-mark" aria-hidden="true">
          <Flower size={18} />
        </span>
        <span className="brand-copy">
          <strong>Venus Whisper</strong>
          <small>LOCAL AGENT</small>
        </span>
        <span className="mobile-only">
          <Button type="text" size="small" aria-label="关闭会话栏" icon={<Close />} onClick={() => setSidebarOpen(false)} />
        </span>
      </div>

      <Button type="primary" block icon={<Plus />} onClick={newSession}>
        新建会话 <kbd>⌘N</kbd>
      </Button>

      <Input
        size="small"
        placeholder="搜索会话"
        prefix={<Search />}
        allowClear
        value={q}
        onChange={(e) => setQ(e.target.value)}
        onClear={() => setQ("")}
      />

      <div className="session-scroll">
        {groups.map(([group, items]) => (
          <section className="session-group" key={group}>
            <h2>{group}</h2>
            {items.map((s) => (
              <div key={s.id} className={`session-item ${s.id === currentId ? "session-current" : ""}`}>
                <Button block type={s.id === currentId ? "default" : "text"} className="session-row" onClick={() => selectSession(s.id)} icon={<Chat />}>
                  <span className="session-title">{s.title}</span>
                  <span className="session-time">{fmtTime(s.created_at)}</span>
                </Button>
                <Button
                  type="text"
                  size="small"
                  danger
                  className="session-delete"
                  aria-label={`删除会话 ${s.title}`}
                  icon={<Trash />}
                  onClick={() => setPendingDelete(s)}
                />
              </div>
            ))}
          </section>
        ))}
        {!groups.length && <p className="session-empty">{q ? "没有匹配的会话" : "还没有会话，发一条消息开始"}</p>}
      </div>

      <Modal
        open={!!pendingDelete}
        typewriter={false}
        title="删除这个会话？"
        onClose={() => setPendingDelete(null)}
        footer={
          <>
            <Button onClick={() => setPendingDelete(null)}>取消</Button>
            <Button
              type="primary"
              danger
              onClick={() => {
                const id = pendingDelete?.id;
                setPendingDelete(null);
                if (id) void deleteSession(id);
              }}
            >
              删除
            </Button>
          </>
        }
      >
        「{pendingDelete?.title}」的消息记录会一并删除，无法恢复。
      </Modal>

      <div className="sidebar-footer">
        <Button type="text" block icon={<Settings />} onClick={() => setView("settings")}>
          设置
        </Button>
        <span className="local-state">
          <span className="status-dot" /> 本地运行
        </span>
      </div>
    </aside>
  );
}
