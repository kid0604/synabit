//! Asking the timeline what happened. See `crate::timeline`.

use std::sync::Arc;

use chrono::Datelike;

use crate::db::DbState;
use crate::error::{AppError, AppResult};
use crate::timeline::frame::{self, TimeFrame};
use crate::timeline::store::{self, CatchUp, Snapshot, Event};
use crate::timeline::quiet::{self, Hush, Quiet, Subject};
use crate::timeline::asking::{self, Question};
use crate::timeline::pin::{self, Pin};
use crate::timeline::{blocks, extract, magnitude, media, presence, reader, reflect, when, TimelineState};

fn quiet_of(state: &DbState, vault_path: &str) -> AppResult<Arc<Quiet>> {
    let db = state.lock().unwrap_or_else(|e| e.into_inner());
    quiet::current(&db, vault_path)
}

/// What a view of `when` holds, and what it had no room for.
#[derive(Debug, serde::Serialize)]
pub struct Looked {
    pub items: Vec<Event>,
    /// How many were left out because the view is too wide to show them.
    ///
    /// Said out loud rather than swallowed: §10 draws the gaps instead of
    /// skipping them, and a person who cannot tell "nothing happened" from
    /// "too much happened to list" has been lied to by omission.
    pub too_small: usize,
    /// How many of those shown are there because the person put them there.
    pub pinned: usize,
}

/// Everything the vault says happened during `when`.
///
/// `when` is any time the timeline reads: `2016-05-14`, `2016-05`, `2016`,
/// `2016-05-01/2016-06-30` or `~2012`. The timeline catches up with the vault
/// first, and only reads the vault again if something in it changed.
///
/// The width of `when` is the zoom (§4.5): a month or less shows everything,
/// and wider views keep only the biggest — plus everything pinned by hand,
/// which is the whole point of pinning. `all` asks for the lot regardless.
#[tauri::command]
pub fn timeline_query(
    state: tauri::State<'_, DbState>,
    timeline: tauri::State<'_, TimelineState>,
    vault_path: String,
    when: String,
    all: Option<bool>,
) -> AppResult<Looked> {
    let span = when::parse(&when).ok_or_else(|| {
        AppError::General(format!(
            "'{when}' is not a time the timeline can read. {}",
            when::HOW_TO_WRITE_ONE
        ))
    })?;

    let mut timeline = timeline.lock().unwrap_or_else(|e| e.into_inner());
    store::catch_up_in(state.inner(), &mut timeline, Some(&vault_path))?;
    let mut items = timeline.query(span, chrono::Local::now().date_naive())?;

    let pinned = pin::read(&vault_path);
    let held = items.iter().filter(|item| pinned.holds(item)).count();
    let room = if all.unwrap_or(false) { None } else { magnitude::room_for(span.days()) };
    let Some(room) = room else {
        return Ok(Looked { items, too_small: 0, pinned: held });
    };

    let (items, too_small) = pin::keep(items, room, &pinned);
    Ok(Looked { items, too_small, pinned: held })
}

/// Lift one thing above the threshold, for good — §4.5, rule 3.
#[tauri::command]
pub fn timeline_pin(vault_path: String, node_id: String, day: String) -> AppResult<Pin> {
    pin::write(&vault_path, &node_id, &day)
}

/// Let it fall back to its measured size.
#[tauri::command]
pub fn timeline_unpin(vault_path: String, id: String) -> AppResult<()> {
    pin::remove(&vault_path, &id)
}

/// Everything the person has lifted by hand.
#[tauri::command]
pub fn timeline_pins(vault_path: String) -> AppResult<Vec<Pin>> {
    Ok(pin::read(&vault_path).pins().to_vec())
}


/// Stop offering this day's writing for a year — the one action §7.1 asks for.
///
/// A year, because the next time it could come back is its next anniversary,
/// and the person has already said no to this one.
#[tauri::command]
pub fn timeline_not_again(vault_path: String, node_id: String, day: String) -> AppResult<Hush> {
    let until = when::parse(&day)
        .and_then(|span| span.from.with_year(span.from.year() + 1))
        .map(when::iso)
        .ok_or_else(|| AppError::General(format!("'{day}' is not a day")))?;
    quiet::write_hush(
        &vault_path,
        &Subject::Moment { node: node_id, day },
        Some(&until),
    )
}

/// How long somebody set aside stays set aside.
///
/// §7.2: waved away once, gone for months. Six, because the shortest quiet
/// this feature will speak about at all is three (`SHORTEST_WORTH_SAYING`),
/// and a pause shorter than the thing it is pausing would be no pause.
const SET_ASIDE_DAYS: i64 = 183;

/// Leave somebody alone for a while — §7.2's last "must never".
#[tauri::command]
pub fn timeline_set_aside(vault_path: String, who: String) -> AppResult<Hush> {
    let until = when::iso(
        chrono::Local::now().date_naive() + chrono::Duration::days(SET_ASIDE_DAYS),
    );
    quiet::write_hush(&vault_path, &Subject::Person { who }, Some(&until))
}


/// Leave one sentence out of the year, for good.
#[tauri::command]
pub fn timeline_drop_line(vault_path: String, node_id: String, text: String) -> AppResult<Hush> {
    quiet::write_hush(
        &vault_path,
        &Subject::Line { node: node_id, line: quiet::line_id(&text) },
        None,
    )
}

/// The one thing to ask about now, if there is one — §7.4.
///
/// Asking is a write: §7.4 says a thing is not asked about twice in a year
/// whether or not the person answers, so seeing the question is what counts.
/// The reply is a question, never a verdict, and nothing here is counted into
/// any total.
#[tauri::command]
pub fn timeline_ask(
    state: tauri::State<'_, DbState>,
    timeline: tauri::State<'_, TimelineState>,
    vault_path: String,
) -> AppResult<Option<Question>> {
    let mut timeline = timeline.lock().unwrap_or_else(|e| e.into_inner());
    store::catch_up_in(state.inner(), &mut timeline, Some(&vault_path))?;
    let quiet = quiet_of(state.inner(), &vault_path)?;
    let db = state.lock().unwrap_or_else(|e| e.into_inner());
    asking::ask(&timeline, &db, &vault_path, chrono::Local::now().date_naive(), &quiet)
}

/// Derive the whole timeline again from the vault cache.
///
/// Never needed for correctness: a change to the vault is picked up on the
/// next query. It is here for after a change to how items are derived, and
/// for anyone who wants to see it happen.
#[tauri::command]
pub fn timeline_rebuild(
    state: tauri::State<'_, DbState>,
    timeline: tauri::State<'_, TimelineState>,
) -> AppResult<CatchUp> {
    let mut timeline = timeline.lock().unwrap_or_else(|e| e.into_inner());
    let snapshot = {
        let db = state.lock().unwrap_or_else(|e| e.into_inner());
        Snapshot::read(&db)?
    };
    let report = timeline.rebuild(&snapshot)?;
    log::info!(
        "timeline rebuilt: {} nodes read, {} items",
        report.nodes_read,
        report.items
    );
    Ok(report)
}

/// When everything in the graph entered the person's life, for Nexus to
/// travel back through. See `timeline::frame`.
///
/// Sealed periods come back as periods to draw, and what happened in them
/// does not, unless `reveal` asks for it. That is the "show for now" on the
/// strip: it lasts for one look, and nothing is written.
#[tauri::command]
pub fn timeline_frame(
    state: tauri::State<'_, DbState>,
    timeline: tauri::State<'_, TimelineState>,
    vault_path: String,
    reveal: Option<bool>,
) -> AppResult<TimeFrame> {
    let mut timeline = timeline.lock().unwrap_or_else(|e| e.into_inner());
    store::catch_up_in(state.inner(), &mut timeline, Some(&vault_path))?;
    let today = chrono::Local::now().date_naive();
    let items = timeline.all_items(today)?;
    let nodes = {
        let db = state.lock().unwrap_or_else(|e| e.into_inner());
        frame::read_nodes(&db)?
    };
    let _ = reveal;
    Ok(frame::build(&items, &nodes, today))
}


/// What the timeline holds about one node, under its path and its identity.
#[tauri::command]
pub fn timeline_about(
    state: tauri::State<'_, DbState>,
    timeline: tauri::State<'_, TimelineState>,
    vault_path: String,
    node_id: String,
) -> AppResult<Vec<Event>> {
    let mut timeline = timeline.lock().unwrap_or_else(|e| e.into_inner());
    store::catch_up_in(state.inner(), &mut timeline, Some(&vault_path))?;
    let identity: Option<String> = {
        let db = state.lock().unwrap_or_else(|e| e.into_inner());
        db.conn()
            .query_row("SELECT stable_id FROM nodes WHERE id = ?1", [&node_id], |r| {
                r.get::<_, Option<String>>(0)
            })
            .ok()
            .flatten()
    };
    let mut names = vec![node_id.as_str()];
    if let Some(identity) = identity.as_deref().filter(|id| *id != node_id) {
        names.push(identity);
    }
    let items = timeline.about(&names, chrono::Local::now().date_naive())?;
    let db = state.lock().unwrap_or_else(|e| e.into_inner());
    let path_of = person_paths(&db)?;
    drop(db);
    Ok(one_side_of_each_relationship(items, &names, &path_of))
}

/// Every person's path, under its path and its identity.
fn person_paths(db: &crate::db::DbBridge) -> AppResult<std::collections::HashMap<String, String>> {
    let mut stmt = db
        .conn()
        .prepare("SELECT id, COALESCE(NULLIF(stable_id, ''), id) FROM nodes WHERE node_type = 'person'")
        .map_err(|e| AppError::General(format!("timeline: {e}")))?;
    let rows = stmt
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
        .map_err(|e| AppError::General(format!("timeline: {e}")))?;
    let mut out = std::collections::HashMap::new();
    for (path, identity) in rows.flatten() {
        out.insert(identity, path.clone());
        out.insert(path.clone(), path);
    }
    Ok(out)
}

/// A relationship written on both people is one relationship. This person's
/// own side of it is kept, with its own label; two different people over the
/// same years stay two.
pub(crate) fn one_side_of_each_relationship(
    mut items: Vec<Event>,
    names: &[&str],
    path_of: &std::collections::HashMap<String, String>,
) -> Vec<Event> {
    let resolve = |name: &str| path_of.get(name).cloned().unwrap_or_else(|| name.to_string());
    let mine = |item: &Event| names.contains(&item.node_id.as_str());
    items.sort_by_key(|item| !mine(item));
    let mut seen = std::collections::HashSet::new();
    items.retain(|item| {
        if item.kind != "connection" {
            return true;
        }
        let other = if mine(item) {
            item.links
                .iter()
                .find(|link| link.role == "with")
                .map(|link| resolve(&link.node_id))
                .unwrap_or_default()
        } else {
            resolve(&item.node_id)
        };
        seen.insert((other, item.happened_from.clone(), item.happened_to.clone()))
    });
    items.sort_by(|a, b| {
        a.happened_from
            .cmp(&b.happened_from)
            .then_with(|| a.kind.cmp(&b.kind))
            .then_with(|| a.id.cmp(&b.id))
    });
    items
}

