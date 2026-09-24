//! Skill 文档（Markdown）。与人格同一套模型：
//! - 内置：编译期嵌入的 `src-tauri/skills/`，只读，id 为 `builtin/<name>`
//! - 本地：`~/.venus-whisper/skills/<name>.md`，可编辑，id 为 `<name>`
//!
//! `[agent] skills` 列出启用的 id，内容会追加进 system prompt。

use crate::config;
use crate::error::{Error, Result};
use include_dir::{include_dir, Dir};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const SKILLS_DIR: &str = "skills";
pub const BUILTIN_PREFIX: &str = "builtin/";

static BUILTIN: Dir = include_dir!("$CARGO_MANIFEST_DIR/skills");

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SkillSource {
    Builtin,
    Local,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Skill {
    pub id: String,
    pub name: String,
    /// 文档第一行标题（去掉 #）
    #[serde(default)]
    pub title: String,
    pub source: SkillSource,
    pub path: String,
    #[serde(default)]
    pub synced: bool,
    #[serde(default)]
    pub identical: bool,
    #[serde(default)]
    pub origin: Option<String>,
    #[serde(default)]
    pub modified: bool,
}

pub fn dir(config_dir: &Path) -> PathBuf {
    config_dir.join(SKILLS_DIR)
}

fn title_of(text: &str) -> String {
    text.lines()
        .find(|l| l.trim_start().starts_with('#'))
        .map(|l| l.trim_start_matches('#').trim().to_string())
        .unwrap_or_default()
}

fn builtin_text(stem: &str) -> Option<String> {
    BUILTIN
        .get_file(format!("{stem}.md"))
        .and_then(|f| f.contents_utf8())
        .map(str::to_string)
}

fn builtin_list() -> Vec<Skill> {
    let mut out: Vec<Skill> = BUILTIN
        .files()
        .filter(|f| f.path().extension().and_then(|e| e.to_str()) == Some("md"))
        .filter_map(|f| {
            let stem = f.path().file_stem()?.to_str()?;
            Some(Skill {
                id: format!("{BUILTIN_PREFIX}{stem}"),
                name: stem.into(),
                title: title_of(f.contents_utf8().unwrap_or_default()),
                source: SkillSource::Builtin,
                path: format!("{BUILTIN_PREFIX}{stem}"),
                synced: false,
                identical: false,
                origin: None,
                modified: false,
            })
        })
        .collect();
    out.sort_by(|a, b| a.id.cmp(&b.id));
    out
}

fn local_list(config_dir: &Path) -> Result<Vec<Skill>> {
    let d = dir(config_dir);
    std::fs::create_dir_all(&d)?;
    let mut out = vec![];
    for entry in std::fs::read_dir(&d)? {
        let p = entry?.path();
        if !p.is_file() || p.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }
        let Some(stem) = p.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        if stem.starts_with('.') {
            continue;
        }
        let text = std::fs::read_to_string(&p).unwrap_or_default();
        out.push(Skill {
            id: stem.into(),
            name: stem.into(),
            title: title_of(&text),
            source: SkillSource::Local,
            path: p.to_string_lossy().into_owned(),
            synced: false,
            identical: false,
            origin: None,
            modified: false,
        });
    }
    out.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(out)
}

/// 内置在前、本地在后，同名互相标注（比较正文，忽略行尾空白差异）。
pub fn list(config_dir: &Path) -> Result<Vec<Skill>> {
    let mut local = local_list(config_dir)?;
    let mut out = builtin_list();
    for b in &mut out {
        let stem = b.id[BUILTIN_PREFIX.len()..].to_string();
        if let Some(l) = local.iter_mut().find(|l| l.id == stem) {
            let lt = std::fs::read_to_string(&l.path).unwrap_or_default();
            let identical = builtin_text(&stem)
                .map(|t| t.trim_end() == lt.trim_end())
                .unwrap_or(false);
            b.synced = true;
            b.identical = identical;
            l.origin = Some(b.id.clone());
            l.modified = !identical;
        }
    }
    out.extend(local);
    Ok(out)
}

