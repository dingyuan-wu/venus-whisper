import { Button } from "animal-island-ui";
import { useState } from "react";
import { highlight } from "./highlight";
import { Check, Copy } from "./icons";

/** 一键复制，成功后图标变对勾 1.5 秒。 */
export function CopyButton({
  text,
  label = "复制",
  className,
  size = 18,
}: {
  text: string;
  label?: string;
  className?: string;
  size?: number;
}) {
  const [done, setDone] = useState(false);
  const copy = async () => {
    try {
      await navigator.clipboard.writeText(text);
      setDone(true);
      window.setTimeout(() => setDone(false), 1500);
    } catch {
      /* 剪贴板不可用时静默 */
    }
  };
  return (
    <Button
      type="text"
      size="small"
      className={className}
      aria-label={done ? "已复制" : label}
      title={done ? "已复制" : label}
      icon={done ? <Check size={size} /> : <Copy size={size} />}
      onClick={copy}
      disabled={!text}
    />
  );
}

/** 带复制按钮的代码/命令块；给了 lang 且已注册时按语法着色。 */
export function CodePre({ text, lang, className = "" }: { text: string; lang?: string; className?: string }) {
  return (
    <div className={`code-pre ${className}`}>
      <pre>
        <code className="hljs" dangerouslySetInnerHTML={{ __html: highlight(text, lang) }} />
      </pre>
      <CopyButton text={text} className="code-copy" />
    </div>
  );
}
