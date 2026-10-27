//! A visual kept: one node, `SynVisuals/<id>.md`.
//!
//! # Why a node, and why not in `Syn/`
//!
//! Every visual Syn draws is kept (plan §7, decision 4): a conversation opened
//! again shows its visuals, and Nexus finds one by its title. A node is how
//! anything in the vault is found, synced and versioned, and `.md` is one of
//! the three formats sync carries (`sync::utils::is_syncable_document`), so a
//! visual is a Markdown file — the model's fragment in a fenced block, what it
//! answered in the frontmatter.
//!
//! Not in `Syn/`, where conversations and runs live: the scan skips every
//! folder of that name (`commands::nodes::is_in_unscanned_dir`), and a visual
//! there would be kept but never found. `SynVisuals/` sits beside `SynSkills/`.
//!
//! The model never writes one directly. `syn_*` kinds are reserved
//! (`taint::reserved_type`): `create_node` and `update_node` refuse them, and
//! `show_visual` (PR 4) is the one door, after `check`.

use std::path::Path;

use serde::Serialize;

use crate::error::{AppError, AppResult};

pub const FOLDER: &str = "SynVisuals";

pub const TYPE: &str = "syn_visual";

/// What a visual is, which decides the design guide it was drawn with.
pub const KINDS: &[&str] = &["diagram", "chart", "explainer", "simulation"];

/// A visual about to be kept.
pub struct Draft<'a> {
    pub title: &'a str,
    pub kind: &'a str,
    /// The fragment the model wrote, after `check` passed it.
    pub html: &'a str,
    /// What the person asked, for the note it is kept in and for Nexus.
    pub question: &'a str,
    pub conversation: Option<&'a str>,
    pub model: Option<&'a str>,
}

/// A visual read back.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Visual {
    pub id: String,
    pub title: String,
    pub kind: String,
    pub question: String,
    pub conversation: Option<String>,
    #[serde(skip)]
    pub html: String,
}

