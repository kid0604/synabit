//! What the app-lock PIN keeps from Syn.
//!
//! # The gap this closes
//!
//! The app lock (`commands::app_lock`) can protect a whole mini-app —
//! `protected_apps`, ids like `finance` and `people` — or single notes —
//! `protected_notes`, vault-relative paths like `Notes/diary.md`, which is what
//! a node's id is. Until this module, only the lock screen read those lists.
//! Syn read straight from the database, so a note somebody had locked could be
//! searched, read, summarised and sent to a hosted model — from the app, from
//! Telegram, from a routine nobody was watching.
//!
//! Notes stay plaintext on disk, and that is accepted: the PIN is a lock on the
//! app's window, not encryption. What it now also is, is a lock on Syn.
//!
//! # The rule
//!
//! When a model is the caller (`ToolContext::model` is set), Syn neither reads
//! nor changes:
//!
//! - a **protected note** — by its path, its stable id, or a block inside it;
//! - any node of a type owned by a **protected app** (`app_owns_type`), nor
//!   uses the tools that reach an app's own store (`app_tools`: finance's
//!   transactions, feed articles, file text, boards, the Safe).
//!
//! Locked things are left out of listings and searches as if absent, titles
//! included — a title is often the most telling line of a diary. A direct read
//! or write is refused with a message the model can pass on ("that is locked"),
//! naming nothing it did not already say. Retrieval before the turn
//! (`rag::retrieve_context`), the timeline block, and the instant-count sample
//! leave them out the same way.
//!
//! # "Unlocked in this session" does not count
//!
//! The front end keeps which notes and apps were unlocked, and for how long, in
//! its own store (`useAppLockStore.ts`); the backend never learns it. So for Syn
//! a protected thing is always locked — the simplest default that is safe. It
//! is also the right one for Telegram and routines whatever the front end
//! knows: those surfaces are not the person at the keyboard who typed the PIN.
//! If the app surface should ever see an unlocked note, the front end would
//! have to say so with the request, and only `Surface::App` should listen.
//!
//! # The app's own code is not a model
//!
//! A screen calling a tool on the user's behalf (`model: None` — Nexus, a
//! Telegram reminder's "done" button, accepting a proposal) is the user
//! acting, and is not filtered here.
//!
//! # Cost
//!
//! The lists live in the keychain, and a keychain read is a system call that
//! can take milliseconds. They are read once and kept in memory until
//! `SecretManager::update_secrets` writes anything, which calls [`forget`].
//! Resolving protected paths to every name a node goes by is one small query,
//! made once per tool call or retrieval.
//!
//! When the keychain cannot be read, nothing is cached and nothing is hidden
//! for that call — the same answer the lock screen gets from
//! `get_app_lock_config`, which treats an unreadable store as empty. Hiding the
//! whole vault instead would make Syn useless on a machine whose keychain
//! asked a question nobody answered.

use std::collections::HashSet;
use std::sync::{Arc, OnceLock, RwLock};

use serde_json::Value;

use crate::db::DbBridge;

/// What the lock screen protects, as stored.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Locks {
    apps: Vec<String>,
    notes: Vec<String>,
}

impl Locks {
    pub fn new(apps: Vec<String>, notes: Vec<String>) -> Self {
        let notes = notes.iter().map(|n| normalise(n).to_string()).filter(|n| !n.is_empty()).collect();
        Self { apps, notes }
    }

    pub fn none() -> Self {
        Self::default()
    }

    pub fn is_empty(&self) -> bool {
        self.apps.is_empty() && self.notes.is_empty()
    }

    pub fn app_locked(&self, app: &str) -> bool {
        self.apps.iter().any(|a| a == app)
    }

    /// Whether a node of this type belongs to a locked app.
    pub fn hides_type(&self, node_type: &str) -> bool {
        self.apps.iter().any(|app| app_owns_type(app, node_type))
    }

    /// The locked app a tool belongs to, if it belongs to one.
    pub fn locked_app_of_tool(&self, tool: &str) -> Option<&str> {
        self.apps
            .iter()
            .find(|app| app_tools(app).contains(&tool))
            .map(String::as_str)
    }
}

