//! Changes held for the person, after a run has read somebody else's words.
//!
//! # What this replaces
//!
//! A run that had read a web page, a feed article or a file could make new
//! things and nothing else: changing a note that was already there was
//! refused, and the person had to ask again in a new message. That was safe —
//! a page saying *empty the note `keep.md`* could not have it done — and it was
//! the heaviest cost the safety model had. "Read this PDF and update my project
//! note" is ordinary work, and it was refused every time.
//!
//! Now the change is **held**. The run carries on as though it were made —
//! told plainly that it was not — and when it has answered, the held changes
//! wait on one card: what each does, to what, and what the run had read that
//! made it wait. Nothing reaches the vault until the person approves it, item
//! by item; approving is the person saying that change is theirs.
//!
//! # What makes it as safe as refusing
//!
//! - The card's words are Rust's, from the call itself and the vault as it is:
//!   the note's real title, its fields before and after, its body before and
//!   after. Nothing the model wrote describes what a change does.
//! - A change is applied as staged or not at all: by hash of the call, and only
//!   if the note has not changed since it was staged — the person approved a
//!   change to the note they saw.
//! - It is applied through `execute_tool` with the app's PIN locks in force,
//!   as the model's call would have been. A locked note is refused when the
//!   change is staged, and again when it is applied.
//! - Only in the app, only for the run the person asked (not a helper), and
//!   never in plan mode. Telegram and routines have no card to hold it on, and
//!   still refuse. See `gate::decide`.
//! - Which tools can be held is their row in `syn::policy` (`staged()`): the
//!   changes a person can be shown. A recipe is the union of its steps and
//!   reshaping a kind touches every note of it; both are still refused.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::db::DbBridge;

/// Emitted when a run ends with changes waiting, for the card.
pub const EVENT: &str = "syn-changes-needed";

/// What the model is told in place of the tool's result.
pub const HELD: &str = "Held for the user's approval: this run has read something from outside the vault, so \
    changes to what was already there wait on a card the user sees when you finish. It has NOT been made yet — \
    do not read it back to check, and do not try it another way. Carry on with the rest of the work, and in your \
    answer say what you changed and why, and that it waits for their approval.";

/// The error a change is left with when its note changed after it was held,
/// followed by the note's id.
pub const CHANGED_SINCE: &str = "changed_since:";

/// Where one held change stands.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum State {
    Held,
    Applied,
    Declined,
    Failed,
}

/// What a change is, for the card to say in the person's language.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Edit,
    Trash,
    Restore,
    Version,
    Memory,
    Board,
    Table,
    TransactionNew,
    TransactionEdit,
    TransactionDelete,
}

/// One field, before and after. `None` is "not there".
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct Field {
    pub name: String,
    pub before: Option<String>,
    pub after: Option<String>,
}

/// One held change.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Change {
    /// The call's hash (`writes::hash`): what is applied is what was held.
    pub hash: String,
    pub tool: String,
    pub args: Value,
    pub kind: Kind,
    /// What it is about: the notes' titles, a transaction's note.
    pub titles: Vec<String>,
    #[serde(default)]
    pub fields: Vec<Field>,
    /// A body, before and after, where the change replaces one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub before: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after: Option<String>,
    /// Each note it touches, with its `updated_at` when it was held. Applying
    /// refuses a note that has changed since.
    #[serde(default)]
    pub seen: Vec<(String, String)>,
    pub state: State,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Everything one run held, and why it waited.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct Changeset {
    pub changes: Vec<Change>,
    /// What the run had read from outside: a site, a file, a connector.
    #[serde(default)]
    pub because: Vec<String>,
}

impl Changeset {
    /// Whether anything is still waiting for the person.
    pub fn awaiting(&self) -> bool {
        self.changes.iter().any(|c| c.state == State::Held)
    }

    /// Hold one more. The same call twice is held once.
    pub fn hold(&mut self, change: Change) -> bool {
        if self.changes.iter().any(|c| c.hash == change.hash) {
            return false;
        }
        self.changes.push(change);
        true
    }

    /// Note what was read, once each.
    pub fn read(&mut self, sources: Vec<String>) {
        for s in sources {
            if !self.because.contains(&s) {
                self.because.push(s);
            }
        }
    }
}

/// How much of a body the card shows, before and after.
const BODY_CHARS: usize = 2_000;

