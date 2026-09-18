//! Asking the timeline what happened. See `crate::timeline`.
//!
//! Every answer here passes through the seals (`timeline::seal`): what the
//! person sealed is not brought back by the timeline either.

use std::sync::Arc;

use crate::db::DbState;
use crate::error::{AppError, AppResult};
use crate::timeline::frame::{self, TimeFrame};
use crate::timeline::seal::{self, SealedPeriod, Seals};
use crate::timeline::store::{self, CatchUp, Snapshot, Event};
use crate::timeline::quiet::{self, Hush, Quiet, Subject};
use crate::timeline::{extract, media, presence, reflect, when, TimelineState};

fn seals_of(state: &DbState, vault_path: &str) -> AppResult<Arc<Seals>> {
    let db = state.lock().unwrap_or_else(|e| e.into_inner());
    seal::current(&db, vault_path)
}

fn quiet_of(state: &DbState, vault_path: &str) -> AppResult<Arc<Quiet>> {
    let db = state.lock().unwrap_or_else(|e| e.into_inner());
    quiet::current(&db, vault_path)
}

/// Everything the vault says happened during `when`, less what is sealed.
///
/// `when` is any time the timeline reads: `2016-05-14`, `2016-05`, `2016`,
/// `2016-05-01/2016-06-30` or `~2012`. The timeline catches up with the vault
/// first, and only reads the vault again if something in it changed.
#[tauri::command]
pub fn timeline_query(
    state: tauri::State<'_, DbState>,
    timeline: tauri::State<'_, TimelineState>,
    vault_path: String,
    when: String,
) -> AppResult<Vec<Event>> {
    let span = when::parse(&when).ok_or_else(|| {
        AppError::General(format!(
            "'{when}' is not a time the timeline can read. Use 2016-05-14, 2016-05, 2016, \
             2016-05-01/2016-06-30 or ~2012."
        ))
    })?;

    let mut timeline = timeline.lock().unwrap_or_else(|e| e.into_inner());
    store::catch_up_in(state.inner(), &mut timeline, Some(&vault_path))?;
    let mut items = timeline.query(span, chrono::Local::now().date_naive())?;
    let seals = seals_of(state.inner(), &vault_path)?;
    items.retain(|item| !seals.hides_item(item));
    Ok(items)
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
    let seals = seals_of(state.inner(), &vault_path)?;
    Ok(sealed_frame(items, &nodes, today, &seals, reveal.unwrap_or(false)))
}

/// A frame with what is sealed taken out of it, apart from the command so it
/// can be tested without a runtime.
pub(crate) fn sealed_frame(
    items: Vec<Event>,
    nodes: &[frame::FrameNode],
    today: chrono::NaiveDate,
    seals: &Seals,
    reveal: bool,
) -> TimeFrame {
    let shown: Vec<Event> = if reveal {
        items
    } else {
        // Nothing dated inside a sealed period is drawn: not as density, not
        // as the day somebody arrived, not as a relationship beginning.
        items
            .into_iter()
            .filter(|item| !seals.hides_item(item) && !seals.covers(&item.happened_from))
            .collect()
    };
    let mut built = frame::build(&shown, nodes, today);
    if !reveal {
        // A withheld node keeps its place in the present graph, which is the
        // app's own view of it. What it loses is a place in time: placing it
        // would draw the sealed period back in, one node at a time.
        built.first_seen.retain(|id, day| !seals.hides(id) && !seals.covers(day));
        built.density.retain(|month| !seals.covers_month(&month.month));
        built.died_on.retain(|id, _| !seals.hides(id));
        built
            .links
            .retain(|link| !seals.hides(&link.source) && !seals.hides(&link.target));
    }
    built.sealed = seals.periods().to_vec();
    built
}

/// What the timeline holds about one node, under its path and its identity,
/// less what is sealed.
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
    let seals = seal::current(&db, &vault_path)?;
    let path_of = person_paths(&db)?;
    drop(db);
    let items = items.into_iter().filter(|item| !seals.hides_item(item)).collect();
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