/// An id is 32 lower-case hex digits: a v4 UUID without its dashes. Nothing
/// else reaches a path.
pub fn is_id(id: &str) -> bool {
    id.len() == 32 && id.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

pub fn path_of(id: &str) -> String {
    format!("{FOLDER}/{id}.md")
}

/// The fragment in a fenced block longer than any run of backticks in it, so
/// no fragment can close the block early.
fn fenced(html: &str) -> String {
    let longest = html.split(|c| c != '`').map(str::len).max().unwrap_or(0);
    let fence = "`".repeat(longest.max(2) + 1);
    format!("{fence}html\n{html}\n{fence}\n")
}

/// The fragment back out of the fenced block `fenced` wrote.
fn unfenced(body: &str) -> Option<String> {
    let start = body.find("html\n")?;
    let fence = &body[..start];
    let fence = fence.trim_start();
    if fence.len() < 3 || !fence.bytes().all(|b| b == b'`') {
        return None;
    }
    let inner = &body[start + "html\n".len()..];
    let end = inner.rfind(&format!("\n{fence}"))?;
    Some(inner[..end].to_string())
}

/// Keep a visual. Returns its id.
pub fn save<R: tauri::Runtime>(app: &tauri::AppHandle<R>, db: &crate::db::DbState, vault: &str, draft: &Draft<'_>) -> AppResult<String> {
    if !KINDS.contains(&draft.kind) {
        return Err(AppError::General(format!("A visual's kind is one of {}, not `{}`.", KINDS.join(", "), draft.kind)));
    }
    let id = uuid::Uuid::new_v4().simple().to_string();
    let mut properties = serde_json::Map::new();
    properties.insert("kind".into(), draft.kind.into());
    properties.insert("question".into(), draft.question.into());
    if let Some(conversation) = draft.conversation {
        properties.insert("conversation".into(), conversation.into());
    }
    if let Some(model) = draft.model {
        properties.insert("model".into(), model.into());
    }
    crate::commands::nodes::write_node_inner(
        app,
        db,
        vault.to_string(),
        path_of(&id),
        draft.title.to_string(),
        TYPE.to_string(),
        serde_json::Value::Object(properties),
        Some(fenced(draft.html)),
    )?;
    Ok(id)
}

/// Read a kept visual. `None` for an id that is not one, a file that is not
/// there, or a file there that is not a visual.
pub fn load(vault: &Path, id: &str) -> Option<Visual> {
    if !is_id(id) {
        return None;
    }
    let file = vault.join(path_of(id));
    let node = crate::utils::node_parser::parse_file_to_node(&vault.to_string_lossy(), &file)?;
    if node.node_type != TYPE {
        return None;
    }
    let text = |key: &str| node.properties.get(key).and_then(|v| v.as_str()).map(str::to_string);
    Some(Visual {
        id: id.to_string(),
        title: node.title.clone(),
        kind: text("kind").unwrap_or_else(|| "explainer".into()),
        question: text("question").unwrap_or_default(),
        conversation: text("conversation"),
        html: unfenced(&node.content)?,
    })
}

/// What the chat shows above a visual's frame: its title and kind, never its
/// HTML, which reaches the screen only through `synvisual://`.
#[tauri::command]
pub fn syn_visual_meta(vault_path: String, id: String) -> Result<Visual, AppError> {
    load(Path::new(&vault_path), &id).ok_or_else(|| AppError::General(format!("There is no visual {id}.")))
}

/// A kept visual as one HTML file that opens in any browser, for "Download".
/// Desktop only, where the libraries it writes in are. See `assets::standalone`.
#[cfg(desktop)]
#[tauri::command]
pub fn syn_visual_export(vault_path: String, id: String, theme: String) -> Result<String, AppError> {
    let visual = load(Path::new(&vault_path), &id).ok_or_else(|| AppError::General(format!("There is no visual {id}.")))?;
    Ok(super::assets::standalone(&visual.html, &theme))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_id_is_32_hex_and_nothing_else() {
        assert!(is_id(&uuid::Uuid::new_v4().simple().to_string()));
        for bad in ["", "../../etc/passwd", "0123456789abcdef0123456789abcdeF", "0123456789abcdef0123456789abcde", "0123456789abcdef0123456789abcdef0", "0123456789abcdef/123456789abcdef"] {
            assert!(!is_id(bad), "{bad}");
        }
    }

    /// No fragment can close its own block early, however many backticks it
    /// holds, and what comes back is what went in.
    #[test]
    fn a_fragment_survives_its_fence() {
        for html in ["<p>plain</p>", "<pre>```js\nx\n```</pre>", "<code>````</code>\n<p>after</p>", "<svg></svg>\n"] {
            let body = fenced(html);
            assert_eq!(unfenced(&body).as_deref(), Some(html), "{body}");
        }
    }

    #[test]
    fn a_visual_is_kept_and_read_back() {
        use tauri::Manager;
        let dir = tempfile::tempdir().expect("vault");
        let vault = std::fs::canonicalize(dir.path()).expect("canonical");
        let app = tauri::test::mock_builder().build(tauri::test::mock_context(tauri::test::noop_assets())).expect("app");
        app.manage::<crate::db::DbState>(std::sync::Mutex::new(crate::db::DbBridge::new_in_memory_full().expect("db")));
        let db = app.state::<crate::db::DbState>();
        let html = "<input type=\"range\">\n<pre>```</pre>";
        let id = save(
            app.handle(),
            db.inner(),
            &vault.to_string_lossy(),
            &Draft {
                title: "Con lắc đơn — chu kỳ theo chiều dài",
                kind: "simulation",
                html,
                question: "giải thích dao động con lắc đơn",
                conversation: Some("conv-1"),
                model: Some("gpt-6-luna"),
            },
        )
        .expect("kept");
        assert!(is_id(&id));
        assert!(vault.join(path_of(&id)).exists(), "in {FOLDER}/");
        assert!(!crate::commands::nodes::is_in_unscanned_dir(&path_of(&id)), "the scan finds it");

        let back = load(&vault, &id).expect("read back");
        assert_eq!(back.title, "Con lắc đơn — chu kỳ theo chiều dài");
        assert_eq!(back.kind, "simulation");
        assert_eq!(back.question, "giải thích dao động con lắc đơn");
        assert_eq!(back.conversation.as_deref(), Some("conv-1"));
        assert_eq!(back.html, html);

        assert!(load(&vault, "0123456789abcdef0123456789abcdef").is_none(), "no such visual");
        let err = save(app.handle(), db.inner(), &vault.to_string_lossy(), &Draft { kind: "poster", ..Draft { title: "x", kind: "chart", html: "<p>", question: "", conversation: None, model: None } });
        assert!(err.is_err(), "an unknown kind");
    }

    /// A note someone named like a visual is not one.
    #[test]
    fn a_file_that_is_not_a_visual_is_not_read_as_one() {
        let dir = tempfile::tempdir().expect("vault");
        let id = "0123456789abcdef0123456789abcdef";
        std::fs::create_dir_all(dir.path().join(FOLDER)).expect("folder");
        std::fs::write(dir.path().join(path_of(id)), "---\ntype: note\n---\n```html\n<p>x</p>\n```\n").expect("note");
        assert!(load(dir.path(), id).is_none());
    }
}