/// Whether nodes of `node_type` are what app `app` keeps.
///
/// The ids are `BUILT_IN_APPS` in `src/shared/appRegistry.ts`; the types follow
/// `APP_FOR_OPEN_TYPE` in `src/shared/appAccess.ts`, which says which app opens
/// which node, widened to the types that app keeps without opening them on
/// their own (an interaction belongs to People, a project to Tasks).
///
/// Not here, on purpose:
/// - `messages` is Syn's own window. Locking it says who may open the chat, not
///   what Syn may read — and hiding Syn's memories and skills from Syn would
///   leave nothing to answer with.
/// - `nexus` is a view over everything else and keeps nothing of its own.
pub fn app_owns_type(app: &str, node_type: &str) -> bool {
    match app {
        "note" => matches!(node_type, "note" | "moment"),
        "quickcap" => node_type == "quickcap",
        "task" => matches!(node_type, "task" | "project" | "filter"),
        "calendar" => node_type == "event",
        "whiteboard" => node_type == "whiteboard",
        "people" => matches!(node_type, "person" | "interaction"),
        "finance" => node_type.starts_with("finance_"),
        "feeds" => node_type == "feed_source",
        "file" => matches!(node_type, "file" | "pdf" | "pdf_highlight" | "pdf_drawing"),
        "things" => node_type == "view",
        _ => false,
    }
}

/// The tools that reach an app's store rather than nodes, so the type rule
/// alone would not cover them.
fn app_tools(app: &str) -> &'static [&'static str] {
    match app {
        "finance" => &[
            "get_finance_summary",
            "search_finance",
            "get_transactions",
            "create_transaction",
            "update_transaction",
            "delete_transaction",
        ],
        "feeds" => &["search_feed_articles", "read_feed_article", "update_feed_article"],
        "file" => &["search_files", "read_file_text", "read_spreadsheet", "write_spreadsheet"],
        "whiteboard" => &["read_board", "draw_board", "edit_board"],
        "safe" => &["safe_list", "safe_request", "safe_health"],
        _ => &[],
    }
}

/// A path as the tools and the lock screen both write it.
fn normalise(s: &str) -> &str {
    let s = s.trim();
    let s = s.strip_prefix("./").unwrap_or(s);
    s.trim_start_matches('/')
}

// ── The lists, read once ──────────────────────────────────────────────

static CONFIG: RwLock<Option<Arc<Locks>>> = RwLock::new(None);
/// Every name the protected notes were last seen going by, for the readers that
/// have no database to ask (`timeline::asked::block`, `tempo::sample`).
/// Kept with the list of notes they were resolved from, so a list that has
/// changed since is not answered with the old names.
static KNOWN_IDS: RwLock<Option<(Vec<String>, Arc<HashSet<String>>)>> = RwLock::new(None);
/// The real app, for the platforms whose secrets cannot be read without it.
static HANDLE: OnceLock<tauri::AppHandle> = OnceLock::new();

#[cfg(test)]
thread_local! {
    static OVERRIDE: std::cell::RefCell<Option<Arc<Locks>>> = const { std::cell::RefCell::new(None) };
}

/// Remember the app, so the lists can be read on a phone too.
pub fn know_handle(app: &tauri::AppHandle) {
    let _ = HANDLE.set(app.clone());
}

/// The same, from a handle generic over the runtime: only the real one is kept.
pub fn know_handle_of<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    if HANDLE.get().is_none() {
        if let Some(real) = (app as &dyn std::any::Any).downcast_ref::<tauri::AppHandle>() {
            know_handle(real);
        }
    }
}

/// Drop what was read, so the next reader reads the keychain again. Called
/// whenever the secrets are written.
pub fn forget() {
    *CONFIG.write().unwrap_or_else(|e| e.into_inner()) = None;
    *KNOWN_IDS.write().unwrap_or_else(|e| e.into_inner()) = None;
}

/// What is locked, right now.
pub fn current() -> Arc<Locks> {
    #[cfg(test)]
    {
        OVERRIDE.with(|o| o.borrow().clone()).unwrap_or_else(|| Arc::new(Locks::none()))
    }
    #[cfg(not(test))]
    {
        if let Some(locks) = CONFIG.read().unwrap_or_else(|e| e.into_inner()).as_ref() {
            return locks.clone();
        }
        match load() {
            Some(locks) => {
                let locks = Arc::new(locks);
                *CONFIG.write().unwrap_or_else(|e| e.into_inner()) = Some(locks.clone());
                locks
            }
            None => Arc::new(Locks::none()),
        }
    }
}

