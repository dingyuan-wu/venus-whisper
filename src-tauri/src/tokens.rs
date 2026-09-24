//! 本地 token 估算：拿不到服务端 usage 时的兜底。
//! 规则：CJK 等宽字符按 1 token，其他按 4 个字符 1 token；只求量级，标注 estimated 让用户知道。

pub fn estimate(text: &str) -> u32 {
    let mut cjk = 0u32;
    let mut other = 0u32;
    for c in text.chars() {
        if is_cjk(c) {
            cjk += 1;
        } else if !c.is_whitespace() {
            other += 1;
        }
    }
    cjk + other.div_ceil(4)
}

fn is_cjk(c: char) -> bool {
    matches!(c as u32,
        0x3000..=0x30FF | 0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xAC00..=0xD7AF | 0xF900..=0xFAFF | 0xFF00..=0xFFEF | 0x20000..=0x2FFFF)
}

/// 估算一组 OpenAI 消息的 token：正文 + 每条约 4 个结构开销。
pub fn estimate_messages(messages: &[serde_json::Value]) -> u32 {
    messages
        .iter()
        .map(|m| {
            let body = match m.get("content") {
                Some(serde_json::Value::String(s)) => estimate(s),
                Some(v) if !v.is_null() => estimate(&v.to_string()),
                _ => 0,
            };
            let tools = m
                .get("tool_calls")
                .map(|t| estimate(&t.to_string()))
                .unwrap_or(0);
            body + tools + 4
        })
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rough_but_monotonic() {
        assert_eq!(estimate(""), 0);
        assert_eq!(estimate("abcd"), 1);
        assert_eq!(estimate("你好世界"), 4);
        assert_eq!(estimate("hello 世界"), 2 + 2);
        assert!(estimate(&"x".repeat(400)) == 100);
        let msgs = vec![
            serde_json::json!({"role":"user","content":"你好"}),
            serde_json::json!({"role":"assistant","content":null,"tool_calls":[{"id":"c","function":{"name":"read_file","arguments":"{}"}}]}),
        ];
        assert!(estimate_messages(&msgs) > 8);
    }
}