fn cut(text: &str) -> String {
    let mut out: String = text.chars().take(BODY_CHARS).collect();
    if text.chars().count() > BODY_CHARS {
        out.push('…');
    }
    out
}

fn shown(v: &Value) -> Option<String> {
    match v {
        Value::Null => None,
        Value::String(s) => Some(s.clone()),
        other => Some(other.to_string()),
    }
}

fn ids(args: &Value, one: &str, many: &str) -> Vec<String> {
    let mut out: Vec<String> = args
        .get(many)
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(Value::as_str).map(str::to_string).collect())
        .unwrap_or_default();
    if let Some(id) = args.get(one).and_then(Value::as_str) {
        out.insert(0, id.to_string());
    }
    out.dedup();
    out
}

fn str_arg(args: &Value, key: &str) -> Option<String> {
    args.get(key).and_then(shown).filter(|s| !s.trim().is_empty())
}

/// The fields a transaction call names, as the card shows them.
fn transaction_fields(args: &Value) -> Vec<Field> {
    ["amount", "type", "category", "account", "account_id", "note", "date", "month"]
        .iter()
        .filter_map(|k| str_arg(args, k).map(|v| Field { name: (*k).to_string(), before: None, after: Some(v) }))
        .collect()
}

/// Describe a call as a held change, from the call and the vault as it is.
///
/// An error when there is nothing to describe — a note that is not there — so
/// the model is told, as it would have been by the tool.
pub fn describe(db: &DbBridge, tool: &str, args: &Value) -> Result<Change, String> {
    let mut change = Change {
        hash: crate::syn::writes::hash(tool, args),
        tool: tool.to_string(),
        args: args.clone(),
        kind: Kind::Edit,
        titles: Vec::new(),
        fields: Vec::new(),
        before: None,
        after: None,
        seen: Vec::new(),
        state: State::Held,
        error: None,
    };
    let note = |id: &str, change: &mut Change| -> Result<crate::models::node::NodeMetadata, String> {
        let node = db
            .get_node(id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("No node `{id}`"))?;
        change.titles.push(node.title.clone());
        change.seen.push((node.id.clone(), node.updated_at.clone()));
        Ok(node)
    };

    match tool {
        "update_node" => {
            let targets = ids(args, "node_id", "node_ids");
            if targets.is_empty() {
                return Err("Missing required parameter: node_id".into());
            }
            let mut first = None;
            for id in &targets {
                let node = note(id, &mut change)?;
                first.get_or_insert(node);
            }
            let first = first.expect("one at least");
            if let Some(props) = args.get("properties").and_then(Value::as_object) {
                for (k, v) in props {
                    change.fields.push(Field {
                        name: k.clone(),
                        before: first.properties.get(k).and_then(shown),
                        after: shown(v),
                    });
                }
            }
            if let Some(body) = args.get("content").and_then(Value::as_str) {
                change.before = Some(cut(&first.content));
                change.after = Some(cut(body));
            }
        }
        "trash_node" => {
            change.kind = Kind::Trash;
            let targets = ids(args, "node_id", "node_ids");
            if targets.is_empty() {
                return Err("Missing required parameter: node_id".into());
            }
            for id in &targets {
                note(id, &mut change)?;
            }
        }
        "restore_node" => {
            change.kind = Kind::Restore;
            change.titles.push(str_arg(args, "trash_path").ok_or("Missing required parameter: trash_path")?);
        }
        "restore_version" => {
            change.kind = Kind::Version;
            let id = str_arg(args, "node_id").ok_or("Missing required parameter: node_id")?;
            note(&id, &mut change)?;
            change.fields.push(Field { name: "version_id".into(), before: None, after: str_arg(args, "version_id") });
        }
        "remember" => {
            change.kind = Kind::Memory;
            change.after = Some(cut(&str_arg(args, "body").ok_or("Missing required parameter: body")?));
            if let Some(subject) = str_arg(args, "subject") {
                change.titles.push(subject);
            }
        }
        "edit_board" => {
            change.kind = Kind::Board;
            let board = str_arg(args, "board").ok_or("Missing required parameter: board")?;
            match note(&board, &mut change) {
                Ok(_) => {}
                Err(_) => change.titles.push(board),
            }
            change.after = args.get("changes").map(|c| cut(&serde_json::to_string_pretty(c).unwrap_or_default()));
        }
        "table_rows" => {
            change.kind = Kind::Table;
            let id = str_arg(args, "node_id").ok_or("Missing required parameter: node_id")?;
            note(&id, &mut change)?;
            let mut what = serde_json::Map::new();
            for k in ["table", "add", "update", "delete"] {
                if let Some(v) = args.get(k) {
                    what.insert(k.into(), v.clone());
                }
            }
            change.after = Some(cut(&serde_json::to_string_pretty(&Value::Object(what)).unwrap_or_default()));
        }
        "create_transaction" => {
            change.kind = Kind::TransactionNew;
            change.fields = transaction_fields(args);
            change.titles.push(str_arg(args, "note").or_else(|| str_arg(args, "category")).unwrap_or_default());
        }
        "update_transaction" => {
            change.kind = Kind::TransactionEdit;
            change.titles.push(str_arg(args, "transaction_id").ok_or("Missing required parameter: transaction_id")?);
            change.fields = transaction_fields(args);
        }
        "delete_transaction" => {
            change.kind = Kind::TransactionDelete;
            change.titles.push(str_arg(args, "transaction_id").ok_or("Missing required parameter: transaction_id")?);
        }
        other => return Err(format!("`{other}` is not a change that can be held")),
    }
    Ok(change)
}

