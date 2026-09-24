// 消息渲染：用户气泡、assistant 正文（Markdown）、工具卡、审批卡。
import { Button, Card, Tag } from "animal-island-ui";
import { openUrl } from "@tauri-apps/plugin-opener";
import { useState, type ReactElement, type ReactNode } from "react";
import Markdown from "react-markdown";
import rehypeKatex from "rehype-katex";
import remarkGfm from "remark-gfm";
import remarkMath from "remark-math";
import { CodePre } from "./CopyButton";
import type { Message } from "../api";
import { useStore, type Segment, type ToolBlock } from "../store";
import { Check, ChevronDown, ChevronRight, Close, File, Folder, Globe, Key, Pencil, Search } from "./icons";

const TOOL_META: Record<string, { label: string; icon: ReactElement; write: boolean }> = {
  read_file: { label: "读取文件", icon: <File />, write: false },
  list_dir: { label: "列出目录", icon: <Folder />, write: false },
  grep: { label: "搜索内容", icon: <Search />, write: false },
  web_fetch: { label: "抓取网页", icon: <Globe />, write: false },
  write_file: { label: "写入文件", icon: <Pencil />, write: true },
  edit_file: { label: "修改文件", icon: <Pencil />, write: true },
  run_shell: { label: "运行本地命令", icon: <Key />, write: true },
};

function summary(t: ToolBlock): string {
  const a = t.args;
  return String(a.command ?? a.path ?? a.pattern ?? a.url ?? "");
}

/** 把 react-markdown 传给 <pre> 的 children 还原成纯文本。 */
function nodeText(node: ReactNode): string {
  if (node == null || typeof node === "boolean") return "";
  if (typeof node === "string" || typeof node === "number") return String(node);
  if (Array.isArray(node)) return node.map(nodeText).join("");
  if (typeof node === "object" && "props" in node) return nodeText((node as ReactElement<{ children?: ReactNode }>).props.children);
  return "";
}

/** ```lang 围栏语言写在 <code className="language-xxx"> 上。 */
function fenceLang(node: ReactNode): string | undefined {
  const el = Array.isArray(node) ? node[0] : node;
  const cls = el && typeof el === "object" && "props" in el ? (el as ReactElement<{ className?: string }>).props.className : undefined;
  return cls?.match(/language-([\w+-]+)/)?.[1];
}

const REMARK = [remarkGfm, remarkMath];
const REHYPE = [rehypeKatex];

const MD_COMPONENTS = {
  pre: ({ children }: { children?: ReactNode }) => (
    <CodePre text={nodeText(children).replace(/\n$/, "")} lang={fenceLang(children)} className="md-pre" />
  ),
  // 链接交给系统浏览器，否则会把 WebView 整个导航走
  a: ({ href, children }: { href?: string; children?: ReactNode }) => (
    <a
      href={href}
      onClick={(e) => {
        e.preventDefault();
        if (href) void openUrl(href).catch(() => window.open(href, "_blank"));
      }}
    >
      {children}
    </a>
  ),
  // 表格外包一层，宽表格横向滚动而不是撑破布局
  table: ({ children }: { children?: ReactNode }) => (
    <div className="md-table">
      <table>{children}</table>
    </div>
  ),
};

/** 模型常用的公式写法归一到 remark-math 认的形式：\[..\] / 单行 $$..$$ → 独立块；\(..\) → $..$。 */
export function normalizeMath(src: string): string {
  return src
    .replace(/\\\[([\s\S]+?)\\\]/g, (_, m) => `\n$$\n${m.trim()}\n$$\n`)
    .replace(/\\\(([\s\S]+?)\\\)/g, (_, m) => `$${m.trim()}$`)
    .replace(/^[ \t]*\$\$([^\n$]+?)\$\$[ \t]*$/gm, (_, m) => `$$\n${m.trim()}\n$$`);
}

export function AssistantText({ text }: { text: string }) {
  if (!text.trim()) return null;
  return (
    <div className="md">
      <Markdown remarkPlugins={REMARK} rehypePlugins={REHYPE} components={MD_COMPONENTS}>
        {normalizeMath(text)}
      </Markdown>
    </div>
  );
}

const STATUS: Record<ToolBlock["status"], { color: "app-yellow" | "app-green" | "app-red" | "default" | "app-blue"; text: string }> = {
  pending: { color: "app-yellow", text: "等待审批" },
  running: { color: "app-blue", text: "执行中" },
  done: { color: "app-green", text: "完成" },
  failed: { color: "app-red", text: "失败" },
  denied: { color: "default", text: "已拒绝" },
};