#[cfg(not(test))]
fn load() -> Option<Locks> {
    let handle = HANDLE.get();
    // A phone keeps its secrets where only the app handle reaches; without it
    // the store answers "nothing stored", which must not be cached as the truth.
    if cfg!(mobile) && handle.is_none() {
        return None;
    }
    match crate::secrets::SecretManager::try_app_lock_lists(handle) {
        Ok(Some((apps, notes))) => Some(Locks::new(apps, notes)),
        Ok(None) => Some(Locks::none()),
        Err(e) => {
            log::warn!("[Syn Locks] Could not read the app lock: {e}");
            None
        }
    }
}

/// Run `f` as if `locks` were what the lock screen protects. Tests only: the
/// real lists live in this machine's keychain, which a test must never read.
#[cfg(test)]
pub fn with_locks<T>(locks: Locks, f: impl FnOnce() -> T) -> T {
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            OVERRIDE.with(|o| *o.borrow_mut() = None);
            *KNOWN_IDS.write().unwrap_or_else(|e| e.into_inner()) = None;
        }
    }
    OVERRIDE.with(|o| *o.borrow_mut() = Some(Arc::new(locks)));
    let _reset = Reset;
    f()
}

// ── Resolved against the vault ────────────────────────────────────────

/// The locks, with every protected note's names looked up.
#[derive(Debug, Clone)]
pub struct Hidden {
    locks: Arc<Locks>,
    /// Paths and stable ids of the protected notes.
    ids: HashSet<String>,
    /// The types those notes are, so a bulk change to a whole kind can be
    /// refused when it would reach one of them.
    note_types: HashSet<String>,
}

/// The keys a node's id travels under in a tool's result.
const ID_KEYS: &[&str] = &[
    "id",
    "node_id",
    "source_id",
    "container_node",
    "was_at",
    "trash_path",
    "original_path",
    "board",
];
/// The keys a node's type travels under.
const TYPE_KEYS: &[&str] = &["type", "node_type", "item_type", "source_type"];
/// Counts beside a list, lowered when something is taken out of it.
const COUNT_KEYS: &[&str] = &["_returned", "_total", "total", "total_matches", "total_in_trash"];

/// The tools that change every node of a kind at once.
const BULK_KIND_TOOLS: &[&str] = &["rename_field", "delete_field", "rename_kind", "delete_kind"];

impl Hidden {
    /// The locks as they stand, resolved against `db`.
    pub fn now(db: &DbBridge) -> Self {
        Self::resolve(current(), db)
    }