/// What the run had read from outside, in words a person recognises: a site,
/// a file, a feed, a connector. Taken from the steps, newest last.
pub fn read_from(steps: &[crate::syn::run::Step]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for step in steps.iter().filter(|s| s.ok == Some(true)) {
        let Some(tool) = step.tool.as_deref() else { continue };
        let args = step.args.clone().unwrap_or(Value::Null);
        let said = if tool == crate::syn::tools::BROWSE_TOOL {
            let what = str_arg(&args, "site").or_else(|| str_arg(&args, "what")).unwrap_or_default();
            Some(
                url::Url::parse(&what)
                    .ok()
                    .and_then(|u| u.host_str().map(str::to_string))
                    .unwrap_or(what),
            )
        } else if matches!(tool, "read_file_text" | "read_spreadsheet") {
            str_arg(&args, "path")
                .or_else(|| str_arg(&args, "file_id"))
                .map(|p| p.rsplit('/').next().unwrap_or(&p).to_string())
        } else if matches!(tool, "read_feed_article" | "search_feed_articles") {
            Some("feeds".into())
        } else if tool == "search_files" {
            Some("files".into())
        } else if tool == "get_node" {
            // A clipping: by its title, if the step says what it was.
            serde_json::from_str::<Value>(&step.preview)
                .ok()
                .filter(|v| v.get("properties").is_some_and(crate::syn::taint::from_outside))
                .and_then(|v| v.get("title").and_then(Value::as_str).map(str::to_string))
        } else {
            crate::syn::connector::provider::server_slug_of(tool).map(str::to_string)
        };
        if let Some(said) = said.filter(|s| !s.trim().is_empty()) {
            if !out.contains(&said) {
                out.push(said);
            }
        }
    }
    out
}