/// A person the app has gone quiet about, and why.
///
/// Whether anyone asked is the whole of it: a hush is undone by lifting it, and
/// a death is not undone at all. A screen that showed them the same way would
/// offer to "un-quiet" somebody's father.
#[derive(Debug, serde::Serialize)]
pub struct Hushed {
    pub hushes: Vec<Hush>,
    /// People quiet because they have a `died_on`, by path. §7.3.
    pub dead: Vec<String>,
}

/// Everything the app has been told, or has worked out, not to raise first.
#[tauri::command]
pub fn timeline_quiet(state: tauri::State<'_, DbState>, vault_path: String) -> AppResult<Hushed> {
    let quiet = quiet_of(&state, &vault_path)?;
    let mut dead: Vec<String> = quiet
        .dead_people()
        .filter(|who| who.contains('/'))
        .map(str::to_string)
        .collect();
    dead.sort();
    Ok(Hushed { hushes: quiet.hushes().to_vec(), dead })
}

/// Ask the app to stop raising a person, one day's writing, or a stretch of
/// time — the one action §16 Bước 3 asks for, and it is remembered because it
/// is a file in the vault.
///
/// `until` is the last day it holds; without one it holds for good.
#[tauri::command]
pub fn timeline_hush(
    vault_path: String,
    who: Option<String>,
    node: Option<String>,
    day: Option<String>,
    from: Option<String>,
    to: Option<String>,
    until: Option<String>,
) -> AppResult<Hush> {
    let text = |value: Option<String>| {
        value.map(|v| v.trim().to_string()).filter(|v| !v.is_empty())
    };
    let subject = match (text(who), text(node), text(day), text(from), text(to)) {
        (Some(who), ..) => Subject::Person { who },
        (_, Some(node), Some(day), ..) => Subject::Moment { node, day },
        (_, _, _, Some(from), Some(to)) => Subject::Period { from, to },
        _ => {
            return Err(AppError::General(
                "Say what to go quiet about: a person, a note and a day, or a stretch of time"
                    .into(),
            ))
        }
    };
    quiet::write_hush(&vault_path, &subject, text(until).as_deref())
}

/// Let the app speak about this again.
#[tauri::command]
pub fn timeline_unhush(vault_path: String, id: String) -> AppResult<()> {
    quiet::remove_hush(&vault_path, &id)
}

/// Record what has changed in the vault since this device last looked.
///
/// Async, because the first sweep of a vault fingerprints every file in it,
/// attachments included, and that must not hold the window.
#[tauri::command(async)]
pub fn ledger_sweep(
    app_handle: tauri::AppHandle,
    ledger: tauri::State<'_, crate::timeline::ledger::LedgerDb>,
    vault_path: String,
) -> AppResult<crate::timeline::ledger::SweepReport> {
    let device = crate::commands::sync::ensure_device_id(&app_handle).map_err(AppError::General)?;
    // The ledger's own connection: a first sweep takes minutes, and nothing
    // that reads the timeline should wait for it.
    let conn = ledger.0.lock().unwrap_or_else(|e| e.into_inner());
    let report = crate::timeline::ledger::sweep(&conn, &vault_path, &device, chrono::Utc::now())?;
    if report.recorded + report.changed + report.removed > 0 {
        log::info!(
            "ledger: {} recorded, {} changed, {} removed ({} fingerprinted)",
            report.recorded,
            report.changed,
            report.removed,
            report.hashed
        );
    }
    Ok(report)
}

/// What every device's ledger says about one file.
#[tauri::command(async)]
pub fn ledger_history(
    ledger: tauri::State<'_, crate::timeline::ledger::LedgerDb>,
    vault_path: String,
    rel_path: String,
) -> crate::timeline::ledger::FileHistory {
    let conn = ledger.0.lock().unwrap_or_else(|e| e.into_inner());
    crate::timeline::ledger::history_with(Some(&conn), &vault_path, &rel_path)
}

/// Whether each device's ledger is intact, and if not, from which entry.
#[tauri::command(async)]
pub fn ledger_verify(
    ledger: tauri::State<'_, crate::timeline::ledger::LedgerDb>,
    vault_path: String,
) -> Vec<crate::timeline::ledger::ChainReport> {
    let conn = ledger.0.lock().unwrap_or_else(|e| e.into_inner());
    crate::timeline::ledger::verify_with(Some(&conn), &vault_path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(kind: &str, node_id: &str, from: &str) -> Event {
        Event {
            id: format!("{node_id}#{kind}"),
            kind: kind.into(),
            node_id: node_id.into(),
            node_type: String::new(),
            title: String::new(),
            node_title: String::new(),
            links: Vec::new(),
            magnitude: 0.0,
            container_node: None,
            props: serde_json::Value::Null,
            happened_from: from.into(),
            happened_to: from.into(),
            precision: "day".into(),
            time_source: "frontmatter".into(),
            source: "derived".into(),
            shape: crate::timeline::derive::Shape::Occasion,
        }
    }

}

// ─── Reading notes into proposals (Nhát E) ───────────────────────

/// One reading at a time: two would read the same notes twice and pay for both.
static EXTRACTING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

struct Extracting;

impl Drop for Extracting {
    fn drop(&mut self) {
        EXTRACTING.store(false, std::sync::atomic::Ordering::SeqCst);
    }
}

/// What is left to read, by day, and who is who — `timeline::reader`.
///
/// The vault's identity is looked up before the cache is locked: it takes the
/// same lock, which is not reentrant. Each note's history comes from its Loro
/// document, where there is one; a note never saved here has none, and its
/// blocks take the day it was made.
fn reading_plan<R: tauri::Runtime>(
    app_handle: &tauri::AppHandle<R>,
    state: &DbState,
    timeline: &crate::timeline::store::TimelineStore,
    vault_path: &str,
    config: &extract::Config,
    settled_before: Option<chrono::DateTime<chrono::Utc>>,
) -> AppResult<(reader::Plan, reader::Directory)> {
    let vault_id = crate::sync::core::identity::load_or_register_vault_identity(app_handle, vault_path)
        .map(|identity| identity.vault_id.to_string())
        .ok();
    let today = chrono::Local::now().date_naive();
    let db = state.lock().unwrap_or_else(|e| e.into_inner());
    let history = |rel: &str| -> blocks::History {
        let Some(vault_id) = vault_id.as_deref() else { return blocks::History::default() };
        match db.get_node_id_by_path(vault_id, rel) {
            Ok(Some(doc_id)) => db
                .get_crdt_doc(vault_id, &doc_id)
                .map(|doc| blocks::History::of(&doc))
                .unwrap_or_default(),
            _ => blocks::History::default(),
        }
    };
    reader::plan_in(&db, timeline.conn(), vault_path, config, today, settled_before, &history)
}

/// A note as it is now, for the tray to judge what was read from it by: at
/// the path it was read at, or — moved since — wherever the quoted words are.
fn note_now(state: &DbState) -> impl Fn(&str, &str) -> Option<(String, extract::NoteNow)> + '_ {
    move |id: &str, quote: &str| {
        let db = state.lock().unwrap_or_else(|e| e.into_inner());
        let as_now = |node: crate::models::node::NodeMetadata| {
            (node.id, extract::NoteNow { title: node.title, node_type: node.node_type, content: node.content })
        };
        if let Some(node) = db.get_node(id).ok().flatten() {
            return Some(as_now(node));
        }
        // Words long enough to find it by, from the quote as the model wrote
        // it. The candidates are then held to the whole quote.
        let words: String = quote.split_whitespace().take(5).collect::<Vec<_>>().join(" ");
        if words.chars().count() < 12 {
            return None;
        }
        let candidates: Vec<String> = db
            .conn()
            .prepare("SELECT id FROM nodes WHERE instr(content, ?1) > 0 AND node_type != 'moment' LIMIT 5")
            .and_then(|mut stmt| stmt.query_map([&words], |r| r.get(0)).map(|rows| rows.flatten().collect()))
            .unwrap_or_default();
        candidates
            .into_iter()
            .filter_map(|id| db.get_node(&id).ok().flatten())
            .find(|node| extract::find_quote(&node.content, quote).is_some())
            .map(as_now)
    }
}

/// How reading is set up, and what is waiting to be decided about.
///
/// Everything in here is answered from the timeline's own tables and the
/// vault's index: tens of milliseconds. **How much is left to read** is not —
/// that means walking every note and replaying its history — and it lives in
/// `ReadingLeft`, asked for separately. They used to be one answer, so opening
/// the settings, or the timeline screen, waited on the whole vault before it
/// could draw a checkbox.
#[derive(Debug, serde::Serialize)]
pub struct ExtractStatus {
    pub config: extract::Config,
    pub syn_enabled: bool,
    pub provider: String,
    pub local: bool,
    pub model: Option<String>,
    pub desktop: bool,
    pub running: bool,
    pub unreadable: Vec<String>,
    pub proposals: Vec<extract::Proposal>,
    /// Everybody in the vault, for saying who a name belongs to.
    pub people: Vec<extract::PersonRef>,
    /// The kinds a moment can be here: the vault's list, or the defaults
    /// while it has none. What the review offers, and what a reading is held
    /// to (`extract::Config::categories`).
    pub categories: Vec<String>,
    /// The moments the changes above are about, as they are kept now, so the
    /// review can show what would change.
    pub moments: std::collections::HashMap<String, MomentView>,
}

/// How much of the vault has not been read, and what that would cost.
///
/// Counted a day at a time and without opening a note — see `reader::Left`,
/// which says what that gives up. The block-by-block plan is worked out when
/// somebody presses "Read", and nowhere else: at ten thousand notes it is
/// fifteen seconds, and no screen may cost that to draw a number.
#[derive(Debug, Default, serde::Serialize)]
pub struct ReadingLeft {
    /// Days with writing on them that no reading at this version has covered.
    pub pending: usize,
    /// Days covered only by an older reader.
    pub old_version: usize,
    /// Blocks this reader has already read.
    pub done: usize,
    /// For the days never read.
    pub estimate_ms: u64,
    /// Measured on this device, rather than a starting guess.
    pub estimate_measured: bool,
}

/// A kept moment as the review shows it, beside the change proposed to it.
#[derive(Debug, Clone, serde::Serialize)]
pub struct MomentView {
    pub path: String,
    pub title: String,
    pub happened: String,
    pub people: Vec<String>,
    pub place: Option<String>,
    pub category: Option<String>,
    pub time: Option<String>,
    pub amount: Option<serde_json::Value>,
    /// Fields the person wrote themselves; a change never touches these.
    pub hand: Vec<String>,
}

