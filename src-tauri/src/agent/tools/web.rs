use super::{arg_str, truncate, Tool, ToolCtx};
use crate::error::{Error, Result};
use async_trait::async_trait;
use serde_json::{json, Value};
use std::time::Duration;

pub struct WebFetch;

#[async_trait]
impl Tool for WebFetch {
    fn name(&self) -> &'static str {
        "web_fetch"
    }
    fn description(&self) -> &'static str {
        "抓取 http(s) 网页并转成纯文本（去掉标签、脚本和样式）。"
    }
    fn schema(&self) -> Value {
        json!({"type":"object","properties":{"url":{"type":"string"}},"required":["url"]})
    }
    async fn run(&self, args: Value, _ctx: &ToolCtx) -> Result<String> {
        let url = arg_str(&args, "url")?;
        if !(url.starts_with("http://") || url.starts_with("https://")) {
            return Err(Error::Msg("只支持 http/https".into()));
        }
        let resp = reqwest::Client::builder()
            .timeout(Duration::from_secs(20))
            .user_agent("venus-whisper/0.1")
            .build()?
            .get(url)
            .send()
            .await?;
        let status = resp.status();
        let is_html = resp
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .map(|v| v.contains("html"))
            .unwrap_or(false);
        let body = resp.text().await?;
        let text = if is_html { html_to_text(&body) } else { body };
        Ok(truncate(format!("[HTTP {status}]\n{text}")))
    }
}

pub fn html_to_text(html: &str) -> String {
    let re_block = regex::Regex::new(
        r"(?is)<script[^>]*>.*?</script>|<style[^>]*>.*?</style>|<noscript[^>]*>.*?</noscript>",
    )
    .unwrap();
    let re_br = regex::Regex::new(r"(?i)<(br|/p|/div|/li|/h[1-6]|/tr)[^>]*>").unwrap();
    let re_tag = regex::Regex::new(r"(?s)<[^>]+>").unwrap();
    let re_ws = regex::Regex::new(r"[ \t]+").unwrap();
    let re_nl = regex::Regex::new(r"\n\s*\n+").unwrap();
    let s = re_block.replace_all(html, "");
    let s = re_br.replace_all(&s, "\n");
    let s = re_tag.replace_all(&s, "");
    let s = s
        .replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"");
    let s = re_ws.replace_all(&s, " ");
    re_nl.replace_all(&s, "\n").trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_tags_scripts_and_styles() {
        let html = "<html><head><style>p{}</style><script>var x=1;</script></head><body><h1>Hi</h1><p>a &amp; b</p><div>c</div></body></html>";
        assert_eq!(html_to_text(html), "Hi\na & b\nc");
    }
}