/// What applying one change came to, for the card.
#[derive(Serialize, Debug, Clone)]
pub struct Outcome {
    pub hash: String,
    pub state: State,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Apply the approved changes and decline the rest.
///
/// Each approved one goes through `execute_tool` as the model's call would
/// have — the PIN locks in force — with nothing read: the person approving it
/// is what lets it go. A note that changed after it was held is not
/// overwritten; the person approved a change to the note they saw.
pub fn apply<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    db: &crate::db::DbState,
    vault_path: &str,
    run_id: &str,
    set: &mut Changeset,
    approved: &[String],
) -> Vec<Outcome> {
    let nothing_read = crate::syn::taint::Taint::new();
    let ctx = crate::syn::tools::ToolContext {
        db,
        vault_path,
        app,
        run_id: Some(run_id),
        model: Some(&nothing_read),
    };
    for change in set.changes.iter_mut().filter(|c| c.state == State::Held) {
        if !approved.contains(&change.hash) {
            change.state = State::Declined;
            continue;
        }
        let moved = {
            let guard = db.lock().unwrap_or_else(|e| e.into_inner());
            change.seen.iter().find(|(id, at)| {
                guard.get_node(id).ok().flatten().map(|n| n.updated_at) != Some(at.clone())
            }).map(|(id, _)| id.clone())
        };
        if let Some(id) = moved {
            change.state = State::Failed;
            // A code, not a sentence: the card says it in the person's language.
            change.error = Some(format!("{CHANGED_SINCE}{id}"));
            continue;
        }
        let said = crate::syn::tools::as_approved(|| crate::syn::tools::execute_tool(&ctx, &change.tool, &change.args));
        let error = match said {
            Ok(out) => serde_json::from_str::<Value>(&out).ok().and_then(|v| {
                v.get("error").or_else(|| v.get("refused")).map(|e| e.as_str().map(str::to_string).unwrap_or_else(|| e.to_string()))
            }),
            Err(e) => Some(e.to_string()),
        };
        match error {
            None => change.state = State::Applied,
            Some(e) => {
                change.state = State::Failed;
                change.error = Some(e);
            }
        }
    }
    set.changes
        .iter()
        .map(|c| Outcome { hash: c.hash.clone(), state: c.state, error: c.error.clone() })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::node::NodeMetadata;
    use serde_json::json;

    fn db_with(id: &str, title: &str, content: &str, properties: Value) -> DbBridge {
        let db = DbBridge::new_in_memory_full().expect("db");
        db.upsert_node(&NodeMetadata {
            id: id.into(),
            node_type: "note".into(),
            title: title.into(),
            content: content.into(),
            properties,
            created_at: "2026-01-01 00:00:00".into(),
            updated_at: "2026-01-01 00:00:00".into(),
            timestamp: 0,
            blocks: None,
        })
        .expect("seed");
        db
    }

    /// The card says what a change does from the vault, not from the model:
    /// the note's own title, the field as it is and as it would be.
    #[test]
    fn an_edit_is_described_from_the_note_as_it_is() {
        let db = db_with("Notes/p.md", "Dự án Q3", "cũ", json!({ "status": "open" }));
        let c = describe(&db, "update_node", &json!({ "node_id": "Notes/p.md", "properties": { "status": "done" }, "content": "mới" })).expect("held");
        assert_eq!(c.kind, Kind::Edit);
        assert_eq!(c.titles, vec!["Dự án Q3".to_string()]);
        assert_eq!(c.fields, vec![Field { name: "status".into(), before: Some("open".into()), after: Some("done".into()) }]);
        assert_eq!((c.before.as_deref(), c.after.as_deref()), (Some("cũ"), Some("mới")));
        let at = db.get_node("Notes/p.md").unwrap().unwrap().updated_at;
        assert_eq!(c.seen, vec![("Notes/p.md".to_string(), at)]);
        assert_eq!(c.state, State::Held);
    }

    #[test]
    fn a_note_that_is_not_there_is_not_held() {
        let db = db_with("Notes/p.md", "P", "", json!({}));
        assert!(describe(&db, "trash_node", &json!({ "node_id": "Notes/nope.md" })).is_err());
        assert!(describe(&db, "rename_kind", &json!({})).is_err(), "only what can be shown");
    }

    #[test]
    fn the_same_call_is_held_once() {
        let db = db_with("Notes/p.md", "P", "", json!({}));
        let mut set = Changeset::default();
        let args = json!({ "node_id": "Notes/p.md" });
        assert!(set.hold(describe(&db, "trash_node", &args).unwrap()));
        assert!(!set.hold(describe(&db, "trash_node", &args).unwrap()));
        assert_eq!(set.changes.len(), 1);
        assert!(set.awaiting());
    }

    #[test]
    fn what_was_read_is_named_as_a_person_would() {
        let step = |tool: &str, args: Value| crate::syn::run::Step {
            index: 0,
            kind: crate::syn::run::StepKind::ToolCall,
            iteration: 0,
            tool: Some(tool.into()),
            args: Some(args),
            ok: Some(true),
            reversal: None,
            preview: String::new(),
            full: None,
            tokens: None,
            usage: Default::default(),
            ms: 0,
            at: String::new(),
        };
        let read = read_from(&[
            step("browse", json!({ "what": "https://vnexpress.net/a/b" })),
            step("read_file_text", json!({ "path": "Files/2026/bao-cao.pdf" })),
            step("browse", json!({ "what": "https://vnexpress.net/c" })),
            step("query_nodes", json!({ "query": "x" })),
        ]);
        assert_eq!(read, vec!["vnexpress.net".to_string(), "bao-cao.pdf".to_string()]);
    }
}