#[tauri::command(async)]
pub fn timeline_extract_status(
    state: tauri::State<'_, DbState>,
    timeline: tauri::State<'_, TimelineState>,
    vault_path: String,
) -> AppResult<ExtractStatus> {
    let settings = crate::commands::syn::settings_for(&vault_path);
    let config = extract::read_config(&vault_path);

    let timeline = timeline.lock().unwrap_or_else(|e| e.into_inner());
    let loaded = extract::load(timeline.conn(), &vault_path)?;
    // Names, not a plan: the directory is one query of the vault's index and
    // the proposals are rows the timeline already holds. What is left to read
    // is `timeline_reading_left`, and it is not asked for here.
    let directory = {
        let db = state.lock().unwrap_or_else(|e| e.into_inner());
        reader::Directory::read(&db)?
    };
    let proposals = extract::proposals(
        timeline.conn(),
        &note_now(state.inner()),
        &extract::reviewed(&vault_path),
        &|id| directory.title(id).map(String::from),
    )?;
    let local = media::runs_here(&settings);

    let moments = {
        let wanted: std::collections::HashSet<&str> =
            proposals.iter().filter_map(|p| p.about_moment.as_deref()).collect();
        let db = state.lock().unwrap_or_else(|e| e.into_inner());
        crate::timeline::moments::kept(&db)?
            .into_iter()
            .filter(|moment| wanted.contains(moment.path.as_str()))
            .map(|moment| {
                let text = |key: &str| moment.fields.get(key).and_then(serde_json::Value::as_str).map(String::from);
                let view = MomentView {
                    path: moment.path.clone(),
                    title: moment.title.clone(),
                    happened: text("happened").unwrap_or_default(),
                    people: moment
                        .fields
                        .get("people")
                        .and_then(serde_json::Value::as_array)
                        .map(|list| {
                            list.iter()
                                .filter_map(|p| p.as_str())
                                .map(|id| directory.title(id).unwrap_or(id).to_string())
                                .collect()
                        })
                        .unwrap_or_default(),
                    place: text("where"),
                    category: text("category"),
                    time: text("time"),
                    amount: moment.fields.get("amount").cloned(),
                    hand: moment.hand.clone(),
                };
                (moment.path, view)
            })
            .collect()
    };

    Ok(ExtractStatus {
        categories: config.categories(),
        config,
        syn_enabled: settings.enabled,
        provider: settings.provider.key_slot().to_string(),
        local,
        model: settings.default_model.clone(),
        desktop: cfg!(desktop),
        running: EXTRACTING.load(std::sync::atomic::Ordering::SeqCst),
        unreadable: loaded.unreadable,
        proposals,
        people: directory
            .people
            .iter()
            .map(|person| extract::PersonRef { id: person.id.clone(), title: person.name.clone() })
            .collect(),
        moments,
    })
}

/// How much of the vault is left to read.
///
/// Its own command because it is its own question: what reading is set up to
/// do comes from tables the timeline already holds, and this is about the
/// vault. Two queries and no note opened — 7 ms on a vault of a thousand
/// nodes, 56 ms on one of thirteen thousand — so a screen can ask for it
/// without earning a spinner.
#[tauri::command]
pub fn timeline_reading_left(
    app_handle: tauri::AppHandle,
    state: tauri::State<'_, DbState>,
    timeline: tauri::State<'_, TimelineState>,
    vault_path: String,
) -> AppResult<ReadingLeft> {
    let settings = crate::commands::syn::settings_for(&vault_path);
    let config = extract::read_config(&vault_path);
    let device = crate::commands::sync::ensure_device_id(&app_handle).map_err(AppError::General)?;

    let timeline = timeline.lock().unwrap_or_else(|e| e.into_inner());
    extract::load(timeline.conn(), &vault_path)?;
    let left = {
        let db = state.lock().unwrap_or_else(|e| e.into_inner());
        reader::left_to_read(&db, timeline.conn(), &config, chrono::Local::now().date_naive())?
    };
    let local = media::runs_here(&settings);
    let (estimate_ms, estimate_measured) = extract::estimate_ms(timeline.conn(), &device, left.chars, local)?;

    Ok(ReadingLeft {
        pending: left.days,
        old_version: left.old_version,
        done: left.done,
        estimate_ms,
        estimate_measured,
    })
}

#[derive(Debug, serde::Deserialize)]
pub struct ExtractSettings {
    pub enabled: bool,
    pub allow_cloud: bool,
    #[serde(default)]
    pub folders: Vec<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub conversations: bool,
    /// The kinds a moment can be in this vault. Empty means the defaults.
    #[serde(default)]
    pub categories: Vec<String>,
}

/// Turn reading on or off for this vault, and say what it may read.
#[tauri::command]
pub fn timeline_extract_configure(vault_path: String, settings: ExtractSettings) -> AppResult<extract::Config> {
    let mut config = extract::read_config(&vault_path);
    config.enabled = settings.enabled;
    config.allow_cloud = settings.allow_cloud;
    config.folders = settings.folders;
    config.tags = settings.tags;
    config.conversations = settings.conversations;
    // Kept as the person wrote them, less the blanks and the duplicates. The
    // list is theirs; the reader is held to it (`Config::categories`).
    let mut kinds: Vec<String> = Vec::new();
    for kind in &settings.categories {
        let kind = kind.trim().to_lowercase();
        if !kind.is_empty() && !kinds.contains(&kind) {
            kinds.push(kind);
        }
    }
    config.categories = kinds;
    extract::write_config(&vault_path, &mut config, chrono::Utc::now())?;
    Ok(config)
}

/// Read notes into proposals.
///
/// `scope` is `new` (never read), `stale` (edited since), `old` (read by an
/// older extractor) or `all`. An automatic run reads only `new`, only notes
/// left alone for a while, a few at a time, never on a phone, and quietly
/// does nothing when reading is off; asked for by hand, each of those refusals
/// says why instead.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn timeline_extract_run(
    app_handle: tauri::AppHandle,
    state: tauri::State<'_, DbState>,
    timeline: tauri::State<'_, TimelineState>,
    vault_path: String,
    scope: String,
    auto: bool,
    limit: Option<usize>,
) -> AppResult<extract::ExtractRun> {
    let skip = |why: &str| {
        Ok(extract::ExtractRun {
            skipped: Some(why.to_string()),
            ..extract::ExtractRun::default()
        })
    };
    let refuse = |why: &str| Err(AppError::General(why.to_string()));

    let config = extract::read_config(&vault_path);
    if !config.enabled {
        return if auto { skip("off") } else { refuse("Reading notes into the timeline is switched off for this vault") };
    }
    if auto && cfg!(mobile) {
        return skip("phone");
    }
    let settings = crate::commands::syn::settings_for(&vault_path);
    if !settings.enabled {
        return if auto { skip("syn_off") } else { refuse(crate::commands::syn::SWITCHED_OFF) };
    }
    // Model đọc nhật ký có thể khác model trợ lý, và khi nó khác thì nó chạy
    // trên máy này. Xem `extract::reader` và §8.6.
    let (settings, model) = extract::reader(&config, &settings);
    if !media::runs_here(&settings) && !config.allow_cloud {
        return if auto {
            skip("cloud")
        } else {
            refuse("Notes are read only by a model on this machine unless sending them elsewhere is allowed for this vault")
        };
    }
    let Some(model) = model else {
        return if auto { skip("no_model") } else { refuse("No model is configured") };
    };
    if EXTRACTING.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return skip("running");
    }
    let _running = Extracting;

    let device = crate::commands::sync::ensure_device_id(&app_handle).map_err(AppError::General)?;
    let now = chrono::Utc::now();
    let settled_before = auto.then(|| now - chrono::Duration::minutes(extract::SETTLE_MINUTES));

    let (work, changes, directory, remaining) = {
        let timeline = timeline.lock().unwrap_or_else(|e| e.into_inner());
        extract::load(timeline.conn(), &vault_path)?;
        let (plan, directory) =
            reading_plan(&app_handle, state.inner(), &timeline, &vault_path, &config, settled_before)?;
        let asked_for_changes = matches!(scope.as_str(), "new" | "all");
        let changes = if asked_for_changes { plan.changes.clone() } else { Vec::new() };
        let mut work = match (scope.as_str(), auto) {
            ("new", _) => plan.pending,
            (_, true) => return skip("scope"),
            // Edits are read as new blocks; there is nothing else "stale".
            ("stale", false) => Vec::new(),
            ("old", false) => plan.old_version,
            ("all", false) => [plan.pending, plan.old_version].concat(),
            (other, false) => return refuse(&format!("'{other}' is not a scope: use new, stale, old or all")),
        };
        // A day that keeps failing rests before an automatic run tries it
        // again, and waits behind the rest when asked for by hand.
        if auto {
            work.retain(|bag| !extract::resting(&reader::failure_key(bag)));
        } else {
            work.sort_by_key(|bag| extract::resting(&reader::failure_key(bag)));
        }
        let limit = limit.unwrap_or(if auto { extract::AUTO_LIMIT } else { usize::MAX });
        // A moment whose source changed under it is asked about first: it is
        // about something the person already decided to keep.
        let changes: Vec<reader::Change> = changes.into_iter().take(limit).collect();
        let left = limit.saturating_sub(changes.len());
        let remaining = work.len().saturating_sub(left);
        work.truncate(left);
        (work, changes, directory, remaining)
    };

    if work.is_empty() && changes.is_empty() {
        return Ok(extract::ExtractRun::default());
    }

    let provider = crate::commands::syn::provider_for(&app_handle, &settings).await;
    let mut report =
        reader::read_all(provider.as_ref(), &model, settings.num_ctx, &work, &directory, &vault_path, &device).await;
    let about_changes =
        reader::read_all_changes(provider.as_ref(), &model, settings.num_ctx, &changes, &directory, &vault_path, &device)
            .await;
    report.read += about_changes.read;
    report.items += about_changes.items;
    report.failed.extend(about_changes.failed);
    report.remaining = remaining;
    log::info!(
        "timeline extract: read {}, {} proposals, {} left out, {} failed, {} remaining",
        report.read,
        report.items,
        report.dropped,
        report.failed.len(),
        report.remaining
    );

    let timeline = timeline.lock().unwrap_or_else(|e| e.into_inner());
    // Why things were left out is worth keeping, not worth failing a run over.
    if let Err(e) = extract::remember_drops(timeline.conn(), &report.drops) {
        log::warn!("timeline extract: could not keep what was left out: {e}");
    }
    extract::load(timeline.conn(), &vault_path)?;
    Ok(report)
}

/// Who a name belongs to, said by the person in the review.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct Assigned {
    /// The name as the note wrote it: "Cam", "chị Yến".
    pub name: String,
    /// The person's node: `People/<uuid>.md`.
    pub node: String,
}

