import { Button, Tag } from "animal-island-ui";
import { useEffect, useMemo, useRef, useState } from "react";
import { useStore } from "../store";
import { AssistantText, Segments, toSegments } from "./Blocks";
import { fmtTime } from "./Sidebar";
import { ArrowUp, Flower, Menu, Stop, User } from "./icons";

export function ChatView() {
  const { currentId, sessions, messages, live, settings, personas, userAvatar, usage, send, cancel, setSidebarOpen } = useStore();
  const debug = !!settings?.ui.debug;
  const runUsage = currentId ? usage[currentId] ?? [] : [];
  const userName = settings?.user?.name?.trim() || "你";
  const userIcon = userAvatar ? <img className="avatar-img" src={userAvatar} alt="" /> : <User />;
  const active = personas.find((p) => p.id === settings?.agent.persona);
  const agentName = active?.kind === "role" ? active.name : "Venus";
  const agentAvatar = active?.avatar ? <img className="avatar-img" src={active.avatar} alt="" /> : <Flower />;
  const session = sessions.find((s) => s.id === currentId);
  const msgs = currentId ? messages[currentId] ?? [] : [];
  const cur = currentId ? live[currentId] : undefined;
  const running = !!cur?.running;
  const waiting = cur?.segments.some((s) => s.kind === "tool" && s.status === "pending");
  const [draft, setDraft] = useState("");
  const scrollRef = useRef<HTMLDivElement>(null);
  const stickToBottom = useRef(true); // 用户在底部附近时才跟随流式输出滚动
  const taRef = useRef<HTMLTextAreaElement>(null);
  // 方向键翻历史：index 指向 history 的位置，-1 表示不在历史里；savedDraft 是进入历史前的草稿
  const histIdx = useRef(-1);
  const savedDraft = useRef("");

  useEffect(() => {
    const el = scrollRef.current;
    if (el && stickToBottom.current) el.scrollTop = el.scrollHeight;
  }, [msgs.length, cur?.segments]);

  const onScroll = () => {
    const el = scrollRef.current;
    if (el) stickToBottom.current = el.scrollHeight - el.scrollTop - el.clientHeight < 48;
  };

  const history = useMemo(() => {
    const out: string[] = [];
    for (const m of msgs) {
      if (m.role !== "user") continue;
      const t = String(m.content.content ?? "");
      if (t && out[out.length - 1] !== t) out.push(t);
    }
    return out;
  }, [msgs]);

  const setDraftAndGrow = (text: string) => {
    setDraft(text);
    requestAnimationFrame(() => {
      const el = taRef.current;
      if (!el) return;
      el.style.height = "";
      el.style.height = `${Math.min(el.scrollHeight, 160)}px`;
      el.setSelectionRange(text.length, text.length);
    });
  };

  /** ↑ 在第一行、↓ 在最后一行时翻历史，其余情况交给原生光标移动。 */
  const onArrow = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    const el = e.currentTarget;
    const before = el.value.slice(0, el.selectionStart);
    const after = el.value.slice(el.selectionEnd);
    if (e.key === "ArrowUp") {
      if (before.includes("\n") || !history.length) return;
      const next = histIdx.current < 0 ? history.length - 1 : histIdx.current - 1;
      if (next < 0) return;
      if (histIdx.current < 0) savedDraft.current = el.value;
      histIdx.current = next;
      e.preventDefault();
      setDraftAndGrow(history[next]);
    } else if (e.key === "ArrowDown") {
      if (after.includes("\n") || histIdx.current < 0) return;
      e.preventDefault();
      const next = histIdx.current + 1;
      if (next >= history.length) {
        histIdx.current = -1;
        setDraftAndGrow(savedDraft.current);
      } else {
        histIdx.current = next;
        setDraftAndGrow(history[next]);
      }
    }
  };

  const submit = () => {
    const text = draft.trim();
    if (!text || running) return;
    stickToBottom.current = true;
    histIdx.current = -1;
    savedDraft.current = "";
    setDraft("");
    if (taRef.current) taRef.current.style.height = "";
    void send(text);
  };

  const turns = toSegments(msgs);

  return (
    <main className="chat-shell">
      <header className="chat-header">
        <span className="mobile-only">
          <Button type="text" size="small" aria-label="打开会话栏" icon={<Menu />} onClick={() => setSidebarOpen(true)} />
        </span>
        <div className="chat-title">
          <h1>{session?.title ?? "Venus Whisper"}</h1>
          <Tag size="small" color={running ? (waiting ? "app-yellow" : "app-blue") : "app-teal"}>
            {running ? (waiting ? "等待审批" : "思考中") : "就绪"}
          </Tag>
        </div>
      </header>

      <div className="conversation" ref={scrollRef} onScroll={onScroll}>
        <div className="conversation-inner">
          {!turns.length && !cur && (
            <div className="empty-state">
              <span className="brand-mark big">
                <Flower size={28} />
              </span>
              <h2>有什么要做的？</h2>
              <p>工具只会在 workspace 内读写与执行，写操作按审批模式确认。</p>
            </div>
          )}
          {turns.map(({ msg, segments }) => (
            <article key={msg.id} className={`message message-${msg.role}`}>
              <span className={`avatar avatar-${msg.role}`}>{msg.role === "user" ? userIcon : agentAvatar}</span>
              <div className="message-body">
                <div className="message-meta">
                  <strong>{msg.role === "user" ? userName : agentName}</strong>
                  <time>{fmtTime(msg.created_at)}</time>
                </div>
                {msg.role === "user" ? (
                  <div className="user-bubble">{segments[0]?.kind === "text" ? segments[0].text : ""}</div>
                ) : (
                  <Segments segments={segments} />
                )}
              </div>
            </article>
          ))}
          {cur && (
            <article className="message message-assistant">
              <span className="avatar avatar-assistant">{agentAvatar}</span>
              <div className="message-body">
                <div className="message-meta">
                  <strong>{agentName}</strong>
                </div>
                <Segments segments={cur.segments} />
                {running && !waiting && <AssistantText text={cur.segments.length ? "" : "…"} />}
                {running && !waiting && <span className="typing" aria-label="正在生成" />}
              </div>
            </article>
          )}
          {debug && runUsage.length > 0 && (
            <aside className="debug-usage" aria-label="token 用量">
              <div className="debug-head">
                <span>DEBUG · 本轮 {runUsage.length} 次调用</span>
                <span>
                  合计 输入 {runUsage.reduce((a, u) => a + u.prompt_tokens, 0)} · 输出 {runUsage.reduce((a, u) => a + u.completion_tokens, 0)}
                </span>
              </div>
              {runUsage.map((u) => {
                const b = u.breakdown;
                const parts: [string, number][] = [
                  ["人格", b.persona],
                  ["技能", b.skills],
                  ["输出约定", b.convention],
                  ["工具定义", b.tools],
                  ["历史对话", b.history],
                  ["本次消息", b.current],
                  ["本轮工具往返", b.run],
                ];
                const sum = parts.reduce((a, [, v]) => a + v, 0);
                return (
                  <div key={u.call} className="debug-call">
                    <div className="debug-row">
                      <span>#{u.call}</span>
                      <span>输入 {u.prompt_tokens}</span>
                      <span>输出 {u.completion_tokens}</span>
                      <span>系统提示词 {u.system_tokens}</span>
                      {u.estimated && <span className="debug-est">估算</span>}
                    </div>
                    <div className="debug-row debug-sub">
                      <span>输入构成</span>
                      {parts.filter(([, v]) => v > 0).map(([k, v]) => (
                        <span key={k}>
                          {k} {v}
                        </span>
                      ))}
                      <span className="debug-est">构成为本地估算{u.estimated ? "" : `，合计 ${sum}，服务端计 ${u.prompt_tokens}`}</span>
                    </div>
                  </div>
                );
              })}
            </aside>
          )}
        </div>
      </div>

      <div className="composer-zone">
        <div className="composer">
          <textarea
            ref={taRef}
            rows={1}
            value={draft}
            placeholder="告诉 Venus 接下来要做什么…"
            aria-label="消息"
            onChange={(e) => {
              histIdx.current = -1;
              setDraft(e.target.value);
              e.target.style.height = "";
              e.target.style.height = `${Math.min(e.target.scrollHeight, 160)}px`;
            }}
            onKeyDown={(e) => {
              // 输入法组合中（选候选词、按 Enter 上屏）不当作发送或翻历史；Safari 会在 compositionend 后再发一个 keyCode 229 的 keydown
              if (e.nativeEvent.isComposing || e.nativeEvent.keyCode === 229) return;
              if (e.key === "Enter" && !e.shiftKey) {
                e.preventDefault();
                submit();
              } else if (e.key === "ArrowUp" || e.key === "ArrowDown") {
                onArrow(e);
              }
            }}
          />
          <div className="composer-footer">
            <span className="composer-hint">Enter 发送 · Shift+Enter 换行 · ↑↓ 翻历史</span>
            {running ? (
              <Button type="primary" danger size="small" icon={<Stop />} aria-label="停止" onClick={cancel}>
                停止
              </Button>
            ) : (
              <Button type="primary" size="small" icon={<ArrowUp />} aria-label="发送" disabled={!draft.trim()} onClick={submit}>
                发送
              </Button>
            )}
          </div>
        </div>
      </div>

    </main>
  );
}