export function ToolCard({ tool }: { tool: ToolBlock }) {
  const [open, setOpen] = useState(false);
  const meta = TOOL_META[tool.name] ?? { label: tool.name, icon: <File />, write: true };
  if (tool.status === "pending") return <ApprovalCard tool={tool} />;
  const st = STATUS[tool.status];
  return (
    <Card className="tool-card">
      <div className="tool-head">
        <span className="tool-icon">{meta.icon}</span>
        <span className="tool-heading">
          <strong>{meta.label}</strong>
          <span title={summary(tool)}>{summary(tool)}</span>
        </span>
        <Tag size="small" color={st.color}>
          {st.text}
          {tool.duration_ms != null && tool.status === "done" ? ` · ${tool.duration_ms} ms` : ""}
        </Tag>
        {tool.output != null && (
          <Button type="text" size="small" aria-label={open ? "收起输出" : "展开输出"} icon={open ? <ChevronDown /> : <ChevronRight />} onClick={() => setOpen(!open)} />
        )}
      </div>
      {open && tool.output != null && <CodePre text={tool.output} className="tool-output" />}
    </Card>
  );
}

export function ApprovalCard({ tool }: { tool: ToolBlock }) {
  const approve = useStore((s) => s.approve);
  const workspace = useStore((s) => s.sessions.find((x) => x.id === s.currentId)?.workspace ?? "");
  const meta = TOOL_META[tool.name] ?? { label: tool.name, icon: <Key />, write: true };
  const isShell = tool.name === "run_shell";
  return (
    <Card color="app-yellow" className="approval-card">
      <div className="approval-head">
        <span className="tool-icon">{meta.icon}</span>
        <span className="tool-heading">
          <small className="eyebrow">需要你的确认</small>
          <strong>{meta.label}</strong>
        </span>
        <Tag size="small" color="app-red" variant="solid">
          写操作
        </Tag>
      </div>
      <CodePre text={summary(tool)} lang={isShell ? "bash" : undefined} className="command-block" />
      {tool.name === "write_file" && <p className="approval-fact">写入 {String((tool.args.content as string | undefined)?.length ?? 0)} 字符</p>}
      {tool.name === "edit_file" && (
        <div className="diff">
          <pre className="diff-old">{String(tool.args.old ?? "")}</pre>
          <pre className="diff-new">{String(tool.args.new ?? "")}</pre>
        </div>
      )}
      <p className="approval-fact">
        <Folder /> {workspace}
      </p>
      <div className="approval-actions">
        <Button size="small" icon={<Close />} onClick={() => approve(tool.call_id, "deny")}>
          拒绝
        </Button>
        {isShell && (
          <Button size="small" onClick={() => approve(tool.call_id, "always")}>
            总是允许
          </Button>
        )}
        <Button size="small" type="primary" icon={<Check />} onClick={() => approve(tool.call_id, "allow")}>
          允许
        </Button>
      </div>
    </Card>
  );
}

export function Segments({ segments }: { segments: Segment[] }) {
  return (
    <>
      {segments.map((seg, i) =>
        seg.kind === "text" ? <AssistantText key={i} text={seg.text} /> : <ToolCard key={seg.call_id} tool={seg} />,
      )}
    </>
  );
}

/** 把落库的 OpenAI 消息序列转成可渲染回合：assistant 的 tool_calls 与后续 tool 消息按 tool_call_id 合并。 */
export function toSegments(messages: Message[]): { msg: Message; segments: Segment[] }[] {
  const toolOutput = new Map<string, string>();
  for (const m of messages) {
    if (m.role === "tool") toolOutput.set(String(m.content.tool_call_id), String(m.content.content ?? ""));
  }
  const out: { msg: Message; segments: Segment[] }[] = [];
  for (const m of messages) {
    if (m.role === "user") {
      out.push({ msg: m, segments: [{ kind: "text", text: String(m.content.content ?? "") }] });
    } else if (m.role === "assistant") {
      const segments: Segment[] = [];
      const text = m.content.content;
      if (typeof text === "string" && text) segments.push({ kind: "text", text });
      const calls = (m.content.tool_calls as { id: string; function: { name: string; arguments: string } }[] | undefined) ?? [];
      for (const c of calls) {
        const output = toolOutput.get(c.id);
        let args: Record<string, unknown> = {};
        try {
          args = JSON.parse(c.function.arguments);
        } catch {
          args = { raw: c.function.arguments };
        }
        const status: ToolBlock["status"] =
          output == null ? "failed" : output === "denied by user" ? "denied" : output.startsWith("error:") ? "failed" : "done";
        segments.push({ kind: "tool", call_id: c.id, name: c.function.name, args, status, output });
      }
      out.push({ msg: m, segments });
    }
  }
  return out;
}