/// Keep a proposal, or decline it.
///
/// Keeping writes the moment to a file of its own, `Moments/<uuid>.md`, which
/// names the note it was read from (`timeline::moments`). The note itself is
/// not touched. Declining writes only the decision, so no device offers it
/// again.
///
/// Every field is the person's to put right first (`extract::Edits`), and what
/// they put right is remembered twice over: the fields they wrote are marked
/// `hand` in the moment, so a later reading never proposes over them (§15.2),
/// and the correction itself is kept for the next reading to be told about
/// (§8.2). A name they say belongs to somebody becomes that person's alias, so
/// nobody is asked who "Cam" is twice.
///
/// A proposal that is a **change** to a moment already kept (§15) is applied
/// to that moment instead of writing a new one; accepting one that says the
/// words are gone is how a moment is let go, and it goes to the trash rather
/// than being deleted.
#[tauri::command(async)]
#[allow(clippy::too_many_arguments)]
pub fn timeline_extract_review(
    app_handle: tauri::AppHandle,
    state: tauri::State<'_, DbState>,
    timeline: tauri::State<'_, TimelineState>,
    vault_path: String,
    item_id: String,
    accept: bool,
    node_id: Option<String>,
    edits: Option<extract::Edits>,
    assigned: Option<Vec<Assigned>>,
) -> AppResult<()> {
    let device = crate::commands::sync::ensure_device_id(&app_handle).map_err(AppError::General)?;
    let proposed = {
        let timeline = timeline.lock().unwrap_or_else(|e| e.into_inner());
        extract::load(timeline.conn(), &vault_path)?;
        extract::item(timeline.conn(), &item_id)?
    }
    .ok_or_else(|| AppError::General(format!("No proposal {item_id}")))?;

    let edits = edits.unwrap_or_default();
    let (item, hand) = extract::as_kept(&proposed, &edits)?;
    // Where the note is now, as the tray saw it: a note moved since it was
    // read is kept into at its new path.
    let node = node_id
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| item.evidence.first().map(|e| e.node.clone()).unwrap_or_default());
    let now = chrono::Utc::now();
    let decision = |verdict: &str, moment: Option<extract::Extracted>| extract::Decision {
        item: item.id.clone(),
        decision: verdict.to_string(),
        node: node.clone(),
        at: crate::utils::timestamp::canonical(now),
        moment,
    };

    if !accept {
        extract::decide(&vault_path, &device, decision("declined", None), now)?;
        let timeline = timeline.lock().unwrap_or_else(|e| e.into_inner());
        extract::load(timeline.conn(), &vault_path)?;
        return Ok(());
    }

    let base = node.split('#').next().unwrap_or_default();

    match proposed.about_moment.as_deref() {
        // A change to a moment already kept.
        Some(path) => {
            let kept = {
                let db = state.lock().unwrap_or_else(|e| e.into_inner());
                crate::timeline::moments::kept(&db)?.into_iter().find(|moment| moment.path == path)
            }
            .ok_or_else(|| AppError::General(format!("{path} is no longer in the vault")))?;

            match proposed.verdict.as_deref() {
                Some("retracted") | Some("gone") => {
                    // Letting it go, to the trash: a moment kept once was a
                    // decision, and undoing a decision is not deleting a file.
                    let db = state.lock().unwrap_or_else(|e| e.into_inner());
                    crate::commands::trash::apply_trash(&db, &vault_path, path)?;
                }
                _ => {
                    let serde_json::Value::Object(mut fields) = extract::moment_entry_with(&item, &hand) else {
                        return Err(AppError::General("a moment is an object".into()));
                    };
                    fields.remove("id");
                    fields.remove("extract");
                    // What the person wrote in the review is theirs from now on,
                    // as much as what they wrote when they kept it.
                    let mut theirs = kept.hand.clone();
                    theirs.extend(hand.iter().cloned());
                    theirs.sort();
                    theirs.dedup();
                    fields.insert("hand".into(), serde_json::json!(theirs));
                    let source = item.evidence.first().map(|e| (e.node.clone(), e.hash.clone(), item.payload.quote.clone()));
                    crate::timeline::moments::apply(
                        &app_handle,
                        state.inner(),
                        &vault_path,
                        &kept,
                        fields,
                        source.as_ref().map(|(node, hash, quote)| (node.as_str(), hash.as_str(), quote.as_str())),
                    )?;
                }
            }
        }
        // Something new.
        None => {
            if !base.starts_with("Syn/") {
                let existing = {
                    let db = state.lock().unwrap_or_else(|e| e.into_inner());
                    db.get_node(base)?
                }
                .ok_or_else(|| AppError::General(format!("{base} is no longer in the vault")))?;
                if !extract::still_reads_as_read(&existing.content, &item) {
                    return Err(AppError::General(
                        "The words this was read from are no longer in the note, so nothing shows it happened.".into(),
                    ));
                }
            }
            let serde_json::Value::Object(entry) = extract::moment_entry_with(&item, &hand) else {
                return Err(AppError::General("a moment is an object".into()));
            };
            let id = entry.get("id").and_then(serde_json::Value::as_str).unwrap_or_default().to_string();
            crate::timeline::moments::write(
                &app_handle,
                state.inner(),
                &vault_path,
                &id,
                crate::timeline::moments::frontmatter(
                    &entry,
                    Some(base),
                    Some(&item.payload.quote),
                    item.evidence.first().map(|e| e.hash.as_str()),
                ),
            )?;
        }
    }

    learn_from(&app_handle, state.inner(), &timeline, &vault_path, &proposed, &item, assigned.unwrap_or_default())?;
    extract::decide(&vault_path, &device, decision("accepted", None), now)?;

    let timeline = timeline.lock().unwrap_or_else(|e| e.into_inner());
    extract::load(timeline.conn(), &vault_path)?;
    Ok(())
}

/// Keep what the person put right, so the next reading is told (§8.2).
///
/// A name they said belongs to somebody becomes that person's alias in their
/// own note — which is where a nickname belongs, and what makes the next
/// reading resolve it without being told again.
fn learn_from(
    app_handle: &tauri::AppHandle,
    state: &DbState,
    timeline: &tauri::State<'_, TimelineState>,
    vault_path: &str,
    proposed: &extract::Extracted,
    kept: &extract::Extracted,
    assigned: Vec<Assigned>,
) -> AppResult<()> {
    let mut learned: Vec<reader::Correction> = Vec::new();
    if kept.payload.title != proposed.payload.title {
        learned.push(reader::Correction {
            field: "title".into(),
            before: proposed.payload.title.clone(),
            after: kept.payload.title.clone(),
        });
    }
    if kept.payload.category != proposed.payload.category {
        if let Some(after) = kept.payload.category.clone() {
            learned.push(reader::Correction {
                field: "category".into(),
                before: proposed.payload.category.clone().unwrap_or_default(),
                after,
            });
        }
    }

    for who in assigned {
        let (name, node) = (who.name.trim().to_string(), who.node.trim().to_string());
        if name.is_empty() || node.is_empty() {
            continue;
        }
        let person = {
            let db = state.lock().unwrap_or_else(|e| e.into_inner());
            db.get_node(&node)?
        };
        let Some(person) = person.filter(|node| node.node_type == "person") else {
            continue;
        };
        let abs = crate::path_utils::resolve_safe_path(vault_path, &node).map_err(|e| AppError::General(e.to_string()))?;
        let on_disk = crate::commands::nodes::existing_properties(&abs, "md");
        let mut aliases: Vec<String> = match on_disk.get("aliases") {
            Some(serde_json::Value::Array(list)) => list.iter().filter_map(|a| a.as_str()).map(String::from).collect(),
            Some(serde_json::Value::String(one)) => vec![one.clone()],
            _ => Vec::new(),
        };
        if aliases.iter().any(|alias| alias.eq_ignore_ascii_case(&name)) || person.title.eq_ignore_ascii_case(&name) {
            continue;
        }
        aliases.push(name.clone());
        crate::commands::nodes::write_node_inner(
            app_handle,
            state,
            vault_path.to_string(),
            node.clone(),
            person.title.clone(),
            "person".to_string(),
            serde_json::json!({ "aliases": aliases }),
            None,
        )?;
        learned.push(reader::Correction { field: "person".into(), before: name, after: person.title.clone() });
    }

    if learned.is_empty() {
        return Ok(());
    }
    let timeline = timeline.lock().unwrap_or_else(|e| e.into_inner());
    for correction in &learned {
        // Worth keeping, not worth failing the keep over.
        if let Err(e) = reader::remember_correction(timeline.conn(), correction) {
            log::warn!("timeline reader: could not keep a correction: {e}");
        }
    }
    Ok(())
}

// ─── A moment already kept (§4.5) ────────────────────────────────

/// One kept moment, as the sheet that edits it needs it.
#[derive(Debug, Clone, serde::Serialize)]
pub struct MomentDetail {
    pub path: String,
    pub title: String,
    pub happened_from: String,
    pub happened_to: String,
    pub precision: String,
    pub time: Option<String>,
    pub place: Option<String>,
    pub category: Option<String>,
    pub amount: Option<serde_json::Value>,
    pub about: Vec<String>,
    /// Who took part, as far as anybody knows who they are…
    pub people: Vec<extract::PersonRef>,
    /// …and the names nobody has claimed yet.
    pub names: Vec<String>,
    /// Fields the person wrote themselves. Everything they edit here joins it.
    pub hand: Vec<String>,
    /// The note it was read from, and the words, when it was read from one.
    pub source_node: Option<String>,
    pub quote: Option<String>,
    pub origin: Option<String>,
    /// The kinds this vault keeps moments in, for the picker.
    pub categories: Vec<String>,
    /// Everybody, for saying who a name belongs to.
    pub known_people: Vec<extract::PersonRef>,
}