    pub fn resolve(locks: Arc<Locks>, db: &DbBridge) -> Self {
        let mut ids: HashSet<String> = locks.notes.iter().cloned().collect();
        let mut note_types = HashSet::new();
        if !locks.notes.is_empty() {
            let marks = (1..=locks.notes.len()).map(|i| format!("?{i}")).collect::<Vec<_>>().join(", ");
            let sql = format!(
                "SELECT id, node_type, stable_id, CASE WHEN json_valid(properties) THEN COALESCE(json_extract(properties, '$.node_id'), '') END \
                   FROM nodes WHERE id IN ({marks}) OR stable_id IN ({marks})"
            );
            let found = db.conn().prepare(&sql).and_then(|mut stmt| {
                stmt.query_map(rusqlite::params_from_iter(locks.notes.iter()), |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, Option<String>>(2)?.unwrap_or_default(),
                        row.get::<_, Option<String>>(3)?.unwrap_or_default(),
                    ))
                })?
                .collect::<Result<Vec<_>, _>>()
            });
            match found {
                Ok(rows) => {
                    for (id, node_type, stable, prop) in rows {
                        ids.insert(id);
                        note_types.insert(node_type);
                        for other in [stable, prop] {
                            if !other.trim().is_empty() {
                                ids.insert(other);
                            }
                        }
                    }
                }
                Err(e) => log::warn!("[Syn Locks] Could not look up the locked notes: {e}"),
            }
            *KNOWN_IDS.write().unwrap_or_else(|e| e.into_inner()) =
                Some((locks.notes.clone(), Arc::new(ids.clone())));
        }
        Self { locks, ids, note_types }
    }

    /// The locks as they stand, for a reader with no database: the protected
    /// paths, and whatever other names they were last seen going by.
    pub fn without_db() -> Self {
        let locks = current();
        let mut ids: HashSet<String> = locks.notes.iter().cloned().collect();
        if let Some((notes, known)) = KNOWN_IDS.read().unwrap_or_else(|e| e.into_inner()).as_ref() {
            if !locks.notes.is_empty() && *notes == locks.notes {
                ids.extend(known.iter().cloned());
            }
        }
        Self { locks, ids, note_types: HashSet::new() }
    }

    pub fn is_empty(&self) -> bool {
        self.locks.is_empty()
    }

    pub fn locks(&self) -> &Locks {
        &self.locks
    }

    pub fn hides_type(&self, node_type: &str) -> bool {
        self.locks.hides_type(node_type)
    }

    /// Whether this string names a protected note: its path, a stable id, a
    /// block inside it (`path#block`), or its place in the trash.
    pub fn hides_ref(&self, s: &str) -> bool {
        if self.ids.is_empty() {
            return false;
        }
        let s = normalise(s);
        let s = s.strip_prefix(".trash/").unwrap_or(s);
        let s = s.split('#').next().unwrap_or(s);
        self.ids.contains(s)
    }

    /// Whether the node `id` — of `node_type`, when that is known — is locked.
    /// With a database, an id whose type was not given is looked up.
    pub fn hides(&self, db: Option<&DbBridge>, id: &str, node_type: Option<&str>) -> bool {
        if self.hides_ref(id) {
            return true;
        }
        if self.locks.apps.is_empty() {
            return false;
        }
        match node_type.filter(|t| *t != "block") {
            Some(t) => self.hides_type(t),
            None => db
                .and_then(|db| type_of(db, normalise(id).split('#').next().unwrap_or(id)))
                .is_some_and(|t| self.hides_type(&t)),
        }
    }

    /// Whether any protected note's name appears anywhere in `text`.
    pub fn mentioned_in(&self, text: &str) -> bool {
        self.ids.iter().any(|id| text.contains(id.as_str()))
    }

    /// Why a model may not make this call, or `None` when it may.
    pub fn refuse(&self, db: &DbBridge, tool: &str, args: &Value) -> Option<String> {
        if self.is_empty() {
            return None;
        }
        if let Some(app) = self.locks.locked_app_of_tool(tool) {
            return Some(app_refusal(app));
        }
        if BULK_KIND_TOOLS.contains(&tool) {
            let kind = ["node_type", "from", "kind"]
                .iter()
                .find_map(|k| args.get(*k).and_then(Value::as_str))
                .map(str::trim)
                .unwrap_or("");
            if self.hides_type(kind) || self.note_types.contains(kind) {
                return Some(LOCKED.to_string());
            }
        }
        if self.arg_is_locked(db, None, args, 0) {
            return Some(LOCKED.to_string());
        }
        None
    }

    fn arg_is_locked(&self, db: &DbBridge, key: Option<&str>, v: &Value, depth: usize) -> bool {
        if depth > 6 {
            return false;
        }
        match v {
            Value::String(s) if s.len() <= 1024 => {
                if self.hides_ref(s) {
                    return true;
                }
                if self.locks.apps.is_empty() {
                    return false;
                }
                if key.is_some_and(|k| TYPE_KEYS.contains(&k)) {
                    return self.hides_type(s.trim());
                }
                // Anything shaped like a path may be a node of a locked app.
                (s.contains('/') || s.ends_with(".md") || s.ends_with(".json"))
                    && type_of(db, normalise(s).split('#').next().unwrap_or(s)).is_some_and(|t| self.hides_type(&t))
            }
            Value::Array(items) => items.iter().any(|i| self.arg_is_locked(db, key, i, depth + 1)),
            Value::Object(map) => map.iter().any(|(k, i)| self.arg_is_locked(db, Some(k), i, depth + 1)),
            _ => false,
        }
    }

    /// A tool's result with everything locked taken out.
    ///
    /// JSON is walked: an object that names a locked node by id or type is
    /// dropped from the list it is in (and the counts beside the list lowered),
    /// or, when it is the whole result, the result becomes a refusal. A result
    /// that is not JSON is refused whole if it names a protected note at all.
    pub fn filter_result(&self, db: &DbBridge, out: String) -> String {
        if self.is_empty() {
            return out;
        }
        match serde_json::from_str::<Value>(&out) {
            Ok(mut v) => {
                if self.object_is_locked(db, &v) {
                    return refusal_json(LOCKED);
                }
                if self.prune(db, &mut v, 0) {
                    v.to_string()
                } else {
                    // Untouched, so exactly as the tool wrote it.
                    out
                }
            }
            Err(_) if self.mentioned_in(&out) => refusal_json(LOCKED),
            Err(_) => out,
        }
    }

    fn object_is_locked(&self, db: &DbBridge, v: &Value) -> bool {
        let Some(map) = v.as_object() else { return false };
        // An error already says nothing about what is behind it.
        if map.contains_key("error") {
            return false;
        }
        let node_type = TYPE_KEYS.iter().find_map(|k| map.get(*k).and_then(Value::as_str));
        if node_type.is_some_and(|t| self.hides_type(t)) {
            return true;
        }
        ID_KEYS
            .iter()
            .filter_map(|k| map.get(*k).and_then(Value::as_str))
            .any(|id| self.hides(Some(db), id, node_type))
    }

    /// Whether anything was taken out.
    fn prune(&self, db: &DbBridge, v: &mut Value, depth: usize) -> bool {
        if depth > 8 {
            return false;
        }
        let mut changed = false;
        match v {
            Value::Array(items) => {
                let before = items.len();
                items.retain(|i| !self.object_is_locked(db, i));
                changed |= items.len() != before;
                for i in items.iter_mut() {
                    changed |= self.prune(db, i, depth + 1);
                }
            }
            Value::Object(map) => {
                let mut removed = 0usize;
                for (_, child) in map.iter_mut() {
                    match child {
                        Value::Array(items) => {
                            let before = items.len();
                            items.retain(|i| !self.object_is_locked(db, i));
                            removed += before - items.len();
                        }
                        other if self.object_is_locked(db, other) => {
                            *other = Value::Null;
                            changed = true;
                        }
                        _ => {}
                    }
                }
                if removed > 0 {
                    changed = true;
                    for key in COUNT_KEYS {
                        if let Some(n) = map.get(*key).and_then(Value::as_u64) {
                            map.insert(key.to_string(), Value::from(n.saturating_sub(removed as u64)));
                        }
                    }
                }
                for child in map.values_mut() {
                    changed |= self.prune(db, child, depth + 1);
                }
            }
            _ => {}
        }
        changed
    }
}