pub fn load(config_dir: &Path, id: &str) -> Result<String> {
    if let Some(stem) = id.strip_prefix(BUILTIN_PREFIX) {
        return builtin_text(stem).ok_or_else(|| Error::Msg(format!("内置 skill 不存在: {id}")));
    }
    let p = dir(config_dir).join(format!("{id}.md"));
    std::fs::read_to_string(&p).map_err(|_| Error::Msg(format!("skill 不存在: {id}")))
}

/// 启用的 skill：(名字, 正文)。缺失的静默跳过，不阻断对话。
pub fn load_enabled(config_dir: &Path, cfg: &config::Config) -> Vec<(String, String)> {
    cfg.agent
        .skills
        .iter()
        .filter_map(|id| {
            load(config_dir, id)
                .ok()
                .map(|t| (id.trim_start_matches(BUILTIN_PREFIX).to_string(), t))
        })
        .collect()
}

pub fn save(config_dir: &Path, id: &str, text: &str) -> Result<()> {
    if id.starts_with(BUILTIN_PREFIX) {
        return Err(Error::Msg("内置 skill 只读，请先同步到本地".into()));
    }
    let d = dir(config_dir);
    std::fs::create_dir_all(&d)?;
    std::fs::write(d.join(format!("{id}.md")), text)?;
    Ok(())
}

pub fn sync_to_local(config_dir: &Path, id: &str, overwrite: bool) -> Result<String> {
    let stem = id
        .strip_prefix(BUILTIN_PREFIX)
        .ok_or_else(|| Error::Msg("只有内置 skill 可以同步".into()))?;
    let text = builtin_text(stem).ok_or_else(|| Error::Msg(format!("内置 skill 不存在: {id}")))?;
    let d = dir(config_dir);
    std::fs::create_dir_all(&d)?;
    let target = d.join(format!("{stem}.md"));
    if target.exists() && !overwrite {
        return Err(Error::Msg(format!("本地已有 {stem}，请确认是否覆盖")));
    }
    std::fs::write(target, text)?;
    Ok(stem.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp() -> PathBuf {
        let p = std::env::temp_dir().join(format!("vw-skill-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn builtin_role_replication_is_embedded() {
        let b = builtin_list();
        assert!(b
            .iter()
            .any(|s| s.id == "builtin/role-replication-skill-v2"));
        assert!(load(
            Path::new("/nonexistent"),
            "builtin/role-replication-skill-v2"
        )
        .unwrap()
        .contains("Skill"));
        assert!(!b[0].title.is_empty(), "取第一行标题");
    }

    #[test]
    fn local_sync_diff_and_enabled_prompt() {
        let d = tmp();
        let id = "builtin/role-replication-skill-v2";
        assert!(
            list(&d)
                .unwrap()
                .iter()
                .all(|s| s.source == SkillSource::Builtin),
            "默认不落盘"
        );

        let local = sync_to_local(&d, id, false).unwrap();
        let l = list(&d).unwrap();
        assert!(l.iter().find(|s| s.id == id).unwrap().identical);
        assert!(!l.iter().find(|s| s.id == local).unwrap().modified);

        save(&d, &local, "# 改过的\n内容").unwrap();
        let l = list(&d).unwrap();
        assert!(l.iter().find(|s| s.id == local).unwrap().modified);
        assert_eq!(l.iter().find(|s| s.id == local).unwrap().title, "改过的");
        assert!(sync_to_local(&d, id, false).is_err());
        assert!(save(&d, id, "x").is_err(), "内置只读");

        let mut cfg = config::Config::default();
        cfg.agent.skills = vec![local.clone(), "ghost".into(), id.into()];
        let enabled = load_enabled(&d, &cfg);
        assert_eq!(enabled.len(), 2, "缺失的跳过");
        assert_eq!(enabled[0].0, "role-replication-skill-v2");
        assert_eq!(enabled[0].1, "# 改过的\n内容");
        std::fs::remove_dir_all(&d).unwrap();
    }
}