/// A moment as it is kept, for the sheet that edits it.
///
/// Read from the vault's cache rather than the timeline index: what is being
/// edited is the file, and the index is only a reading of it.
#[tauri::command(async)]
pub fn timeline_moment(
    state: tauri::State<'_, DbState>,
    vault_path: String,
    path: String,
) -> AppResult<MomentDetail> {
    let config = extract::read_config(&vault_path);
    let db = state.lock().unwrap_or_else(|e| e.into_inner());
    let moment = crate::timeline::moments::one(&db, &path)?
        .ok_or_else(|| AppError::General(format!("{path} is not a moment this vault holds")))?;
    let directory = reader::Directory::read(&db)?;

    let text = |key: &str| moment.fields.get(key).and_then(serde_json::Value::as_str).map(String::from);
    let span = text("happened").as_deref().and_then(when::parse);
    let (mut people, mut names) = (Vec::new(), Vec::new());
    if let Some(serde_json::Value::Array(list)) = moment.fields.get("people") {
        for who in list.iter().filter_map(serde_json::Value::as_str) {
            match directory.title(who) {
                Some(title) => people.push(extract::PersonRef { id: who.to_string(), title: title.to_string() }),
                None => names.push(who.to_string()),
            }
        }
    }
    Ok(MomentDetail {
        happened_from: span.map(|s| when::iso(s.from)).unwrap_or_default(),
        happened_to: span.map(|s| when::iso(s.to)).unwrap_or_default(),
        precision: span.map(|s| s.precision.as_str().to_string()).unwrap_or_else(|| "day".into()),
        title: moment.title.clone(),
        time: text("time"),
        place: text("where"),
        category: text("category"),
        amount: moment.fields.get("amount").cloned(),
        about: moment
            .fields
            .get("about")
            .and_then(serde_json::Value::as_array)
            .map(|list| list.iter().filter_map(|a| a.as_str()).map(String::from).collect())
            .unwrap_or_default(),
        people,
        names,
        hand: moment.hand.clone(),
        source_node: moment.source_node.clone(),
        quote: moment.quote.clone(),
        origin: text("origin"),
        path: moment.path.clone(),
        categories: config.categories(),
        known_people: directory
            .people
            .iter()
            .map(|person| extract::PersonRef { id: person.id.clone(), title: person.name.clone() })
            .collect(),
    })
}

/// Change a moment already kept.
///
/// Whatever the person writes here is theirs: every field they change joins
/// `hand`, so no later reading of the note proposes over it (§15.2). The
/// moment's own file is written; the note it was read from is not touched.
#[tauri::command(async)]
pub fn timeline_moment_write(
    app_handle: tauri::AppHandle,
    state: tauri::State<'_, DbState>,
    timeline: tauri::State<'_, TimelineState>,
    vault_path: String,
    path: String,
    edits: extract::Edits,
) -> AppResult<()> {
    let moment = {
        let db = state.lock().unwrap_or_else(|e| e.into_inner());
        crate::timeline::moments::one(&db, &path)?
    }
    .ok_or_else(|| AppError::General(format!("{path} is not a moment this vault holds")))?;

    let mut fields = serde_json::Map::new();
    if let Some(title) = edits.title.as_deref().map(str::trim) {
        if title.is_empty() {
            return Err(AppError::General("A moment with no title is not a moment".into()));
        }
        fields.insert("title".into(), serde_json::Value::String(title.to_string()));
    }
    if let Some(from) = edits.happened_from.as_deref().map(str::trim).filter(|d| !d.is_empty()) {
        let to = edits.happened_to.as_deref().map(str::trim).filter(|d| !d.is_empty()).unwrap_or(from);
        let (from, to) = if from <= to { (from, to) } else { (to, from) };
        let precision = edits.precision.clone().unwrap_or_else(|| if from == to { "day".into() } else { "range".into() });
        fields.insert("happened".into(), serde_json::Value::String(happened(&precision, from, to)));
    }
    let word = |value: Option<&String>| match value.map(|v| v.trim()) {
        Some(text) if !text.is_empty() => Some(serde_json::Value::String(text.to_string())),
        Some(_) => Some(serde_json::Value::Null),
        None => None,
    };
    for (key, value) in [("time", word(edits.time.as_ref())), ("where", word(edits.place.as_ref())), ("category", word(edits.category.as_ref()))] {
        if let Some(value) = value {
            fields.insert(key.into(), value);
        }
    }
    if let Some(people) = &edits.people {
        let people: Vec<String> = people.iter().map(|p| p.trim().to_string()).filter(|p| !p.is_empty()).collect();
        fields.insert("people".into(), serde_json::json!(people));
    }
    if let Some(about) = &edits.about {
        let about: Vec<String> = about.iter().map(|a| a.trim().to_string()).filter(|a| !a.is_empty()).collect();
        fields.insert("about".into(), serde_json::json!(about));
    }
    if let Some(value) = edits.amount {
        let unit = edits.unit.as_deref().map(str::trim).filter(|u| !u.is_empty()).unwrap_or("VND").to_uppercase();
        let amount = if value.is_finite() && value > 0.0 {
            serde_json::json!({ "value": value, "unit": unit })
        } else {
            serde_json::Value::Null
        };
        fields.insert("amount".into(), amount);
    }

    crate::timeline::moments::edit(&app_handle, state.inner(), &vault_path, &moment, fields)?;
    let mut timeline = timeline.lock().unwrap_or_else(|e| e.into_inner());
    store::catch_up_in(state.inner(), &mut timeline, Some(&vault_path))?;
    Ok(())
}

/// Let a moment go: its file to the trash, and off the timeline with it.
///
/// To the trash rather than deleted: keeping it was a decision, and undoing a
/// decision is not the same as destroying the record of it.
#[tauri::command(async)]
pub fn timeline_moment_delete(
    state: tauri::State<'_, DbState>,
    timeline: tauri::State<'_, TimelineState>,
    vault_path: String,
    path: String,
) -> AppResult<String> {
    let moved = {
        let db = state.lock().unwrap_or_else(|e| e.into_inner());
        if crate::timeline::moments::one(&db, &path)?.is_none() {
            return Err(AppError::General(format!("{path} is not a moment this vault holds")));
        }
        crate::commands::trash::apply_trash(&db, &vault_path, &path)?
    };
    let mut timeline = timeline.lock().unwrap_or_else(|e| e.into_inner());
    store::catch_up_in(state.inner(), &mut timeline, Some(&vault_path))?;
    Ok(moved)
}

/// What starting the timeline again would take away, before it takes any.
#[tauri::command(async)]
pub fn timeline_reset_plan(
    state: tauri::State<'_, DbState>,
    timeline: tauri::State<'_, TimelineState>,
    vault_path: String,
) -> AppResult<crate::timeline::reset::Plan> {
    let timeline = timeline.lock().unwrap_or_else(|e| e.into_inner());
    extract::load(timeline.conn(), &vault_path)?;
    let db = state.lock().unwrap_or_else(|e| e.into_inner());
    crate::timeline::reset::plan(&db, timeline.conn(), &vault_path)
}

/// Start the timeline again: every moment to the trash, every reading and
/// decision forgotten, the index emptied and rebuilt.
///
/// For when the timeline was read by a model that was not up to it. The notes
/// it was read from are untouched, the settings stay, and every file goes to
/// the trash rather than away — see `timeline::reset`.
#[tauri::command(async)]
pub fn timeline_reset(
    state: tauri::State<'_, DbState>,
    timeline: tauri::State<'_, TimelineState>,
    vault_path: String,
    expect_moments: Option<usize>,
) -> AppResult<crate::timeline::reset::Done> {
    let done = {
        let timeline = timeline.lock().unwrap_or_else(|e| e.into_inner());
        crate::timeline::reset::reset(state.inner(), timeline.conn(), &vault_path, expect_moments)?
    };
    let mut timeline = timeline.lock().unwrap_or_else(|e| e.into_inner());
    // Everything derived is gone with the index; this reads the vault again,
    // which now says nothing about moments.
    store::catch_up_in(state.inner(), &mut timeline, Some(&vault_path))?;
    log::info!(
        "timeline reset: {} moments to the trash, {} month files emptied, {} transcripts kept, {} failed",
        done.moments,
        done.month_files,
        done.surrogates_kept,
        done.failed.len()
    );
    Ok(done)
}

/// When a node was part of the life, in stretches. See `timeline::presence`.
///
/// Read off the events that name it, so a person nothing mentions comes back
/// with nothing rather than with a guess. A stretch that begins inside a
/// sealed period is not returned: the seal covers when somebody was there as
/// much as what they did.
#[tauri::command]
pub fn timeline_presence(
    state: tauri::State<'_, DbState>,
    timeline: tauri::State<'_, TimelineState>,
    vault_path: String,
    node_id: String,
) -> AppResult<Vec<presence::Presence>> {
    let mut timeline = timeline.lock().unwrap_or_else(|e| e.into_inner());
    store::catch_up_in(state.inner(), &mut timeline, Some(&vault_path))?;

    let identity: Option<String> = {
        let db = state.lock().unwrap_or_else(|e| e.into_inner());
        db.conn()
            .query_row("SELECT stable_id FROM nodes WHERE id = ?1", [&node_id], |r| {
                r.get::<_, Option<String>>(0)
            })
            .ok()
            .flatten()
    };
    let mut names = vec![node_id.as_str()];
    if let Some(identity) = identity.as_deref().filter(|id| *id != node_id) {
        names.push(identity);
    }


    // From the events themselves, through the same seal filter every other
    // read goes through: what is sealed never reaches the stretches, rather
    // than being subtracted from them afterwards.
    let today = chrono::Local::now().date_naive();
    let events: Vec<_> = timeline
        .about(&names, today)?
        .into_iter()
        .collect();
    let spans: Vec<(&str, &str)> = events
        .iter()
        .map(|event| (event.happened_from.as_str(), event.happened_to.as_str()))
        .collect();
    Ok(presence::merge(&spans, today))
}

// ─── Soạn một sự kiện (Bước 4) ───────────────────────────────────

/// One line, as the app understood it. Shown before anything is written.
#[derive(Debug, Clone, serde::Serialize, PartialEq)]
pub struct ComposedView {
    pub title: String,
    pub happened_from: String,
    pub happened_to: String,
    pub precision: String,
    /// False when the line named no time and the dates above are a guess.
    pub dated: bool,
    pub with: Vec<ComposedPerson>,
    pub place: Option<String>,
}

/// A name the line gave, and the node it turned out to be, if any.
#[derive(Debug, Clone, serde::Serialize, PartialEq)]
pub struct ComposedPerson {
    pub name: String,
    pub node_id: Option<String>,
}

/// What the model made of a typed line, before anything is written.
#[derive(Debug, Clone, Default, serde::Serialize, PartialEq)]
pub struct ComposedReply {
    /// What it understood. `None` when nothing could be read from the line.
    pub read: Option<ComposedView>,
    /// The model that read it. `None` means there is none to read with, and
    /// the box asks the person for the fields itself rather than guessing.
    pub model: Option<String>,
    /// Why nothing came back, in words meant for a person.
    pub refused: Option<String>,
}