/// A node's type, without reading its content.
fn type_of(db: &DbBridge, id: &str) -> Option<String> {
    db.conn()
        .query_row(
            "SELECT node_type FROM nodes WHERE id = ?1 OR stable_id = ?1 LIMIT 1",
            [id],
            |r| r.get::<_, String>(0),
        )
        .ok()
}

/// What a model is told about a locked note. It names nothing, so it says
/// nothing the model did not already have.
pub const LOCKED: &str = "That is locked with the app PIN. Syn cannot read, search or change \
     locked notes or apps. Tell the person it is locked; they can open it in the app themselves.";

fn app_refusal(app: &str) -> String {
    format!(
        "The `{app}` app is locked with the app PIN, so Syn cannot use it. Tell the person it is \
         locked; they can open it in the app themselves."
    )
}

pub fn refusal_json(why: &str) -> String {
    serde_json::json!({ "error": why, "locked": true }).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::node::NodeMetadata;

    fn node(id: &str, node_type: &str, title: &str, content: &str, props: Value) -> NodeMetadata {
        NodeMetadata {
            id: id.into(),
            node_type: node_type.into(),
            title: title.into(),
            content: content.into(),
            properties: props,
            created_at: "2026-10-01T00:00:00Z".into(),
            updated_at: "2026-10-01T00:00:00Z".into(),
            timestamp: 0,
            blocks: None,
        }
    }

    fn vault() -> DbBridge {
        let db = DbBridge::new_in_memory_full().expect("schema");
        db.upsert_node(&node("Notes/diary.md", "note", "Diary", "secret", serde_json::json!({ "node_id": "uuid-diary" })))
            .unwrap();
        db.upsert_node(&node("Notes/open.md", "note", "Open", "fine", serde_json::json!({}))).unwrap();
        db.upsert_node(&node("People/mai.md", "person", "Mai", "", serde_json::json!({}))).unwrap();
        db
    }

    fn locks(apps: &[&str], notes: &[&str]) -> Arc<Locks> {
        Arc::new(Locks::new(
            apps.iter().map(|s| s.to_string()).collect(),
            notes.iter().map(|s| s.to_string()).collect(),
        ))
    }

    #[test]
    fn a_protected_note_is_hidden_by_every_name_it_goes_by() {
        let db = vault();
        let hidden = Hidden::resolve(locks(&[], &["Notes/diary.md"]), &db);
        for name in ["Notes/diary.md", "/Notes/diary.md", "./Notes/diary.md", "uuid-diary", "Notes/diary.md#blk1", ".trash/Notes/diary.md"] {
            assert!(hidden.hides_ref(name), "{name}");
        }
        assert!(!hidden.hides_ref("Notes/open.md"));
        assert!(hidden.note_types.contains("note"));
    }

    #[test]
    fn a_locked_app_hides_its_types_and_its_tools() {
        let l = locks(&["finance", "people"], &[]);
        assert!(l.hides_type("finance_month"));
        assert!(l.hides_type("person"));
        assert!(l.hides_type("interaction"));
        assert!(!l.hides_type("note"));
        assert_eq!(l.locked_app_of_tool("get_transactions"), Some("finance"));
        assert_eq!(l.locked_app_of_tool("query_nodes"), None);
        // Syn's own window keeps nothing Syn may not read.
        assert!(!locks(&["messages"], &[]).hides_type("syn_memory"));
    }

    #[test]
    fn results_lose_locked_rows_and_their_counts() {
        let db = vault();
        let hidden = Hidden::resolve(locks(&["people"], &["Notes/diary.md"]), &db);
        let out = serde_json::json!({
            "results": [
                { "id": "Notes/diary.md", "type": "note", "title": "Diary" },
                { "id": "Notes/open.md", "type": "note", "title": "Open" },
                { "id": "People/mai.md", "title": "Mai" },
            ],
            "total_matches": 3,
            "_returned": 3,
        });
        let kept: Value = serde_json::from_str(&hidden.filter_result(&db, out.to_string())).unwrap();
        assert_eq!(kept["results"].as_array().unwrap().len(), 1);
        assert_eq!(kept["results"][0]["id"], "Notes/open.md");
        assert_eq!(kept["total_matches"], 1);
        assert_eq!(kept["_returned"], 1);
        assert!(!kept.to_string().contains("Diary"));
    }

    #[test]
    fn a_whole_result_about_a_locked_note_becomes_a_refusal() {
        let db = vault();
        let hidden = Hidden::resolve(locks(&[], &["Notes/diary.md"]), &db);
        let got = hidden.filter_result(&db, serde_json::json!({ "id": "Notes/diary.md", "content": "secret" }).to_string());
        assert!(!got.contains("secret"));
        assert!(got.contains("\"locked\":true"));
        // Text that names it is refused whole.
        assert!(hidden.filter_result(&db, "File: Notes/diary.md\nsecret".into()).contains("locked"));
        assert_eq!(hidden.filter_result(&db, "nothing here".into()), "nothing here");
    }

    #[test]
    fn calls_that_name_a_locked_thing_are_refused() {
        let db = vault();
        let hidden = Hidden::resolve(locks(&["people", "finance"], &["Notes/diary.md"]), &db);
        assert!(hidden.refuse(&db, "get_node", &serde_json::json!({ "node_id": "Notes/diary.md" })).is_some());
        assert!(hidden.refuse(&db, "update_node", &serde_json::json!({ "node_ids": ["Notes/open.md", "uuid-diary"] })).is_some());
        assert!(hidden.refuse(&db, "get_node", &serde_json::json!({ "node_id": "People/mai.md" })).is_some());
        assert!(hidden.refuse(&db, "create_node", &serde_json::json!({ "node_type": "person", "title": "x" })).is_some());
        assert!(hidden.refuse(&db, "get_transactions", &serde_json::json!({})).is_some());
        // A bulk change to the kind a locked note is.
        assert!(hidden.refuse(&db, "delete_kind", &serde_json::json!({ "node_type": "note" })).is_some());
        // Everything else goes through.
        assert!(hidden.refuse(&db, "get_node", &serde_json::json!({ "node_id": "Notes/open.md" })).is_none());
        assert!(hidden.refuse(&db, "query_nodes", &serde_json::json!({ "query": "type:note" })).is_none());
    }

    #[test]
    fn nothing_locked_changes_nothing() {
        let db = vault();
        let hidden = Hidden::resolve(locks(&[], &[]), &db);
        let out = serde_json::json!({ "id": "Notes/diary.md", "content": "secret" }).to_string();
        assert_eq!(hidden.filter_result(&db, out.clone()), out);
        assert!(hidden.refuse(&db, "get_node", &serde_json::json!({ "node_id": "Notes/diary.md" })).is_none());
    }
}
