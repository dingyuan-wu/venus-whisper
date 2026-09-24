use super::{arg_str, resolve_in_workspace, truncate, Tool, ToolCtx};
use crate::error::{Error, Result};
use async_trait::async_trait;
use serde_json::{json, Value};
use std::path::Path;

pub struct ReadFile;
#[async_trait]
impl Tool for ReadFile {
    fn name(&self) -> &'static str {
        "read_file"
    }
    fn description(&self) -> &'static str {
        "读取 workspace 内的文本文件。可选 offset/limit 按行读取大文件。"
    }
    fn schema(&self) -> Value {
        json!({"type":"object","properties":{
            "path":{"type":"string","description":"相对 workspace 的路径"},
            "offset":{"type":"integer","description":"起始行号，从 1 开始"},
            "limit":{"type":"integer","description":"最多读取行数"}
        },"required":["path"]})
    }
    async fn run(&self, args: Value, ctx: &ToolCtx) -> Result<String> {
        let p = resolve_in_workspace(&ctx.workspace, arg_str(&args, "path")?)?;
        let text = tokio::fs::read_to_string(&p).await?;
        let offset = args
            .get("offset")
            .and_then(|v| v.as_u64())
            .unwrap_or(1)
            .max(1) as usize;
        let limit = args
            .get("limit")
            .and_then(|v| v.as_u64())
            .map(|v| v as usize);
        let out: String = match limit {
            None if offset == 1 => text,
            _ => text
                .lines()
                .enumerate()
                .skip(offset - 1)
                .take(limit.unwrap_or(usize::MAX))
                .map(|(i, l)| format!("{}\t{l}\n", i + 1))
                .collect(),
        };
        Ok(truncate(out))
    }
}

pub struct ListDir;
#[async_trait]
impl Tool for ListDir {
    fn name(&self) -> &'static str {
        "list_dir"
    }
    fn description(&self) -> &'static str {
        "列出目录内容，目录以 / 结尾。默认列 workspace 根。"
    }
    fn schema(&self) -> Value {
        json!({"type":"object","properties":{"path":{"type":"string"}}})
    }
    async fn run(&self, args: Value, ctx: &ToolCtx) -> Result<String> {
        let rel = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");
        let p = resolve_in_workspace(&ctx.workspace, rel)?;
        let mut names: Vec<String> = std::fs::read_dir(&p)?
            .filter_map(|e| e.ok())
            .map(|e| {
                let n = e.file_name().to_string_lossy().into_owned();
                if e.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                    format!("{n}/")
                } else {
                    n
                }
            })
            .collect();
        names.sort();
        Ok(truncate(names.join("\n")))
    }
}

pub struct Grep;
#[async_trait]
impl Tool for Grep {
    fn name(&self) -> &'static str {
        "grep"
    }
    fn description(&self) -> &'static str {
        "在 workspace 内按正则递归搜索文本，输出 path:line: text。跳过 .git/node_modules/target 与二进制文件，最多 200 条。"
    }
    fn schema(&self) -> Value {
        json!({"type":"object","properties":{
            "pattern":{"type":"string","description":"Rust 正则"},
            "path":{"type":"string","description":"搜索起点，默认 workspace 根"}
        },"required":["pattern"]})
    }
    async fn run(&self, args: Value, ctx: &ToolCtx) -> Result<String> {
        let re = regex::Regex::new(arg_str(&args, "pattern")?)
            .map_err(|e| Error::Msg(format!("正则错误: {e}")))?;
        let rel = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");
        let root = resolve_in_workspace(&ctx.workspace, rel)?;
        let ws = ctx.workspace.canonicalize()?;
        let mut out = vec![];
        let mut stack = vec![root];
        'outer: while let Some(dir) = stack.pop() {
            if dir.is_file() {
                grep_file(&re, &dir, &ws, &mut out);
                continue;
            }
            let Ok(rd) = std::fs::read_dir(&dir) else {
                continue;
            };
            for e in rd.filter_map(|e| e.ok()) {
                let p = e.path();
                let name = e.file_name().to_string_lossy().into_owned();
                if p.is_dir() {
                    if !matches!(name.as_str(), ".git" | "node_modules" | "target" | "dist") {
                        stack.push(p);
                    }
                } else if p.is_file() {
                    grep_file(&re, &p, &ws, &mut out);
                }
                if out.len() >= 200 {
                    out.push("[... 超过 200 条，已截断 ...]".into());
                    break 'outer;
                }
            }
        }
        Ok(truncate(if out.is_empty() {
            "无匹配".into()
        } else {
            out.join("\n")
        }))
    }
}