/// Read one typed line as an event, and write nothing.
///
/// The reading is the model's, through the same guards the vault's own notes
/// go through (`timeline::extract`): the model must quote the line back, a
/// time it cannot read is refused rather than guessed, and something that has
/// not happened yet is not history. A hand-written rule parser stood here
/// first and was removed: `voi` and `với`, `tai` and `tại`, and a time at the
/// end of a sentence were all beyond it, and each patch invited the next.
#[tauri::command(async)]
pub async fn timeline_read_line(
    app_handle: tauri::AppHandle,
    state: tauri::State<'_, DbState>,
    vault_path: String,
    line: String,
) -> AppResult<ComposedReply> {
    let line = line.trim().to_string();
    let no_model = |why: Option<&str>| ComposedReply {
        read: None,
        model: None,
        refused: why.map(String::from),
    };

    let settings = crate::commands::syn::settings_for(&vault_path);
    if !settings.enabled {
        return Ok(no_model(None));
    }
    let config = extract::read_config(&vault_path);
    let (settings, model) = extract::reader(&config, &settings);
    let Some(model) = model else {
        return Ok(no_model(None));
    };
    // The same rule the vault's notes are read under: what you write stays on
    // this machine unless this vault says otherwise.
    if !media::runs_here(&settings) && !config.allow_cloud {
        return Ok(no_model(Some(
            "What you write is read only by a model on this machine, unless sending it elsewhere is allowed for this vault",
        )));
    }

    // An empty line is how the box asks whether there is a model at all,
    // before it has anything to read.
    if line.is_empty() {
        return Ok(ComposedReply { read: None, model: Some(model), refused: None });
    }

    // The same reading as a day's (§9): one line, on today, which is what a
    // line with no time of its own is about.
    let today = chrono::Local::now().date_naive();
    let directory = {
        let db = state.lock().unwrap_or_else(|e| e.into_inner());
        reader::Directory::read(&db)?
    };
    let Some(bag) = reader::bag_of_line(&line, today, &directory) else {
        return Ok(ComposedReply { read: None, model: Some(model), refused: None });
    };
    let provider = crate::commands::syn::provider_for(&app_handle, &settings).await;

    let (items, _, _) =
        reader::read_bag(provider.as_ref(), &model, settings.num_ctx, &bag, &directory, chrono::Utc::now()).await?;
    let Some(item) = items.into_iter().next() else {
        return Ok(ComposedReply {
            read: None,
            model: Some(model),
            refused: Some("Nothing in that line reads as something that happened".into()),
        });
    };

    let with = item
        .payload
        .people
        .iter()
        .map(|id| ComposedPerson { name: directory.title(id).unwrap_or(id).to_string(), node_id: Some(id.clone()) })
        .chain(item.payload.names.iter().map(|name| ComposedPerson { name: name.clone(), node_id: None }))
        .collect();

    Ok(ComposedReply {
        read: Some(ComposedView {
            // Dated by the words, not by default: a line that named no time
            // was put on today, and the person should see that was a guess.
            dated: item.payload.date_basis.as_deref() != Some("the_day"),
            title: item.payload.title,
            happened_from: item.happened_from,
            happened_to: item.happened_to,
            precision: item.precision,
            with,
            place: item.payload.place,
        }),
        model: Some(model),
        refused: None,
    })
}

/// Write a moment somebody entered by hand.
///
/// To a file of its own, `Moments/<uuid>.md`, the same as a kept proposal —
/// see `timeline::moments`. It used to go into `moments[]` in the day's note,
/// making the note if the day had none. Returns the moment's file.
///
/// Everything goes through `write_node_inner`, the one way a node reaches
/// disk.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub fn timeline_write_event(
    app_handle: tauri::AppHandle,
    state: tauri::State<'_, DbState>,
    timeline: tauri::State<'_, TimelineState>,
    vault_path: String,
    title: String,
    happened_from: String,
    happened_to: String,
    precision: String,
    with: Vec<String>,
    place: Option<String>,
    // What it was about: a project or anything else that is not a person and
    // not a place. §4.2's fourth role.
    about: Vec<String>,
) -> AppResult<String> {
    let written = write_event_inner(
        &app_handle,
        state.inner(),
        &vault_path,
        &title,
        &happened_from,
        &happened_to,
        &precision,
        &with,
        place.as_deref(),
        &about,
    )?;
    let mut timeline = timeline.lock().unwrap_or_else(|e| e.into_inner());
    store::catch_up_in(state.inner(), &mut timeline, Some(&vault_path))?;
    Ok(written)
}

/// The writing itself, apart from the command so it can be tested.
#[allow(clippy::too_many_arguments)]
pub(crate) fn write_event_inner<R: tauri::Runtime>(
    app_handle: &tauri::AppHandle<R>,
    state: &DbState,
    vault_path: &str,
    title: &str,
    happened_from: &str,
    happened_to: &str,
    precision: &str,
    with: &[String],
    place: Option<&str>,
    about: &[String],
) -> AppResult<String> {
    let title = title.trim().to_string();
    if title.is_empty() {
        return Err(AppError::General("An event needs something to call it by".into()));
    }
    let from = chrono::NaiveDate::parse_from_str(happened_from, "%Y-%m-%d")
        .map_err(|_| AppError::General(format!("'{happened_from}' is not a day")))?;

    let people: Vec<String> = {
        let db = state.lock().unwrap_or_else(|e| e.into_inner());
        with.iter()
            .map(|name| name.trim())
            .filter(|name| !name.is_empty())
            .map(|name| store::node_for(&db, name).unwrap_or_else(|| name.to_string()))
            .collect()
    };

    // What is sealed stays sealed, including from this side: an event about a
    // sealed person would be written into the vault and then hidden from the
    // person who wrote it.

    let mut moment = serde_json::Map::new();
    moment.insert("title".into(), serde_json::Value::String(title.clone()));
    moment.insert("happened".into(), serde_json::Value::String(happened(precision, happened_from, happened_to)));
    if !people.is_empty() {
        moment.insert("people".into(), serde_json::json!(people));
    }
    if let Some(place) = place.map(str::trim).filter(|p| !p.is_empty()) {
        moment.insert("where".into(), serde_json::Value::String(place.to_string()));
    }
    let about: Vec<&str> =
        about.iter().map(|name| name.trim()).filter(|name| !name.is_empty()).collect();
    if !about.is_empty() {
        moment.insert("about".into(), serde_json::json!(about));
    }

    let id = uuid::Uuid::new_v4().to_string();
    let rel_path = crate::timeline::moments::write(
        app_handle,
        state,
        vault_path,
        &id,
        crate::timeline::moments::frontmatter(&moment, None, None, None),
    )?;

    // Somebody you had lunch with was contacted that day. The interaction form
    // in People writes this; an event written here is the same fact coming in
    // by another door, and keep-in-touch reads it. Without this, recording
    // lunch through the new box left the person's ring saying "overdue".
    if from <= chrono::Local::now().date_naive() {
        for person in &people {
            let Some(node) = ({
                let db = state.lock().unwrap_or_else(|e| e.into_inner());
                db.get_node(person)?
            }) else {
                continue;
            };
            if node.node_type != "person" {
                continue;
            }
            let abs = crate::path_utils::resolve_safe_path(vault_path, person)
                .map_err(|e| AppError::General(e.to_string()))?;
            let on_disk = crate::commands::nodes::existing_properties(&abs, "md");
            let known = on_disk
                .get("last_contacted")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default();
            if happened_from <= known {
                continue;
            }
            crate::commands::nodes::write_node_inner(
                app_handle,
                state,
                vault_path.to_string(),
                person.clone(),
                on_disk
                    .get("title")
                    .and_then(serde_json::Value::as_str)
                    .map(String::from)
                    .unwrap_or_else(|| node.title.clone()),
                "person".to_string(),
                serde_json::json!({ "last_contacted": happened_from }),
                None,
            )?;
        }
    }

    Ok(rel_path)
}

/// `moments[].happened`, written the way `when::parse` reads it back.
fn happened(precision: &str, from: &str, to: &str) -> String {
    match precision {
        "day" => from.to_string(),
        "month" => from.get(..7).unwrap_or(from).to_string(),
        "year" => from.get(..4).unwrap_or(from).to_string(),
        _ if from == to => from.to_string(),
        _ => format!("{from}/{to}"),
    }
}

// ─── Chiêm nghiệm (Nhát F) ───────────────────────────────────────

/// Decisions, which are due a look back, and what they share, or only the
/// switch when reflection is off.
#[tauri::command(async)]
pub fn reflect_overview(state: tauri::State<'_, DbState>, vault_path: String) -> AppResult<reflect::Overview> {
    let config = reflect::read_config(&vault_path);
    if !config.enabled {
        return Ok(reflect::overview(&config, Vec::new()));
    }
    let db = state.lock().unwrap_or_else(|e| e.into_inner());
    let decisions = reflect::decisions(&db, chrono::Local::now().date_naive())?;
    Ok(reflect::overview(&config, decisions))
}

#[tauri::command]
pub fn reflect_configure(vault_path: String, enabled: bool) -> AppResult<()> {
    let mut config = reflect::read_config(&vault_path);
    config.enabled = enabled;
    reflect::write_config(&vault_path, &mut config, chrono::Utc::now())
}

/// Write a decision down: why, what is expected, and when to look again.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub fn reflect_create_decision(
    app_handle: tauri::AppHandle,
    state: tauri::State<'_, DbState>,
    vault_path: String,
    title: String,
    expected: String,
    reasoning: String,
    review_on: Option<String>,
    tags: Vec<String>,
) -> AppResult<String> {
    let title = title.trim();
    if title.is_empty() {
        return Err(AppError::General("A decision needs a name".into()));
    }
    let today = chrono::Local::now().date_naive();
    let properties = reflect::new_decision(today, &expected, review_on.as_deref(), &tags)?;
    let id = format!("{}/{}.md", reflect::FOLDER, uuid::Uuid::new_v4());
    crate::commands::nodes::write_node_inner(
        &app_handle,
        state.inner(),
        vault_path,
        id.clone(),
        title.to_string(),
        "decision".into(),
        properties,
        Some(reasoning),
    )?;
    Ok(id)
}

/// Answer "what actually happened?" for one decision.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub fn reflect_add_review(
    app_handle: tauri::AppHandle,
    state: tauri::State<'_, DbState>,
    vault_path: String,
    id: String,
    happened: String,
    outcome: Option<String>,
    next_review_on: Option<String>,
) -> AppResult<()> {
    let node = {
        let db = state.lock().unwrap_or_else(|e| e.into_inner());
        db.get_node(&id)?
    }
    .filter(|node| node.node_type == "decision")
    .ok_or_else(|| AppError::General(format!("No decision at {id}")))?;
    // From the file, not the cache: a look back saved on another device may
    // have arrived on disk and not been read in yet.
    let abs = crate::path_utils::resolve_safe_path(&vault_path, &id).map_err(|e| AppError::General(e.to_string()))?;
    let on_disk = crate::commands::nodes::existing_properties(&abs, "md");
    let title = on_disk.get("title").and_then(serde_json::Value::as_str).map(String::from).unwrap_or_else(|| node.title.clone());
    let properties = reflect::with_review(
        &serde_json::Value::Object(on_disk),
        chrono::Local::now().date_naive(),
        &happened,
        outcome.as_deref(),
        next_review_on.as_deref(),
    )?;
    // Only what looking back changes is written: the rest of the file stays as it is.
    let mut changed = serde_json::Map::new();
    for key in ["reviews", "review_on"] {
        if let Some(value) = properties.get(key) {
            changed.insert(key.into(), value.clone());
        }
    }
    crate::commands::nodes::write_node_inner(
        &app_handle,
        state.inner(),
        vault_path,
        id,
        title,
        node.node_type.clone(),
        serde_json::Value::Object(changed),
        None,
    )
}

