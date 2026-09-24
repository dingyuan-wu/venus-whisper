// 只注册聊天里常见的语言，避免整包 1MB。
import hljs from "highlight.js/lib/core";
import bash from "highlight.js/lib/languages/bash";
import css from "highlight.js/lib/languages/css";
import diff from "highlight.js/lib/languages/diff";
import go from "highlight.js/lib/languages/go";
import ini from "highlight.js/lib/languages/ini";
import javascript from "highlight.js/lib/languages/javascript";
import json from "highlight.js/lib/languages/json";
import markdown from "highlight.js/lib/languages/markdown";
import python from "highlight.js/lib/languages/python";
import rust from "highlight.js/lib/languages/rust";
import sql from "highlight.js/lib/languages/sql";
import typescript from "highlight.js/lib/languages/typescript";
import xml from "highlight.js/lib/languages/xml";
import yaml from "highlight.js/lib/languages/yaml";

const LANGS: Record<string, Parameters<typeof hljs.registerLanguage>[1]> = {
  bash, css, diff, go, ini, javascript, json, markdown, python, rust, sql, typescript, xml, yaml,
};
for (const [name, def] of Object.entries(LANGS)) hljs.registerLanguage(name, def);
hljs.registerAliases(["sh", "shell", "zsh", "console"], { languageName: "bash" });
hljs.registerAliases(["toml"], { languageName: "ini" });
hljs.registerAliases(["js", "jsx", "mjs"], { languageName: "javascript" });
hljs.registerAliases(["ts", "tsx"], { languageName: "typescript" });
hljs.registerAliases(["html", "svg"], { languageName: "xml" });
hljs.registerAliases(["yml"], { languageName: "yaml" });
hljs.registerAliases(["py"], { languageName: "python" });
hljs.registerAliases(["md"], { languageName: "markdown" });
hljs.registerAliases(["rs"], { languageName: "rust" });

/** 返回带 hljs-* class 的 HTML；未知语言或未指定语言时不着色，只做转义。 */
export function highlight(code: string, lang?: string): string {
  const name = lang?.toLowerCase();
  if (name && hljs.getLanguage(name)) {
    return hljs.highlight(code, { language: name, ignoreIllegals: true }).value;
  }
  return code.replace(/[&<>]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;" })[c]!);
}