fn grep_file(re: &regex::Regex, p: &Path, ws: &Path, out: &mut Vec<String>) {
    let Ok(bytes) = std::fs::read(p) else { return };
    if bytes.iter().take(8192).any(|&b| b == 0) {
        return; // 二进制
    }
    let text = String::from_utf8_lossy(&bytes);
    let rel = p.strip_prefix(ws).unwrap_or(p).display().to_string();
    for (i, line) in text.lines().enumerate() {
        if re.is_match(line) {
            out.push(format!("{rel}:{}: {}", i + 1, line.trim_end()));
            if out.len() >= 200 {
                return;
            }
        }
    }
}

pub struct WriteFile;
#[async_trait]
impl Tool for WriteFile {
    fn name(&self) -> &'static str {
        "write_file"
    }
    fn description(&self) -> &'static str {
        "写入（覆盖）workspace 内的文件，父目录不存在时自动创建。"
    }
    fn schema(&self) -> Value {
        json!({"type":"object","properties":{"path":{"type":"string"},"content":{"type":"string"}},"required":["path","content"]})
    }
    async fn run(&self, args: Value, ctx: &ToolCtx) -> Result<String> {
        let rel = arg_str(&args, "path")?;
        // 允许新建多级目录：先在 workspace 内校验最近的已存在祖先
        let p = resolve_new_path(&ctx.workspace, rel)?;
        if let Some(parent) = p.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let content = arg_str(&args, "content")?;
        tokio::fs::write(&p, content).await?;
        Ok(format!("已写入 {} ({} 字节)", rel, content.len()))
    }
}

/// 新建文件专用：逐级向上找到第一个已存在的祖先做沙箱校验。
fn resolve_new_path(ws: &Path, rel: &str) -> Result<std::path::PathBuf> {
    let ws_c = ws.canonicalize()?;
    let raw = if Path::new(rel).is_absolute() {
        std::path::PathBuf::from(rel)
    } else {
        ws_c.join(rel)
    };
    if rel.split(['/', '\\']).any(|c| c == "..") {
        return Err(Error::Msg(format!("拒绝：路径 {rel} 含 ..")));
    }
    let mut anc = raw.as_path();
    while !anc.exists() {
        anc = anc
            .parent()
            .ok_or_else(|| Error::Msg(format!("非法路径: {rel}")))?;
    }
    if !anc.canonicalize()?.starts_with(&ws_c) {
        return Err(Error::Msg(format!("拒绝：路径 {rel} 超出 workspace")));
    }
    Ok(raw)
}

pub struct EditFile;
#[async_trait]
impl Tool for EditFile {
    fn name(&self) -> &'static str {
        "edit_file"
    }
    fn description(&self) -> &'static str {
        "精确字串替换：old 必须在文件中恰好出现一次。"
    }
    fn schema(&self) -> Value {
        json!({"type":"object","properties":{"path":{"type":"string"},"old":{"type":"string"},"new":{"type":"string"}},"required":["path","old","new"]})
    }
    async fn run(&self, args: Value, ctx: &ToolCtx) -> Result<String> {
        let rel = arg_str(&args, "path")?;
        let p = resolve_in_workspace(&ctx.workspace, rel)?;
        let old = arg_str(&args, "old")?;
        let new = arg_str(&args, "new")?;
        let text = tokio::fs::read_to_string(&p).await?;
        match text.matches(old).count() {
            0 => Err(Error::Msg("old 在文件中不存在".into())),
            1 => {
                tokio::fs::write(&p, text.replacen(old, new, 1)).await?;
                Ok(format!("已修改 {rel}"))
            }
            n => Err(Error::Msg(format!("old 出现 {n} 次，需要唯一"))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::temp_ws;
    use super::*;

    #[tokio::test]
    async fn write_read_edit_grep_flow() {
        let ws = temp_ws();
        let ctx = ToolCtx {
            workspace: ws.clone(),
        };
        WriteFile
            .run(
                json!({"path":"a/b/c.txt","content":"foo\nbar\nfoo\n"}),
                &ctx,
            )
            .await
            .unwrap();
        assert_eq!(
            ReadFile
                .run(json!({"path":"a/b/c.txt"}), &ctx)
                .await
                .unwrap(),
            "foo\nbar\nfoo\n"
        );
        assert_eq!(
            ReadFile
                .run(json!({"path":"a/b/c.txt","offset":2,"limit":1}), &ctx)
                .await
                .unwrap(),
            "2\tbar\n"
        );

        let err = EditFile
            .run(json!({"path":"a/b/c.txt","old":"foo","new":"x"}), &ctx)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("2 次"));
        EditFile
            .run(json!({"path":"a/b/c.txt","old":"bar","new":"baz"}), &ctx)
            .await
            .unwrap();

        let hits = Grep.run(json!({"pattern":"ba."}), &ctx).await.unwrap();
        assert_eq!(hits, "a/b/c.txt:2: baz");
        assert_eq!(ListDir.run(json!({"path":"a"}), &ctx).await.unwrap(), "b/");

        assert!(WriteFile
            .run(json!({"path":"../x","content":""}), &ctx)
            .await
            .is_err());
        std::fs::remove_dir_all(&ws).unwrap();
    }
}