/// What the decisions tagged `tag` share, told by Syn and checked here.
///
/// Nothing is asked of a model when reflection is off, when fewer than three
/// of them have been looked back on, or for anything sealed.
#[tauri::command]
pub async fn reflect_pattern(
    app_handle: tauri::AppHandle,
    state: tauri::State<'_, DbState>,
    vault_path: String,
    tag: String,
    locale: Option<String>,
) -> AppResult<reflect::Pattern> {
    if !reflect::read_config(&vault_path).enabled {
        return Err(AppError::General("Reflection is switched off for this vault".into()));
    }
    let settings = crate::commands::syn::settings_for(&vault_path);
    if !settings.enabled {
        return Err(AppError::General(crate::commands::syn::SWITCHED_OFF.to_string()));
    }
    let model = settings
        .default_model
        .clone()
        .ok_or_else(|| AppError::General("No model is configured".into()))?;

    let (tag, sources) = {
        let db = state.lock().unwrap_or_else(|e| e.into_inner());
        let decisions = reflect::decisions(&db, chrono::Local::now().date_naive())?;
        let Some(group) = reflect::groups(&decisions)
            .into_iter()
            .find(|group| group.tag.to_lowercase() == tag.trim().trim_start_matches('#').to_lowercase())
        else {
            return Err(AppError::General(format!("No decisions looked back on are tagged '{tag}'")));
        };
        if !group.enough {
            return Ok(reflect::Pattern {
                tag: group.tag,
                withheld: Some("not_enough_cases".into()),
                ..reflect::Pattern::default()
            });
        }
        let cases: Vec<&reflect::Decision> = decisions.iter().filter(|d| group.cases.contains(&d.id)).collect();
        (group.tag.clone(), reflect::sources(&cases))
    };

    let language = if locale.as_deref().unwrap_or("en").starts_with("vi") { "Vietnamese" } else { "English" };
    let provider = crate::commands::syn::provider_for(&app_handle, &settings).await;
    let messages = vec![crate::syn::provider::ChatMessage::new(
        "user",
        reflect::pattern_prompt(&tag, &sources, language),
    )];
    let reply = provider
        .chat(crate::syn::provider::ChatRequest {
            model: &model,
            messages: &messages,
            temperature: Some(0.2),
            num_ctx: settings.num_ctx,
            tools: None,
            json_schema: None,
        })
        .await
        .map_err(|e| AppError::General(format!("Syn could not look back: {e}")))?;
    Ok(reflect::check_pattern(&tag, &reply.content, sources))
}

// ─── Transcripts, captions and moments (Nhát G) ──────────────────

static MEDIA_RUNNING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

struct MediaRunning;

impl Drop for MediaRunning {
    fn drop(&mut self) {
        MEDIA_RUNNING.store(false, std::sync::atomic::Ordering::SeqCst);
    }
}

/// Where a file's bytes are on this device, and how many there are.
fn locate(db: &crate::db::DbBridge) -> impl Fn(&str) -> Option<(String, u64)> + '_ {
    move |id| {
        db.file_locations_for_node(id).ok()?.into_iter().find_map(|location| {
            let meta = std::fs::metadata(&location.abs_path).ok()?;
            meta.is_file().then(|| (location.abs_path.clone(), meta.len()))
        })
    }
}

#[derive(Debug, serde::Serialize)]
pub struct MediaStatus {
    pub config: media::Config,
    pub desktop: bool,
    pub provider: String,
    /// Captions need Ollama on this machine.
    pub local_provider: bool,
    pub pending_transcripts: usize,
    pub pending_captions: usize,
    pub done: usize,
    pub too_large: usize,
    pub running: bool,
}

#[tauri::command(async)]
pub fn timeline_media_status(
    state: tauri::State<'_, DbState>,
    timeline: tauri::State<'_, TimelineState>,
    vault_path: String,
) -> AppResult<MediaStatus> {
    let config = media::read_config(&vault_path);
    let settings = crate::commands::syn::settings_for(&vault_path);
    let timeline = timeline.lock().unwrap_or_else(|e| e.into_inner());
    extract::load(timeline.conn(), &vault_path)?;
    let db = state.lock().unwrap_or_else(|e| e.into_inner());
    media::mirror_into_search(timeline.conn(), &db)?;
    let (inputs, too_large) = media::inputs(&db, &config, locate(&db))?;
    let plan = media::plan(timeline.conn(), inputs)?;
    Ok(MediaStatus {
        desktop: cfg!(desktop),
        provider: settings.provider.key_slot().to_string(),
        local_provider: media::runs_here(&settings),
        pending_transcripts: plan.pending.iter().filter(|i| i.kind == "transcript").count(),
        pending_captions: plan.pending.iter().filter(|i| i.kind == "caption").count(),
        done: plan.done,
        too_large,
        running: MEDIA_RUNNING.load(std::sync::atomic::Ordering::SeqCst),
        config,
    })
}

#[derive(Debug, serde::Deserialize)]
pub struct MediaSettings {
    pub transcripts: bool,
    #[serde(default)]
    pub transcribe_url: String,
    #[serde(default)]
    pub transcribe_model: String,
    pub captions: bool,
    #[serde(default)]
    pub caption_model: String,
}

#[tauri::command]
pub fn timeline_media_configure(vault_path: String, settings: MediaSettings) -> AppResult<media::Config> {
    let mut config = media::read_config(&vault_path);
    config.transcripts = settings.transcripts;
    config.transcribe_url = settings.transcribe_url.trim().to_string();
    config.transcribe_model = settings.transcribe_model.trim().to_string();
    config.captions = settings.captions;
    config.caption_model = settings.caption_model.trim().to_string();
    if config.transcripts && config.transcribe_url.is_empty() {
        return Err(AppError::General("Transcripts need the address of a transcription server on this machine".into()));
    }
    media::write_config(&vault_path, &mut config, chrono::Utc::now())?;
    Ok(config)
}

/// Make transcripts and captions for files no device has read.
///
/// Only on a computer; transcripts only through a server on this machine,
/// captions only through Ollama. An automatic run reads a few files and says
/// nothing when it cannot run; asked for by hand it says why.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn timeline_media_run(
    app_handle: tauri::AppHandle,
    state: tauri::State<'_, DbState>,
    timeline: tauri::State<'_, TimelineState>,
    vault_path: String,
    auto: bool,
    limit: Option<usize>,
    locale: Option<String>,
) -> AppResult<media::MediaRun> {
    let skip = |why: &str| Ok(media::MediaRun { skipped: Some(why.to_string()), ..media::MediaRun::default() });
    if let Err(refused) = media::refuse_on_phone(cfg!(mobile)) {
        return if auto { skip("phone") } else { Err(refused) };
    }
    let config = media::read_config(&vault_path);
    if !config.transcripts && !config.captions {
        return if auto { skip("off") } else { Err(AppError::General("Transcripts and captions are both switched off".into())) };
    }
    let settings = crate::commands::syn::settings_for(&vault_path);
    let captions_ready = config.captions
        && settings.enabled
        && media::runs_here(&settings)
        && !config.caption_model.trim().is_empty();
    let transcripts_ready = config.transcripts && media::is_loopback(&config.transcribe_url);
    if !captions_ready && !transcripts_ready {
        return if auto {
            skip("not_ready")
        } else {
            Err(AppError::General(
                "Nothing can run: captions need Ollama on this machine and a model that reads images; transcripts need a transcription server on this machine".into(),
            ))
        };
    }
    if MEDIA_RUNNING.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return skip("running");
    }
    let _running = MediaRunning;
    let device = crate::commands::sync::ensure_device_id(&app_handle).map_err(AppError::General)?;

    let (work, remaining) = {
        let timeline = timeline.lock().unwrap_or_else(|e| e.into_inner());
        extract::load(timeline.conn(), &vault_path)?;
        let db = state.lock().unwrap_or_else(|e| e.into_inner());
        media::mirror_into_search(timeline.conn(), &db)?;
        let (inputs, _) = media::inputs(&db, &config, locate(&db))?;
        let mut work: Vec<media::MediaInput> = media::plan(timeline.conn(), inputs)?
            .pending
            .into_iter()
            .filter(|input| if input.kind == "caption" { captions_ready } else { transcripts_ready })
            .collect();
        if auto {
            work.retain(|input| !extract::resting(&media::failure_key(input)));
        } else {
            work.sort_by_key(|input| extract::resting(&media::failure_key(input)));
        }
        let limit = limit.unwrap_or(if auto { media::AUTO_LIMIT } else { usize::MAX });
        let remaining = work.len().saturating_sub(limit);
        work.truncate(limit);
        (work, remaining)
    };

    let mut report = media::MediaRun { remaining, ..media::MediaRun::default() };
    if work.is_empty() {
        return Ok(report);
    }
    let provider = if captions_ready {
        Some(crate::commands::syn::provider_for(&app_handle, &settings).await)
    } else {
        None
    };
    let language = if locale.as_deref().unwrap_or("en").starts_with("vi") { "Vietnamese" } else { "English" };

    for input in &work {
        let made = match (input.kind, provider.as_deref()) {
            ("transcript", _) => media::transcribe(input, &config.transcribe_url, &config.transcribe_model, chrono::Utc::now()).await,
            (_, Some(provider)) => {
                media::caption(provider, &config.caption_model, settings.num_ctx, input, language, chrono::Utc::now()).await
            }
            _ => continue,
        };
        match made.and_then(|surrogate| extract::record_surrogate(&vault_path, &device, surrogate, chrono::Utc::now())) {
            Ok(()) => {
                extract::clear_failure(&media::failure_key(input));
                report.read += 1
            }
            Err(e) => {
                extract::note_failure(&media::failure_key(input));
                report.failed.push(format!("{}: {e}", input.node_id))
            }
        }
    }
    log::info!("timeline media: made {}, {} failed, {} remaining", report.read, report.failed.len(), report.remaining);

    let timeline = timeline.lock().unwrap_or_else(|e| e.into_inner());
    extract::load(timeline.conn(), &vault_path)?;
    let db = state.lock().unwrap_or_else(|e| e.into_inner());
    media::mirror_into_search(timeline.conn(), &db)?;
    Ok(report)
}

