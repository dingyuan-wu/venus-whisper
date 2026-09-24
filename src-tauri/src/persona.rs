//! 人格（system prompt）有两个来源：
//! - 内置：编译期嵌入的 `src-tauri/personas/`，只读，id 为 `builtin/<name>`；默认不落盘
//! - 本地：`~/.venus-whisper/personas/`，可编辑，id 为 `<name>`
//!
//! 两种来源下都支持两种形态：`<name>.md` 简单人格（同名图片为头像），
//! `<slug>/` 角色目录（角色还原 skill 的产物，读 `prompt.md`、`role.json`、`avatar.*`）。
//! 用户可把内置项"同步到本地"得到可编辑副本。

use crate::config;
use crate::error::{Error, Result};
use include_dir::{include_dir, Dir};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const PERSONAS_DIR: &str = "personas";
pub const BUILTIN_PREFIX: &str = "builtin/";
pub const DEFAULT_ID: &str = "builtin/venus";

static BUILTIN: Dir = include_dir!("$CARGO_MANIFEST_DIR/personas");

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PersonaKind {
    Md,
    Role,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PersonaSource {
    Builtin,
    Local,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Persona {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub kind: PersonaKind,
    pub source: PersonaSource,
    /// 本地：文件或目录路径；内置：`builtin/<name>`
    pub path: String,
    #[serde(default)]
    pub has_bible: bool,
    /// data URL
    #[serde(default)]
    pub avatar: Option<String>,
    /// 内置项：本地是否已有同名副本
    #[serde(default)]
    pub synced: bool,
    /// 内置项：本地副本内容（prompt + bible）与内置完全一致
    #[serde(default)]
    pub identical: bool,
    /// 本地项：对应的内置 id（同名），用于 diff
    #[serde(default)]
    pub origin: Option<String>,
    /// 本地项：与内置不一致
    #[serde(default)]
    pub modified: bool,
}

#[derive(Debug, Default, Deserialize)]
struct RoleMeta {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    description: Option<String>,
}

const AVATAR_EXTS: &[(&str, &str)] = &[
    ("png", "image/png"),
    ("jpg", "image/jpeg"),
    ("jpeg", "image/jpeg"),
    ("webp", "image/webp"),
    ("gif", "image/gif"),
];

pub fn dir(config_dir: &Path) -> PathBuf {
    config_dir.join(PERSONAS_DIR)
}

/// 建目录；迁移旧版 `persona.md` 为本地 `venus.md`。内置人格不落盘。
pub fn ensure_dirs(config_dir: &Path) -> Result<()> {
    let d = dir(config_dir);
    std::fs::create_dir_all(&d)?;
    let legacy = config_dir.join(config::PERSONA_FILE);
    if legacy.is_file() && !d.join("venus.md").exists() {
        std::fs::rename(&legacy, d.join("venus.md"))?;
    }
    Ok(())
}

// ---------- 内置 ----------

fn data_url(bytes: &[u8], mime: &str) -> String {
    format!("data:{mime};base64,{}", base64_encode(bytes))
}

fn builtin_avatar(base: &str) -> Option<String> {
    AVATAR_EXTS.iter().find_map(|(ext, mime)| {
        BUILTIN
            .get_file(format!("{base}.{ext}"))
            .map(|f| data_url(f.contents(), mime))
    })
}

fn builtin_list() -> Vec<Persona> {
    let mut out = vec![];
    for entry in BUILTIN.entries() {
        let Some(stem) = entry.path().file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        if stem.starts_with('.') {
            continue;
        }
        match entry {
            include_dir::DirEntry::File(f)
                if f.path().extension().and_then(|e| e.to_str()) == Some("md") =>
            {
                out.push(Persona {
                    id: format!("{BUILTIN_PREFIX}{stem}"),
                    name: stem.into(),
                    description: String::new(),
                    kind: PersonaKind::Md,
                    source: PersonaSource::Builtin,
                    path: format!("{BUILTIN_PREFIX}{stem}"),
                    has_bible: false,
                    avatar: builtin_avatar(stem),
                    synced: false,
                    identical: false,
                    origin: None,
                    modified: false,
                })
            }
            include_dir::DirEntry::Dir(d) if d.get_file(format!("{stem}/prompt.md")).is_some() => {
                let meta: RoleMeta = d
                    .get_file(format!("{stem}/role.json"))
                    .and_then(|f| f.contents_utf8())
                    .and_then(|t| serde_json::from_str(t).ok())
                    .unwrap_or_default();
                out.push(Persona {
                    id: format!("{BUILTIN_PREFIX}{stem}"),
                    name: meta.name.unwrap_or_else(|| stem.into()),
                    description: meta.description.unwrap_or_else(|| "角色目录".into()),
                    kind: PersonaKind::Role,
                    source: PersonaSource::Builtin,
                    path: format!("{BUILTIN_PREFIX}{stem}"),
                    has_bible: d.get_file(format!("{stem}/character_bible.md")).is_some(),
                    avatar: builtin_avatar(&format!("{stem}/avatar")),
                    synced: false,
                    identical: false,
                    origin: None,
                    modified: false,
                });
            }
            _ => {}
        }
    }
    out.sort_by(|a, b| {
        (a.id != DEFAULT_ID)
            .cmp(&(b.id != DEFAULT_ID))
            .then_with(|| a.id.cmp(&b.id))
    });
    out
}

fn builtin_prompt(stem: &str) -> Option<String> {
    BUILTIN
        .get_file(format!("{stem}.md"))
        .or_else(|| BUILTIN.get_file(format!("{stem}/prompt.md")))
        .and_then(|f| f.contents_utf8())
        .map(str::to_string)
}

fn builtin_bible(stem: &str) -> Option<String> {
    BUILTIN
        .get_file(format!("{stem}/character_bible.md"))
        .and_then(|f| f.contents_utf8())
        .map(str::to_string)
}

// ---------- 本地 ----------

/// 读 `<base>.<png|jpg|jpeg|webp|gif>` 为 data URL；用户头像与人格头像共用。
pub fn local_avatar(base: &Path) -> Option<String> {
    AVATAR_EXTS.iter().find_map(|(ext, mime)| {
        let p = base.with_extension(ext);
        let bytes = std::fs::read(&p).ok()?;
        Some(data_url(&bytes, mime))
    })
}

fn local_list(config_dir: &Path) -> Result<Vec<Persona>> {
    let mut out = vec![];
    for entry in std::fs::read_dir(dir(config_dir))? {
        let p = entry?.path();
        let Some(stem) = p.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        if stem.starts_with('.') {
            continue;
        }
        if p.is_file() && p.extension().and_then(|e| e.to_str()) == Some("md") {
            out.push(Persona {
                id: stem.into(),
                name: stem.into(),
                description: String::new(),
                kind: PersonaKind::Md,
                source: PersonaSource::Local,
                path: p.to_string_lossy().into_owned(),
                has_bible: false,
                avatar: local_avatar(&p.with_extension("")),
                synced: false,
                identical: false,
                origin: None,
                modified: false,
            });
        } else if p.is_dir() && p.join("prompt.md").is_file() {
            let meta: RoleMeta = std::fs::read_to_string(p.join("role.json"))
                .ok()
                .and_then(|t| serde_json::from_str(&t).ok())
                .unwrap_or_default();
            out.push(Persona {
                id: stem.into(),
                name: meta.name.unwrap_or_else(|| stem.into()),
                description: meta.description.unwrap_or_else(|| "角色目录".into()),
                kind: PersonaKind::Role,
                source: PersonaSource::Local,
                path: p.to_string_lossy().into_owned(),
                has_bible: p.join("character_bible.md").is_file(),
                avatar: local_avatar(&p.join("avatar")),
                synced: false,
                identical: false,
                origin: None,
                modified: false,
            });
        }
    }
    out.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(out)
}

fn local_prompt(p: &Persona) -> Option<String> {
    let file = match p.kind {
        PersonaKind::Md => PathBuf::from(&p.path),
        PersonaKind::Role => PathBuf::from(&p.path).join("prompt.md"),
    };
    std::fs::read_to_string(file).ok()
}

fn local_bible(p: &Persona) -> Option<String> {
    (p.kind == PersonaKind::Role)
        .then(|| std::fs::read_to_string(PathBuf::from(&p.path).join("character_bible.md")).ok())
        .flatten()
}

// ---------- 对外 ----------

/// 内置在前，本地在后。同名的内置/本地互相标注：内置项 `synced`/`identical`，本地项 `origin`/`modified`。
/// 一致性按 prompt 与 character_bible 的文本比较，行尾换行差异忽略。
pub fn list(config_dir: &Path) -> Result<Vec<Persona>> {
    ensure_dirs(config_dir)?;
    let mut local = local_list(config_dir)?;
    let mut out = builtin_list();
    for b in &mut out {
        let stem = b.id[BUILTIN_PREFIX.len()..].to_string();
        if let Some(l) = local.iter_mut().find(|l| l.id == stem) {
            let same = |a: Option<String>, b: Option<String>| {
                a.map(|s| s.trim_end().to_string()) == b.map(|s| s.trim_end().to_string())
            };
            let identical = same(builtin_prompt(&stem), local_prompt(l))
                && same(builtin_bible(&stem), local_bible(l));
            b.synced = true;
            b.identical = identical;
            l.origin = Some(b.id.clone());
            l.modified = !identical;
            // 本地副本没放头像时借用内置的，避免同步后头像丢失
            if l.avatar.is_none() {
                l.avatar = b.avatar.clone();
            }
        }
    }
    out.extend(local);
    Ok(out)
}

/// 读人格里的某个文件："prompt"（默认）或 "bible"。
pub fn load_file(config_dir: &Path, id: &str, file: &str) -> Result<String> {
    match file {
        "bible" => {
            if let Some(stem) = id.strip_prefix(BUILTIN_PREFIX) {
                return builtin_bible(stem)
                    .ok_or_else(|| Error::Msg(format!("{id} 没有 character_bible.md")));
            }
            let p = find(config_dir, id)?;
            local_bible(&p).ok_or_else(|| Error::Msg(format!("{id} 没有 character_bible.md")))
        }
        _ => load_prompt(config_dir, id),
    }
}

fn find(config_dir: &Path, id: &str) -> Result<Persona> {
    list(config_dir)?
        .into_iter()
        .find(|p| p.id == id)
        .ok_or_else(|| Error::Msg(format!("人格不存在: {id}")))
}

pub fn load_prompt(config_dir: &Path, id: &str) -> Result<String> {
    if let Some(stem) = id.strip_prefix(BUILTIN_PREFIX) {
        return builtin_prompt(stem).ok_or_else(|| Error::Msg(format!("内置人格不存在: {id}")));
    }
    let p = find(config_dir, id)?;
    let file = match p.kind {
        PersonaKind::Md => PathBuf::from(&p.path),
        PersonaKind::Role => PathBuf::from(&p.path).join("prompt.md"),
    };
    Ok(std::fs::read_to_string(file)?)
}

/// 配置里选中的人格；找不到时回落内置默认。
pub fn load_active(config_dir: &Path, cfg: &config::Config) -> Result<String> {
    load_prompt(config_dir, &cfg.agent.persona).or_else(|_| load_prompt(config_dir, DEFAULT_ID))
}

/// 只能改本地 md 人格。
pub fn save_prompt(config_dir: &Path, id: &str, text: &str) -> Result<()> {
    if id.starts_with(BUILTIN_PREFIX) {
        return Err(Error::Msg("内置人格只读，请先同步到本地".into()));
    }
    let p = find(config_dir, id)?;
    if p.kind != PersonaKind::Md {
        return Err(Error::Msg("角色目录请直接编辑其中的 prompt.md".into()));
    }
    std::fs::write(&p.path, text)?;
    Ok(())
}

/// 把内置人格复制到本地目录，返回本地 id。已存在且未指定 overwrite 时报错。
pub fn sync_to_local(config_dir: &Path, id: &str, overwrite: bool) -> Result<String> {
    let stem = id
        .strip_prefix(BUILTIN_PREFIX)
        .ok_or_else(|| Error::Msg("只有内置人格可以同步".into()))?;
    ensure_dirs(config_dir)?;
    let d = dir(config_dir);
    let exists = d.join(format!("{stem}.md")).exists() || d.join(stem).is_dir();
    if exists && !overwrite {
        return Err(Error::Msg(format!("本地已有 {stem}，请确认是否覆盖")));
    }
    if let Some(sub) = BUILTIN.get_dir(stem) {
        let target = d.join(stem);
        if target.exists() {
            std::fs::remove_dir_all(&target)?;
        }
        write_dir(sub, &d)?;
    } else if let Some(f) = BUILTIN.get_file(format!("{stem}.md")) {
        std::fs::write(d.join(format!("{stem}.md")), f.contents())?;
        for (ext, _) in AVATAR_EXTS {
            if let Some(img) = BUILTIN.get_file(format!("{stem}.{ext}")) {
                std::fs::write(d.join(format!("{stem}.{ext}")), img.contents())?;
            }
        }
    } else {
        return Err(Error::Msg(format!("内置人格不存在: {id}")));
    }
    Ok(stem.to_string())
}

fn write_dir(src: &Dir, root: &Path) -> Result<()> {
    for e in src.entries() {
        match e {
            include_dir::DirEntry::Dir(sub) => {
                std::fs::create_dir_all(root.join(sub.path()))?;
                write_dir(sub, root)?;
            }
            include_dir::DirEntry::File(f) => {
                if let Some(parent) = f.path().parent() {
                    std::fs::create_dir_all(root.join(parent))?;
                }
                std::fs::write(root.join(f.path()), f.contents())?;
            }
        }
    }
    Ok(())
}

/// 标准 base64，不引依赖。
fn base64_encode(bytes: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = (b[0] as u32) << 16 | (b[1] as u32) << 8 | b[2] as u32;
        out.push(T[(n >> 18) as usize & 63] as char);
        out.push(T[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            T[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            T[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const PNG_1X1: &[u8] = &[
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44,
        0x52,
    ];

    fn tmp() -> PathBuf {
        let p = std::env::temp_dir().join(format!("vw-persona-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn base64_matches_reference() {
        assert_eq!(base64_encode(b""), "");
        assert_eq!(base64_encode(b"f"), "Zg==");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
        assert_eq!(base64_encode(b"foo"), "Zm9v");
        assert_eq!(base64_encode(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn builtin_venus_is_embedded_and_first() {
        let b = builtin_list();
        assert_eq!(b[0].id, DEFAULT_ID);
        assert_eq!(b[0].source, PersonaSource::Builtin);
        assert!(builtin_prompt("venus").unwrap().contains("Venus Whisper"));
        assert!(
            load_prompt(Path::new("/nonexistent"), DEFAULT_ID).is_ok(),
            "内置人格不依赖本地目录"
        );
    }

    #[test]
    fn builtin_not_written_but_local_listed_with_markers() {
        let d = tmp();
        let pd = dir(&d);
        let list0 = list(&d).unwrap();
        assert!(!pd.join("venus.md").exists(), "默认不落盘");
        assert!(list0.iter().all(|p| p.source == PersonaSource::Builtin));

        std::fs::create_dir_all(pd.join("kaguya")).unwrap();
        std::fs::write(pd.join("kaguya/prompt.md"), "你是辉夜。").unwrap();
        std::fs::write(pd.join("kaguya/role.json"), r#"{"name":"四宫辉夜"}"#).unwrap();
        std::fs::write(pd.join("kaguya/avatar.png"), PNG_1X1).unwrap();
        std::fs::write(pd.join("coder.md"), "只回答代码。").unwrap();
        std::fs::create_dir_all(pd.join("broken")).unwrap();

        let list1 = list(&d).unwrap();
        let n_builtin = builtin_list().len();
        assert!(
            list1[..n_builtin]
                .iter()
                .all(|p| p.id.starts_with(BUILTIN_PREFIX)),
            "内置在前"
        );
        let local: Vec<_> = list1[n_builtin..].iter().map(|p| p.id.as_str()).collect();
        assert_eq!(local, ["coder", "kaguya"]);
        let list1 = &list1[n_builtin - 1..]; // 对齐下标：[内置末项, coder, kaguya]
        assert_eq!(list1[1].source, PersonaSource::Local);
        assert_eq!(list1[2].name, "四宫辉夜");
        assert!(list1[2]
            .avatar
            .as_deref()
            .unwrap()
            .starts_with("data:image/png;base64,"));
        assert_eq!(load_prompt(&d, "coder").unwrap(), "只回答代码。");
        std::fs::write(pd.join("kaguya/character_bible.md"), "# 设定集").unwrap();
        assert_eq!(load_file(&d, "kaguya", "bible").unwrap(), "# 设定集");
        assert!(load_file(&d, "coder", "bible").is_err());
        assert!(save_prompt(&d, "builtin/venus", "x").is_err());
        assert!(save_prompt(&d, "kaguya", "x").is_err());
        save_prompt(&d, "coder", "改了").unwrap();
        assert_eq!(load_prompt(&d, "coder").unwrap(), "改了");
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn sync_builtin_to_local_and_overwrite_rules() {
        let d = tmp();
        let id = sync_to_local(&d, DEFAULT_ID, false).unwrap();
        assert_eq!(id, "venus");
        let list = super::list(&d).unwrap();
        assert!(list
            .iter()
            .any(|p| p.id == "venus" && p.source == PersonaSource::Local));
        assert!(list.iter().find(|p| p.id == DEFAULT_ID).unwrap().synced);
        assert_eq!(
            load_prompt(&d, "venus").unwrap(),
            load_prompt(&d, DEFAULT_ID).unwrap()
        );

        std::fs::write(dir(&d).join("venus.md"), "本地改过").unwrap();
        assert!(
            sync_to_local(&d, DEFAULT_ID, false).is_err(),
            "已存在需确认"
        );
        assert_eq!(load_prompt(&d, "venus").unwrap(), "本地改过");
        sync_to_local(&d, DEFAULT_ID, true).unwrap();
        assert_eq!(
            load_prompt(&d, "venus").unwrap(),
            load_prompt(&d, DEFAULT_ID).unwrap()
        );
        assert!(sync_to_local(&d, "coder", false).is_err(), "本地项不能同步");
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn legacy_persona_md_becomes_local_venus() {
        let d = tmp();
        std::fs::write(d.join(config::PERSONA_FILE), "用户改过的旧人格").unwrap();
        ensure_dirs(&d).unwrap();
        assert!(!d.join(config::PERSONA_FILE).exists());
        assert_eq!(load_prompt(&d, "venus").unwrap(), "用户改过的旧人格");
        let mut cfg = config::Config::default();
        cfg.agent.persona = "ghost".into();
        assert!(
            load_active(&d, &cfg).unwrap().contains("Venus Whisper"),
            "找不到回落内置默认"
        );
        std::fs::remove_dir_all(&d).unwrap();
    }
}