/// Seal a period: `from` and `to` as 2019, 2019-02 or 2019-02-14.
#[tauri::command]
pub fn seal_period(vault_path: String, from: String, to: String) -> AppResult<SealedPeriod> {
    seal::write_period(&vault_path, &from, &to)
}

/// Lift the seal on a period.
#[tauri::command]
pub fn remove_seal(vault_path: String, id: String) -> AppResult<()> {
    seal::remove_period(&vault_path, &id)
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
    use crate::timeline::frame::FrameNode;

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
        }
    }

    /// The strip shows a sealed period as sealed: no density in it, no node
    /// placed inside it, and the period itself to draw.
    #[test]
    fn a_frame_does_not_draw_what_happened_in_a_sealed_period_unless_asked() {
        let dir = tempfile::tempdir().unwrap();
        let vault = dir.path().to_string_lossy().to_string();
        seal::write_period(&vault, "2019-02", "2019-09").unwrap();
        let db = crate::db::DbBridge::new_in_memory_full().unwrap();
        let seals = seal::Seals::read(&db, &vault).unwrap();

        let nodes = [FrameNode {
            id: "Notes/then.md".into(),
            stable_id: "Notes/then.md".into(),
            created_at: "2026-01-01T12:00:00.000Z".into(),
        }];
        let items = vec![item("note", "Notes/then.md", "2019-05-11"), item("note", "Notes/after.md", "2020-01-01")];
        let today = chrono::NaiveDate::from_ymd_opt(2026, 9, 15).unwrap();

        let sealed = sealed_frame(items.clone(), &nodes, today, &seals, false);
        assert_eq!(sealed.sealed.len(), 1);
        assert!(sealed.density.iter().all(|m| m.month != "2019-05"), "{:?}", sealed.density);
        assert_ne!(sealed.first_seen.get("Notes/then.md").map(String::as_str), Some("2019-05-11"));

        let revealed = sealed_frame(items, &nodes, today, &seals, true);
        assert!(revealed.density.iter().any(|m| m.month == "2019-05"));
        assert_eq!(revealed.sealed.len(), 1, "revealed for one look, still sealed");
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

/// Everything the tray shows: whether reading is on, what it would send where,
/// how much is left and how long that would take, and what waits for a decision.
#[derive(Debug, serde::Serialize)]
pub struct ExtractStatus {
    pub config: extract::Config,
    pub syn_enabled: bool,
    pub provider: String,
    pub local: bool,
    pub model: Option<String>,
    pub desktop: bool,
    pub running: bool,
    pub pending: usize,
    pub stale: usize,
    pub old_version: usize,
    pub done: usize,
    /// For the notes never read.
    pub estimate_ms: u64,
    /// For everything a full re-read would cover: never read, edited, and read
    /// by an older extractor.
    pub estimate_all_ms: u64,
    /// Measured on this device, rather than a starting guess.
    pub estimate_measured: bool,
    pub unreadable: Vec<String>,
    pub proposals: Vec<extract::Proposal>,
}

#[tauri::command(async)]
pub fn timeline_extract_status(
    app_handle: tauri::AppHandle,
    state: tauri::State<'_, DbState>,
    timeline: tauri::State<'_, TimelineState>,
    vault_path: String,
) -> AppResult<ExtractStatus> {
    let settings = crate::commands::syn::settings_for(&vault_path);
    let config = extract::read_config(&vault_path);
    let device = crate::commands::sync::ensure_device_id(&app_handle).map_err(AppError::General)?;
    let today = chrono::Local::now().date_naive();

    let timeline = timeline.lock().unwrap_or_else(|e| e.into_inner());
    let loaded = extract::load(timeline.conn(), &vault_path)?;
    let (inputs, seals, people) = {
        let db = state.lock().unwrap_or_else(|e| e.into_inner());
        let seals = seal::current(&db, &vault_path)?;
        let inputs = extract::inputs(&db, &vault_path, &config, &seals, today, None)?;
        (inputs, seals, extract::People::read(&db)?)
    };
    let proposals = extract::proposals(
        timeline.conn(),
        &inputs,
        &extract::reviewed(&vault_path),
        &seals,
        &people,
    )?;
    let plan = extract::plan(timeline.conn(), inputs)?;
    let chars = |inputs: &[extract::Input]| inputs.iter().map(|i| i.text.chars().count()).sum::<usize>();
    let local = media::runs_here(&settings);
    let (estimate_ms, measured) = extract::estimate_ms(timeline.conn(), &device, chars(&plan.pending), local)?;
    let everything = chars(&plan.pending) + chars(&plan.stale) + chars(&plan.old_version);
    let (estimate_all_ms, _) = extract::estimate_ms(timeline.conn(), &device, everything, local)?;

    Ok(ExtractStatus {
        config,
        syn_enabled: settings.enabled,
        provider: settings.provider.key_slot().to_string(),
        local,
        model: settings.default_model.clone(),
        desktop: cfg!(desktop),
        running: EXTRACTING.load(std::sync::atomic::Ordering::SeqCst),
        pending: plan.pending.len(),
        stale: plan.stale.len(),
        old_version: plan.old_version.len(),
        done: plan.done,
        estimate_ms,
        estimate_all_ms,
        estimate_measured: measured,
        unreadable: loaded.unreadable,
        proposals,
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

    let (work, people, remaining) = {
        let timeline = timeline.lock().unwrap_or_else(|e| e.into_inner());
        extract::load(timeline.conn(), &vault_path)?;
        let (inputs, people) = {
            let db = state.lock().unwrap_or_else(|e| e.into_inner());
            let seals = seal::current(&db, &vault_path)?;
            let today = chrono::Local::now().date_naive();
            let inputs = extract::inputs(&db, &vault_path, &config, &seals, today, settled_before)?;
            (inputs, extract::People::read(&db)?)
        };
        let plan = extract::plan(timeline.conn(), inputs)?;
        let mut work = match (scope.as_str(), auto) {
            ("new", _) => plan.pending,
            (_, true) => return skip("scope"),
            ("stale", false) => plan.stale,
            ("old", false) => plan.old_version,
            ("all", false) => [plan.pending, plan.stale, plan.old_version].concat(),
            (other, false) => return refuse(&format!("'{other}' is not a scope: use new, stale, old or all")),
        };
        // A note that keeps failing rests before an automatic run tries it
        // again, and waits behind the rest when asked for by hand.
        if auto {
            work.retain(|input| !extract::resting(&extract::failure_key(input)));
        } else {
            work.sort_by_key(|input| extract::resting(&extract::failure_key(input)));
        }
        let limit = limit.unwrap_or(if auto { extract::AUTO_LIMIT } else { usize::MAX });
        let remaining = work.len().saturating_sub(limit);
        work.truncate(limit);
        (work, people, remaining)
    };

    if work.is_empty() {
        return Ok(extract::ExtractRun::default());
    }

    let provider = crate::commands::syn::provider_for(&app_handle, &settings).await;
    let mut report =
        extract::extract_all(provider.as_ref(), &model, settings.num_ctx, &work, &people, &vault_path, &device).await;
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
    extract::load(timeline.conn(), &vault_path)?;
    Ok(report)
}

/// Accept a proposal into the note it came from, or decline it.
///
/// Accepting writes a `moments` entry into that note's frontmatter, which is
/// the first moment anything reaches the vault's own notes (§4.7). Declining
/// writes only the decision, so no device offers it again.
#[tauri::command(async)]
pub fn timeline_extract_review(
    app_handle: tauri::AppHandle,
    state: tauri::State<'_, DbState>,
    timeline: tauri::State<'_, TimelineState>,
    vault_path: String,
    item_id: String,
    accept: bool,
    node_id: Option<String>,
) -> AppResult<()> {
    let device = crate::commands::sync::ensure_device_id(&app_handle).map_err(AppError::General)?;
    let item = {
        let timeline = timeline.lock().unwrap_or_else(|e| e.into_inner());
        extract::load(timeline.conn(), &vault_path)?;
        extract::item(timeline.conn(), &item_id)?
    }
    .ok_or_else(|| AppError::General(format!("No proposal {item_id}")))?;
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
    } else {
        let seals = seals_of(state.inner(), &vault_path)?;
        let base = node.split('#').next().unwrap_or_default();
        if seals.hides(base) || item.payload.people.iter().any(|person| seals.hides(person)) {
            return Err(AppError::General("That proposal is about something sealed".into()));
        }
        if base.starts_with("Syn/") {
            // A conversation has no frontmatter; the decision keeps the moment.
            extract::decide(&vault_path, &device, decision("accepted", Some(item.clone())), now)?;
        } else {
            let existing = {
                let db = state.lock().unwrap_or_else(|e| e.into_inner());
                db.get_node(base)?
            }
            .ok_or_else(|| AppError::General(format!("{base} is no longer in the vault")))?;
            if !extract::still_reads_as_read(&existing.content, &item) {
                return Err(AppError::General(
                    "The note has changed since this was read from it. Read it again before keeping this.".into(),
                ));
            }
            // From the file, not the cache: a moment kept on another device may
            // have arrived on disk and not been read in yet.
            let abs = crate::path_utils::resolve_safe_path(&vault_path, base).map_err(|e| AppError::General(e.to_string()))?;
            let on_disk = crate::commands::nodes::existing_properties(&abs, "md");
            let moments = extract::moments_after_keeping(&on_disk, &item);
            let title = on_disk.get("title").and_then(serde_json::Value::as_str).map(String::from).unwrap_or_else(|| existing.title.clone());
            let node_type = on_disk.get("type").and_then(serde_json::Value::as_str).map(String::from).unwrap_or_else(|| existing.node_type.clone());
            crate::commands::nodes::write_node_inner(
                &app_handle,
                state.inner(),
                vault_path.clone(),
                base.to_string(),
                title,
                node_type,
                serde_json::json!({ "moments": moments }),
                None,
            )?;
            extract::decide(&vault_path, &device, decision("accepted", None), now)?;
        }
    }

    let timeline = timeline.lock().unwrap_or_else(|e| e.into_inner());
    extract::load(timeline.conn(), &vault_path)?;
    Ok(())
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

    let seals = seals_of(state.inner(), &vault_path)?;
    if names.iter().any(|name| seals.hides(name)) {
        return Ok(Vec::new());
    }

    // From the events themselves, through the same seal filter every other
    // read goes through: what is sealed never reaches the stretches, rather
    // than being subtracted from them afterwards.
    let today = chrono::Local::now().date_naive();
    let events: Vec<_> = timeline
        .about(&names, today)?
        .into_iter()
        .filter(|event| !seals.hides_item(event) && !seals.covers(&event.happened_from))
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

    let today = chrono::Local::now().date_naive();
    let input = extract::Input {
        node_id: String::new(),
        node_type: "note".into(),
        title: when::iso(today),
        recorded: today,
        // A line with no time of its own happened today, which is what the
        // person sees and can correct.
        dated_by_day: true,
        hash: blake3::hash(line.as_bytes()).to_hex().to_string(),
        text: line,
        person_id: None,
    };
    let people = {
        let db = state.lock().unwrap_or_else(|e| e.into_inner());
        extract::People::read(&db)?
    };
    let provider = crate::commands::syn::provider_for(&app_handle, &settings).await;

    let (items, _) = extract::extract_one(
        provider.as_ref(),
        &model,
        settings.num_ctx,
        &input,
        &people,
        chrono::Utc::now(),
    )
    .await?;
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
        .map(|id| ComposedPerson { name: people.title(id).unwrap_or(id).to_string(), node_id: Some(id.clone()) })
        .chain(item.payload.names.iter().map(|name| ComposedPerson { name: name.clone(), node_id: None }))
        .collect();

    let today_iso = when::iso(today);
    Ok(ComposedReply {
        read: Some(ComposedView {
            title: item.payload.title,
            dated: !(item.happened_from == today_iso && item.happened_to == today_iso),
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

/// Write an event into the note for the day it happened on.
///
/// Into `moments[]` in that note's frontmatter, which is where a person's own
/// moments already live (Nhát E) and what `derive::moments` reads. The note is
/// made if the day has none. Returns the note it was written into.
///
/// Everything goes through `write_node_inner`, the one way a node reaches
/// disk, which merges with what is on the file rather than with the cache —
/// so a key somebody added by hand survives this.
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
    format_str: String,
    tag: String,
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
        &format_str,
        &tag,
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
    format_str: &str,
    tag: &str,
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
    let seals = seals_of(state, vault_path)?;
    if let Some(hidden) = people.iter().find(|person| seals.hides(person)) {
        return Err(AppError::General(format!("{hidden} is sealed")));
    }

    let mut moment = serde_json::Map::new();
    moment.insert("title".into(), serde_json::Value::String(title.clone()));
    moment.insert("happened".into(), serde_json::Value::String(happened(precision, happened_from, happened_to)));
    if !people.is_empty() {
        moment.insert("people".into(), serde_json::json!(people));
    }
    if let Some(place) = place.map(str::trim).filter(|p| !p.is_empty()) {
        moment.insert("where".into(), serde_json::Value::String(place.to_string()));
    }

    let (id, note_title) = daily_note(state, vault_path, from, format_str)?;
    let making_the_note = id.is_none();
    let (moments, on_disk_title) = match &id {
        Some(id) => {
            let abs = crate::path_utils::resolve_safe_path(vault_path, id)
                .map_err(|e| AppError::General(e.to_string()))?;
            let on_disk = crate::commands::nodes::existing_properties(&abs, "md");
            let mut moments = on_disk
                .get("moments")
                .and_then(serde_json::Value::as_array)
                .cloned()
                .unwrap_or_default();
            moments.push(serde_json::Value::Object(moment));
            let title = on_disk
                .get("title")
                .and_then(serde_json::Value::as_str)
                .map(String::from)
                .unwrap_or_else(|| note_title.clone());
            (moments, title)
        }
        None => (vec![serde_json::Value::Object(moment)], note_title.clone()),
    };

    let rel_path = id.unwrap_or_else(|| format!("Notes/{}.md", uuid::Uuid::new_v4()));
    let mut properties = serde_json::Map::new();
    properties.insert("moments".into(), serde_json::Value::Array(moments));
    // `date` and `tags` are the note's own, and `resolve_properties` replaces a
    // key rather than merging inside it — so setting them on a note that
    // already exists would take whatever the person had put there. They are
    // only ours to write on a note this call is making.
    if making_the_note {
        properties.insert("date".into(), serde_json::Value::String(when::iso(from)));
        if let Some(tag) = Some(tag.trim()).filter(|t| !t.is_empty()) {
            properties.insert("tags".into(), serde_json::json!([tag]));
        }
    }
    crate::commands::nodes::write_node_inner(
        app_handle,
        state,
        vault_path.to_string(),
        rel_path.clone(),
        on_disk_title,
        "note".to_string(),
        serde_json::Value::Object(properties),
        None,
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

/// The note for a day, and what it would be called. `None` means no note has
/// that name yet, and the caller makes one.
fn daily_note(
    state: &DbState,
    vault_path: &str,
    day: chrono::NaiveDate,
    format_str: &str,
) -> AppResult<(Option<String>, String)> {
    let noon = day.and_hms_opt(12, 0, 0).unwrap_or_default();
    let local = chrono::TimeZone::from_local_datetime(&chrono::Local, &noon)
        .single()
        .unwrap_or_else(chrono::Local::now);
    // The person's own naming, the same one the Notes app opens a day with.
    let named = crate::commands::nodes::date_string_from_pattern(format_str, local)
        .unwrap_or_else(|| when::iso(day));
    let db = state.lock().unwrap_or_else(|e| e.into_inner());
    let found = db
        .get_nodes_by_type("note")
        .unwrap_or_default()
        .into_iter()
        .find(|note| note.title == named)
        // A row's id is the path inside the vault — but it is only ever as
        // right as whatever wrote it, and everything below treats it as
        // relative. One that is not is made so rather than believed.
        .map(|note| {
            let path = std::path::Path::new(&note.id);
            if path.is_absolute() {
                crate::path_utils::to_relative(path, vault_path)
            } else {
                note.id
            }
        });
    Ok((found, named))
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
    let seals = seal::current(&db, &vault_path)?;
    let decisions = reflect::decisions(&db, &seals, chrono::Local::now().date_naive())?;
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
        let seals = seal::current(&db, &vault_path)?;
        let decisions = reflect::decisions(&db, &seals, chrono::Local::now().date_naive())?;
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
    let seals = seal::current(&db, &vault_path)?;
    let (inputs, too_large) = media::inputs(&db, &config, &seals, locate(&db))?;
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
        let seals = seal::current(&db, &vault_path)?;
        let (inputs, _) = media::inputs(&db, &config, &seals, locate(&db))?;
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
pub fn timeline_media_moments(
    state: tauri::State<'_, DbState>,
    timeline: tauri::State<'_, TimelineState>,
    vault_path: String,
    when: String,
) -> AppResult<Vec<media::MediaMoment>> {
    let span = when::parse(&when)
        .ok_or_else(|| AppError::General(format!("'{when}' is not a time the timeline can read")))?;
    let mut timeline = timeline.lock().unwrap_or_else(|e| e.into_inner());
    store::catch_up_in(state.inner(), &mut timeline, Some(&vault_path))?;
    extract::load(timeline.conn(), &vault_path)?;
    let items = timeline.including_folded(span, chrono::Local::now().date_naive())?;
    let db = state.lock().unwrap_or_else(|e| e.into_inner());
    let seals = seal::current(&db, &vault_path)?;
    media::moments(timeline.conn(), &db, &seals, &items)
}

#[cfg(test)]
mod review_fixes {
    use super::*;
    use serde_json::json;

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

    #[test]
    fn nothing_dated_in_a_sealed_period_is_drawn() {
        let seals = seal::compute(
            vec![seal::SealedPeriod {
                id: "p".into(),
                from: "2019-02-01".into(),
                to: "2019-09-30".into(),
                from_text: "2019-02".into(),
                to_text: "2019-09".into(),
            }],
            &[],
            &[],
        );
        let items = vec![
            item("c", "connection", "People/me.md", Some("People/a.md"), "2019-05-01"),
            item("n", "note", "Notes/x.md", None, "2020-01-01"),
        ];
        let frame = sealed_frame(items, &[], chrono::NaiveDate::from_ymd_opt(2026, 9, 15).unwrap(), &seals, false);
        assert!(frame.links.is_empty());
        assert!(frame.density.iter().all(|m| !m.month.starts_with("2019")), "{:?}", frame.density);
        assert!(frame.first_seen.values().all(|day| !day.starts_with("2019")), "{:?}", frame.first_seen);
        let _ = json!({});
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

    /// A tag the person put on their day is not this feature's to remove.
    #[test]
    fn an_event_written_keeps_the_tags_the_note_already_had() {
        let app = app();
        let vault = tempfile::tempdir().unwrap();
        let vault_path = std::fs::canonicalize(vault.path()).unwrap().to_string_lossy().to_string();
        let managed = app.state::<crate::db::DbState>();
        let state = managed.inner();

        let note = super::write_event_inner(
            app.handle(), state, &vault_path, "Đám cưới", "2016-05-14", "2016-05-14",
            "day", &[], None, "YYYY-MM-DD", "daily",
        )
        .expect("the first event");

        // The person tags their own day, the way they would in the editor.
        crate::commands::nodes::write_node_inner(
            app.handle(), state, vault_path.clone(), note.clone(),
            "2016-05-14".to_string(), "note".to_string(),
            json!({ "tags": ["daily", "công-việc", "hà-nội"] }), None,
        )
        .expect("the person's tags");

        super::write_event_inner(
            app.handle(), state, &vault_path, "Ăn tối", "2016-05-14", "2016-05-14",
            "day", &[], None, "YYYY-MM-DD", "daily",
        )
        .expect("the second event");

        let abs = crate::path_utils::resolve_safe_path(&vault_path, &note).unwrap();
        let tags = crate::commands::nodes::existing_properties(&abs, "md");
        assert_eq!(
            tags.get("tags"),
            Some(&json!(["daily", "công-việc", "hà-nội"])),
            "writing an event took the day's other tags with it"
        );
    }

    #[test]
    fn an_event_written_is_an_event_derived_and_nothing_else_is_lost() {
        let app = app();
        let vault = tempfile::tempdir().unwrap();
        let vault_path = std::fs::canonicalize(vault.path())
            .unwrap()
            .to_string_lossy()
            .to_string();
        let managed = app.state::<crate::db::DbState>();
        let state = managed.inner();

        let write = |title: &str, people: Vec<String>, place: Option<&str>| {
            super::write_event_inner(
                app.handle(),
                state,
                &vault_path,
                title,
                "2016-05-14",
                "2016-05-14",
                "day",
                &people,
                place,
                "YYYY-MM-DD",
                "",
            )
            .expect("the event was written")
        };

        let note = write("Đám cưới Tuấn và Thuỳ", vec!["Tuấn".into()], Some("Hà Nội"));
        let abs = crate::path_utils::resolve_safe_path(&vault_path, &note).unwrap();
        assert!(abs.exists(), "the day had no note, so one was made");

        // A key this app has no meaning for, added the way a person would.
        crate::commands::nodes::write_node_inner(
            app.handle(),
            state,
            vault_path.clone(),
            note.clone(),
            "2016-05-14".to_string(),
            "note".to_string(),
            json!({ "mood": "tốt" }),
            None,
        )
        .expect("the hand-added key");

        let again = write("Ăn tối", Vec::new(), None);
        assert_eq!(again, note, "the same day is the same note");

        let on_disk = crate::commands::nodes::existing_properties(&abs, "md");
        assert_eq!(
            on_disk.get("mood").and_then(serde_json::Value::as_str),
            Some("tốt"),
            "a key nobody here understands is still a key somebody wrote"
        );
        let moments = on_disk.get("moments").and_then(serde_json::Value::as_array).expect("moments");
        assert_eq!(moments.len(), 2, "{moments:?}");
        assert_eq!(moments[0]["title"], json!("Đám cưới Tuấn và Thuỳ"));
        assert_eq!(moments[0]["happened"], json!("2016-05-14"));
        assert_eq!(moments[0]["people"], json!(["Tuấn"]), "no such person yet, so the name stands");
        assert_eq!(moments[0]["where"], json!("Hà Nội"));
        assert!(moments[1].get("people").is_none(), "nobody was named: {:?}", moments[1]);

        // And the timeline reads back what was written.
        let properties = serde_json::Value::Object(on_disk.clone());
        let derived = derive::derive(
            &NodeView { id: &note, node_type: "note", title: "2016-05-14", properties: &properties },
            &HashMap::new(),
        );
        let wedding = derived
            .iter()
            .find(|d| d.title.as_deref() == Some("Đám cưới Tuấn và Thuỳ"))
            .expect("the event that was just written");
        assert_eq!(wedding.kind, "moment");
        assert_eq!(wedding.span, when::parse("2016-05-14").unwrap());
        assert_eq!(wedding.links, vec![Link::with("Tuấn"), Link::at("Hà Nội")]);
    }
}