/// Pictures and recordings during `when`, grouped into moments, with what
/// stands in for each and whether the file itself is on this device.
#[tauri::command(async)]
pub fn timeline_media_clusters(
    state: tauri::State<'_, DbState>,
    timeline: tauri::State<'_, TimelineState>,
    vault_path: String,
    when: String,
) -> AppResult<Vec<media::MediaCluster>> {
    let span = when::parse(&when)
        .ok_or_else(|| AppError::General(format!("'{when}' is not a time the timeline can read")))?;
    let mut timeline = timeline.lock().unwrap_or_else(|e| e.into_inner());
    store::catch_up_in(state.inner(), &mut timeline, Some(&vault_path))?;
    extract::load(timeline.conn(), &vault_path)?;
    let items = timeline.including_folded(span, chrono::Local::now().date_naive())?;
    let db = state.lock().unwrap_or_else(|e| e.into_inner());
    media::clusters(timeline.conn(), &db, &items)
}

#[cfg(test)]
mod review_fixes {
    use super::*;

    fn item(id: &str, kind: &str, node_id: &str, related: Option<&str>, from: &str) -> Event {
        Event {
            id: id.into(),
            kind: kind.into(),
            node_id: node_id.into(),
            node_type: "person".into(),
            title: format!("label of {id}"),
            node_title: node_id.into(),
            // Người kia của một quan hệ nằm ở link, không còn ở một cột (§4.9).
            links: related
                .map(|node| {
                    vec![crate::timeline::store::EventLink {
                        node_id: node.into(),
                        role: "with".into(),
                        label: None,
                    }]
                })
                .unwrap_or_default(),
            magnitude: 0.0,
            container_node: None,
            props: serde_json::Value::Null,
            happened_from: from.into(),
            happened_to: "9999-12-31".into(),
            precision: "month".into(),
            time_source: "frontmatter".into(),
            source: "derived".into(),
            shape: crate::timeline::derive::Shape::Occasion,
        }
    }

    #[test]
    fn a_relationship_on_both_sides_is_one_and_two_people_are_two() {
        let path_of: std::collections::HashMap<String, String> = [
            ("uuid-me", "People/me.md"), ("People/me.md", "People/me.md"),
            ("uuid-a", "People/a.md"), ("People/a.md", "People/a.md"),
            ("uuid-b", "People/b.md"), ("People/b.md", "People/b.md"),
        ]
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
        let items = vec![
            item("theirs", "connection", "People/a.md", Some("uuid-me"), "2014-01-01"),
            item("mine-a", "connection", "People/me.md", Some("uuid-a"), "2014-01-01"),
            item("mine-b", "connection", "People/me.md", Some("uuid-b"), "2014-01-01"),
        ];
        let kept: Vec<String> = one_side_of_each_relationship(items, &["People/me.md", "uuid-me"], &path_of)
            .into_iter()
            .map(|i| i.id)
            .collect();
        assert_eq!(kept, vec!["mine-a".to_string(), "mine-b".to_string()], "own side kept, the other person still there");
    }

}

/// The gate for Bước 4: what is written comes back the way it went in, and a
/// key somebody added by hand is still there afterwards.
#[cfg(test)]
mod writing_an_event {
    use std::collections::HashMap;
    use std::sync::Mutex;

    use serde_json::json;


    use tauri::Manager;

    use crate::db::DbBridge;
    use crate::timeline::derive::{self, Link, NodeView};
    use crate::timeline::when;

    /// The mock app, already holding the cache. `write_node_inner` reaches for
    /// managed state further down rather than only using what it is handed, so
    /// a handle that manages nothing panics before anything is written.
    fn app() -> tauri::App<tauri::test::MockRuntime> {
        let cache: crate::db::DbState = Mutex::new(DbBridge::new_in_memory_full().unwrap());
        tauri::test::mock_builder()
            .manage(cache)
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .expect("a mock app")
    }

    fn vault() -> (tempfile::TempDir, String) {
        let dir = tempfile::tempdir().unwrap();
        let path = std::fs::canonicalize(dir.path()).unwrap().to_string_lossy().to_string();
        (dir, path)
    }

    /// The day's note is the person's. Writing a moment used to add a list
    /// to it, and make the note if the day had none.
    #[test]
    fn a_moment_written_leaves_the_day_s_note_alone() {
        let app = app();
        let (_dir, vault_path) = vault();
        let managed = app.state::<crate::db::DbState>();
        let state = managed.inner();

        crate::commands::nodes::write_node_inner(
            app.handle(), state, vault_path.clone(), "Notes/2016-05-14.md".into(),
            "2016-05-14".to_string(), "note".to_string(),
            json!({ "date": "2016-05-14", "tags": ["daily"] }), Some("Hôm nay.".into()),
        )
        .unwrap();
        let before = std::fs::read_to_string(std::path::Path::new(&vault_path).join("Notes/2016-05-14.md")).unwrap();

        let written = super::write_event_inner(
            app.handle(), state, &vault_path, "Đám cưới", "2016-05-14", "2016-05-14", "day", &[], None, &[],
        )
        .expect("the moment");
        assert!(written.starts_with("Moments/") && written.ends_with(".md"), "{written}");

        let after = std::fs::read_to_string(std::path::Path::new(&vault_path).join("Notes/2016-05-14.md")).unwrap();
        assert_eq!(before, after, "the day's note was written into");
        let notes = std::fs::read_dir(std::path::Path::new(&vault_path).join("Notes")).unwrap().count();
        assert_eq!(notes, 1, "no note was made for the day");
    }

    #[test]
    fn a_moment_written_is_a_moment_derived_and_nothing_else_is_lost() {
        let app = app();
        let (_dir, vault_path) = vault();
        let managed = app.state::<crate::db::DbState>();
        let state = managed.inner();

        let write = |title: &str, people: Vec<String>, place: Option<&str>| {
            super::write_event_inner(
                app.handle(), state, &vault_path, title, "2016-05-14", "2016-05-14", "day", &people, place, &[],
            )
            .expect("the moment was written")
        };

        let wedding = write("Đám cưới Tuấn và Thuỳ", vec!["Tuấn".into()], Some("Hà Nội"));
        let abs = crate::path_utils::resolve_safe_path(&vault_path, &wedding).unwrap();
        assert!(abs.exists());

        // A key this app has no meaning for, added the way a person would.
        crate::commands::nodes::write_node_inner(
            app.handle(), state, vault_path.clone(), wedding.clone(),
            "Đám cưới Tuấn và Thuỳ".to_string(), "moment".to_string(), json!({ "mood": "tốt" }), None,
        )
        .expect("the hand-added key");

        let dinner = write("Ăn tối", Vec::new(), None);
        assert_ne!(dinner, wedding, "one moment, one file");

        let on_disk = crate::commands::nodes::existing_properties(&abs, "md");
        assert_eq!(on_disk["type"], json!("moment"));
        assert_eq!(on_disk["mood"], json!("tốt"), "a key nobody here understands is still a key somebody wrote");
        assert_eq!(on_disk["title"], json!("Đám cưới Tuấn và Thuỳ"));
        assert_eq!(on_disk["happened"], json!("2016-05-14"));
        assert_eq!(on_disk["people"], json!(["Tuấn"]), "no such person yet, so the name stands");
        assert_eq!(on_disk["where"], json!("Hà Nội"));
        assert_eq!(on_disk["origin"], json!("manual"));
        assert!(!on_disk.contains_key("source"), "written by hand, read from nothing");
        let dinner_on_disk = crate::commands::nodes::existing_properties(
            &crate::path_utils::resolve_safe_path(&vault_path, &dinner).unwrap(), "md",
        );
        assert!(dinner_on_disk.get("people").is_none(), "nobody was named: {dinner_on_disk:?}");

        // And the timeline reads back what was written.
        let properties = serde_json::Value::Object(on_disk.clone());
        let derived = derive::derive(
            &NodeView { id: &wedding, node_type: "moment", title: "Đám cưới Tuấn và Thuỳ", properties: &properties },
            &HashMap::new(),
        );
        assert_eq!(derived.len(), 1, "{derived:?}");
        assert_eq!(derived[0].kind, "moment");
        assert_eq!(derived[0].title.as_deref(), Some("Đám cưới Tuấn và Thuỳ"));
        assert_eq!(derived[0].span, when::parse("2016-05-14").unwrap());
        assert_eq!(derived[0].links, vec![Link::with("Tuấn"), Link::at("Hà Nội")]);
    }

    /// A moment already kept is the person's to change and theirs to let go.
    /// What they change becomes theirs — no later reading proposes over it.
    #[test]
    fn a_kept_moment_can_be_put_right_and_let_go() {
        let app = app();
        let (_dir, vault_path) = vault();
        let managed = app.state::<crate::db::DbState>();
        let state = managed.inner();
        let path = "Moments/6f3c9a2e-0000-4000-8000-000000000001.md";

        crate::commands::nodes::write_node_inner(
            app.handle(), state, vault_path.clone(), path.into(),
            "Ăn trưa".to_string(), "moment".to_string(),
            json!({
                "type": "moment", "title": "Ăn trưa", "happened": "2026-07-21",
                "people": ["People/nga.md", "Đức"], "category": "meal", "origin": "extract",
                "source": { "node": "Notes/2026-07-21.md", "quote": "Trưa ăn bún chả", "block": "b1" }
            }),
            Some(String::new()),
        )
        .unwrap();

        let moment = crate::timeline::moments::one(&state.lock().unwrap(), path).unwrap().expect("kept");
        crate::timeline::moments::edit(
            app.handle(),
            state,
            &vault_path,
            &moment,
            serde_json::from_value(json!({
                "title": "Ăn trưa bún chả với Nga",
                "where": "Hàng Mành",
                "category": "meal",
                "people": ["People/nga.md"]
            }))
            .unwrap(),
        )
        .unwrap();

        let abs = crate::path_utils::resolve_safe_path(&vault_path, path).unwrap();
        let on_disk = crate::commands::nodes::existing_properties(&abs, "md");
        assert_eq!(on_disk["title"], json!("Ăn trưa bún chả với Nga"));
        assert_eq!(on_disk["where"], json!("Hàng Mành"));
        assert_eq!(on_disk["people"], json!(["People/nga.md"]));
        // Changed is theirs; left alone is not.
        assert_eq!(on_disk["hand"], json!(["people", "title", "where"]), "{on_disk:?}");
        assert_eq!(
            on_disk["source"],
            json!({ "node": "Notes/2026-07-21.md", "quote": "Trưa ăn bún chả", "block": "b1" }),
            "what it was read from is not theirs to rewrite"
        );

        // And letting it go moves the file to the trash rather than destroying it.
        let db = state.lock().unwrap();
        let moved = crate::commands::trash::apply_trash(&db, &vault_path, path).unwrap();
        assert!(!abs.exists(), "gone from Moments/");
        assert!(crate::path_utils::resolve_safe_path(&vault_path, &moved).unwrap().exists(), "{moved}");
    }
}

