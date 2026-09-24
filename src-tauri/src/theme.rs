//! 外观主题：`~/.venus-whisper/themes/<id>.toml`。内置三套首次启动写入，用户可改可加。

use crate::error::Result;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub const THEMES_DIR: &str = "themes";

/// 内置主题，(id, 文件内容)。只在文件不存在时写入，不覆盖用户改动。
pub const BUILTIN: &[(&str, &str)] = &[
    ("mint", include_str!("../themes/mint.toml")),
    ("peach", include_str!("../themes/peach.toml")),
    ("sky", include_str!("../themes/sky.toml")),
];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Theme {
    /// 文件名（不含扩展名），由加载时填充
    #[serde(default)]
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// 界面色 → CSS 变量 `--vw-<key>`
    #[serde(default)]
    pub colors: BTreeMap<String, String>,
    /// 代码块色 → CSS 变量 `--code-<key>`
    #[serde(default)]
    pub code: BTreeMap<String, String>,
}

pub fn dir(config_dir: &Path) -> PathBuf {
    config_dir.join(THEMES_DIR)
}

/// 写入缺失的内置主题文件。
pub fn ensure_builtin(config_dir: &Path) -> Result<()> {
    let d = dir(config_dir);
    std::fs::create_dir_all(&d)?;
    for (id, body) in BUILTIN {
        let p = d.join(format!("{id}.toml"));
        if !p.exists() {
            std::fs::write(p, body)?;
        }
    }
    Ok(())
}

/// 读取目录下所有 `*.toml`；解析失败的文件跳过。内置主题排前面，其余按文件名。
pub fn load_all(config_dir: &Path) -> Result<Vec<Theme>> {
    ensure_builtin(config_dir)?;
    let mut out = vec![];
    for entry in std::fs::read_dir(dir(config_dir))? {
        let p = entry?.path();
        if p.extension().and_then(|e| e.to_str()) != Some("toml") {
            continue;
        }
        let Some(id) = p.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        let Ok(text) = std::fs::read_to_string(&p) else {
            continue;
        };
        match toml::from_str::<Theme>(&text) {
            Ok(mut t) => {
                t.id = id.to_string();
                out.push(t);
            }
            Err(e) => eprintln!("theme {id} 解析失败，已跳过: {e}"),
        }
    }
    let rank = |t: &Theme| {
        BUILTIN
            .iter()
            .position(|(id, _)| *id == t.id)
            .unwrap_or(usize::MAX)
    };
    out.sort_by(|a, b| rank(a).cmp(&rank(b)).then_with(|| a.id.cmp(&b.id)));
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp() -> PathBuf {
        let p = std::env::temp_dir().join(format!("vw-theme-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn builtin_parse_and_have_full_keys() {
        for (id, body) in BUILTIN {
            let t: Theme = toml::from_str(body).unwrap_or_else(|e| panic!("{id}: {e}"));
            assert_eq!(t.colors.len(), 14, "{id} colors");
            assert_eq!(t.code.len(), 12, "{id} code");
            assert!(
                t.colors
                    .values()
                    .chain(t.code.values())
                    .all(|v| v.starts_with('#')),
                "{id} 颜色需为 hex"
            );
        }
    }

    #[test]
    fn user_theme_is_listed_and_bad_file_skipped() {
        let d = tmp();
        let list = load_all(&d).unwrap();
        assert_eq!(
            list.iter().map(|t| t.id.as_str()).collect::<Vec<_>>(),
            ["mint", "peach", "sky"]
        );

        std::fs::write(
            dir(&d).join("mine.toml"),
            "name = \"我的\"\n[colors]\naccent = \"#ff0000\"\n",
        )
        .unwrap();
        std::fs::write(dir(&d).join("broken.toml"), "name = [oops").unwrap();
        // 用户改了内置文件，不应被覆盖
        std::fs::write(dir(&d).join("mint.toml"), "name = \"改过的薄荷\"\n").unwrap();

        let list = load_all(&d).unwrap();
        let ids: Vec<_> = list.iter().map(|t| t.id.as_str()).collect();
        assert_eq!(ids, ["mint", "peach", "sky", "mine"]);
        assert_eq!(list[0].name, "改过的薄荷");
        assert_eq!(list[3].colors["accent"], "#ff0000");
        assert!(list[3].code.is_empty());
        std::fs::remove_dir_all(&d).unwrap();
    }
}
