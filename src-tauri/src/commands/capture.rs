//! The one way a thought gets into the vault from outside the app.
//!
//! QuickCap can only be reached today by opening the app, waiting for the
//! vault to load and pressing a tab. For a capture inbox that is the thing
//! that decides whether the product is used at all: the whole category is
//! won by whoever is fastest between having a thought and having it saved.
//!
//! Phase 2 adds a lot of ways in — an Android share sheet, a text-selection
//! action, a home widget, a notification you can type into, a desktop
//! hotkey, a browser clipper. None of them should know anything about
//! Markdown, frontmatter or vaults. They all do one thing: hand a string to
//! this module.
//!
//! # Why a queue rather than a write
//!
//! A capture arrives whenever the user has the thought, which is regularly
//! a moment when the app cannot write anything: the vault is locked behind
//! a PIN, or has not been chosen yet, or the process was started by an
//! intent and has no vault open at all. Refusing the capture then would
//! teach the user that the fast path is unreliable, and they would stop
//! using it.
//!
//! So intake is decoupled from storage. Queueing is a single row in
//! `kv_store` and cannot fail for any reason the user would recognise; the
//! cap itself is written later, when a vault is open.
//!
//! # Why the drain lives in the front end
//!
//! Turning text into a cap means deriving its tags and its title and
//! writing frontmatter — rules that already exist, in TypeScript, and are
//! applied to every cap typed into the app. Reimplementing them here would
//! make a captured cap subtly different from a typed one, which is the
//! class of bug `contracts/tag-grammar.json` exists to prevent.
//!
//! So this module owns durability and the front end owns the format: it
//! lists what is waiting, writes each one through the ordinary path, and
//! drops each entry only once its cap is on disk. An interruption costs a
//! duplicate at worst, never a lost thought.

use serde::{Deserialize, Serialize};

use crate::db::DbState;
use crate::error::AppResult;

/// Keys are namespaced so a pending capture cannot collide with the device
/// identity, sync bookkeeping or a migration flag.
const QUEUE_PREFIX: &str = "capture:pending:";

/// Where the next sequence number lives.
///
/// A timestamp is not enough on its own: a share sheet handing over several
/// items lands them inside the same millisecond, and a random tiebreaker
/// then sorts them arbitrarily — which is how "keeps arrival order" failed
/// the first time it was tested. A counter is the only thing that orders
/// two captures that arrived at the same instant.
const SEQUENCE_KEY: &str = "capture:next-seq";

/// The URL prefixes a capture may arrive under, canonical one first.
const SCHEMES: [&str; 2] = ["com.synabit.app://", "synabit://"];

/// A capture waiting for a vault to be open.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct QueuedCapture {
    /// The `kv_store` key, used to drop this entry once its cap is written.
    pub id: String,
    pub text: String,
    /// Where it came from — a share sheet, a widget, a browser. Recorded so
    /// the app can eventually say "3 caps from Chrome this week", and so a
    /// misbehaving surface can be identified rather than guessed at.
    pub source: Option<String>,
    pub received_at: String,
}

/// What is stored under the key. `id` is the key itself, so it is not here.
#[derive(Serialize, Deserialize)]
struct StoredCapture {
    text: String,
    source: Option<String>,
    received_at: String,
}

/// Read `synabit://quickcap/new?text=…&source=…`.
///
/// Every intake surface routes through this URL rather than calling a
/// platform-specific entry point, so a new surface is a manifest entry and
/// nothing else. Returns `None` for any URL this app does not claim, which
/// includes the OAuth redirects that share the same scheme.
pub fn capture_from_url(url: &str) -> Option<QueuedCaptureInput> {
    // `com.synabit.app` is what the platforms actually register — see the
    // deep-link block in `tauri.conf.json` and the Android manifest. The
    // shorter spelling is accepted too so that a change of scheme, or a hand-
    // typed URL from a script, does not silently drop somebody's note.
    let rest = SCHEMES.iter().find_map(|scheme| url.strip_prefix(scheme))?;
    let (path, query) = match rest.split_once('?') {
        Some((path, query)) => (path, query),
        None => (rest, ""),
    };

    if path.trim_end_matches('/') != "quickcap/new" {
        return None;
    }

    let mut text = None;
    let mut source = None;
    for pair in query.split('&') {
        let Some((key, value)) = pair.split_once('=') else {
            continue;
        };
        let decoded = percent_decode(value);
        match key {
            "text" => text = Some(decoded),
            "source" => source = Some(decoded),
            _ => {}
        }
    }

    let text = text?;
    if text.trim().is_empty() {
        return None;
    }

    Some(QueuedCaptureInput {
        text,
        source: source.filter(|s| !s.trim().is_empty()),
    })
}

/// What a surface hands over: text, and where it came from.
#[derive(Debug, Clone, PartialEq)]
pub struct QueuedCaptureInput {
    pub text: String,
    pub source: Option<String>,
}

/// Percent-decoding, with `+` read as a space.
///
/// Written out rather than pulled from a crate because the input is a URL
/// assembled by an Android intent or a browser extension, and the failure
/// mode has to be "leave the bytes alone" rather than "reject the capture".
/// A malformed escape means somebody wrote a literal `%`, and losing their
/// note over it would be the wrong trade.
fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;

    while i < bytes.len() {
        match bytes[i] {
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b'%' if i + 2 < bytes.len() => {
                let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).ok();
                match hex.and_then(|h| u8::from_str_radix(h, 16).ok()) {
                    Some(byte) => {
                        out.push(byte);
                        i += 3;
                    }
                    None => {
                        out.push(bytes[i]);
                        i += 1;
                    }
                }
            }
            other => {
                out.push(other);
                i += 1;
            }
        }
    }

    String::from_utf8_lossy(&out).into_owned()
}

/// Put a capture in the queue. Returns its id.
///
/// This is the call every intake surface makes, and it is deliberately the
/// cheapest thing in the app: one row, no vault, no filesystem, no lock.
pub fn enqueue(db: &crate::db::DbBridge, input: &QueuedCaptureInput) -> AppResult<String> {
    let received_at = chrono::Utc::now();

    // Zero-padded so a plain lexicographic sort is arrival order. The counter
    // only ever climbs, so it stays correct across a drain, a restart, and
    // two captures in the same millisecond.
    let seq: u64 = db
        .get_kv(SEQUENCE_KEY)?
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    db.set_kv(SEQUENCE_KEY, &(seq + 1).to_string())?;

    let id = format!("{QUEUE_PREFIX}{seq:012}");

    let stored = StoredCapture {
        text: input.text.clone(),
        source: input.source.clone(),
        received_at: received_at.to_rfc3339(),
    };

    db.set_kv(&id, &serde_json::to_string(&stored)?)?;
    Ok(id)
}

#[tauri::command]
pub fn queue_capture(
    state: tauri::State<'_, DbState>,
    text: String,
    source: Option<String>,
) -> AppResult<String> {
    if text.trim().is_empty() {
        return Err(crate::error::AppError::General(
            "a capture needs some text".to_string(),
        ));
    }
    let db = state.lock().unwrap_or_else(|e| e.into_inner());
    enqueue(&db, &QueuedCaptureInput { text, source })
}

/// Everything waiting, oldest first.
pub fn queued(db: &crate::db::DbBridge) -> AppResult<Vec<QueuedCapture>> {
    let mut rows = db.get_kv_prefix(QUEUE_PREFIX)?;
    rows.sort_by(|a, b| a.0.cmp(&b.0));

    Ok(rows
        .into_iter()
        .filter_map(|(id, raw)| {
            let stored: StoredCapture = serde_json::from_str(&raw).ok()?;
            Some(QueuedCapture {
                id,
                text: stored.text,
                source: stored.source,
                received_at: stored.received_at,
            })
        })
        .collect())
}

/// Where `CaptureActivity` leaves captures taken while the app was not running.
///
/// That activity has no window and no access to this database — it is started
/// by another app's intent, and the Tauri process may be nowhere in memory.
/// It writes one small JSON file per capture instead, and this is the other
/// half of that handoff.
const HANDOFF_DIR: &str = "pending-captures";

/// What the Android side writes. `received_at` is epoch milliseconds there,
/// because a no-window activity has no business formatting timestamps.
#[derive(Deserialize)]
struct HandoffCapture {
    text: String,
    source: Option<String>,
    received_at: Option<i64>,
}

/// Move every handed-off capture into the queue, deleting each file only
/// once its row exists.
///
/// Ordering is the filenames': `CaptureActivity` zero-pads a counter, so a
/// plain sort is arrival order. A file that cannot be read is left alone
/// rather than deleted — it is somebody's note, and a broken one is still
/// better evidence than a missing one.
pub fn import_handoff(db: &crate::db::DbBridge, dir: &std::path::Path) -> AppResult<usize> {
    if !dir.exists() {
        return Ok(0);
    }

    let mut files: Vec<std::path::PathBuf> = std::fs::read_dir(dir)?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|e| e.to_str()) == Some("json"))
        .collect();
    files.sort();

    let mut imported = 0;
    for path in files {
        let Ok(raw) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Ok(handoff) = serde_json::from_str::<HandoffCapture>(&raw) else {
            log::warn!(
                "handoff capture at {} is unreadable; leaving it",
                path.display()
            );
            continue;
        };
        if handoff.text.trim().is_empty() {
            let _ = std::fs::remove_file(&path);
            continue;
        }

        enqueue(
            db,
            &QueuedCaptureInput {
                text: handoff.text,
                source: handoff.source,
            },
        )?;
        let _ = handoff.received_at;

        // Deleted after the row exists. The other order loses the capture if
        // anything fails in between; this one costs a duplicate at worst.
        let _ = std::fs::remove_file(&path);
        imported += 1;
    }

    if imported > 0 {
        log::info!("imported {imported} capture(s) taken while the app was closed");
    }
    Ok(imported)
}

/// The handoff directory, wherever this platform puts app data.
///
/// Both resolvers are tried because Android maps them to the same place and
/// the desktop does not, and guessing wrong would silently strand captures.
fn handoff_dir<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> Option<std::path::PathBuf> {
    use tauri::Manager;
    [
        app.path().app_data_dir().ok(),
        app.path().app_local_data_dir().ok(),
    ]
    .into_iter()
    .flatten()
    .map(|base| base.join(HANDOFF_DIR))
    .find(|dir| dir.exists())
}

#[tauri::command]
pub fn import_handoff_captures(
    app_handle: tauri::AppHandle,
    state: tauri::State<'_, DbState>,
) -> AppResult<usize> {
    let Some(dir) = handoff_dir(&app_handle) else {
        return Ok(0);
    };
    let db = state.lock().unwrap_or_else(|e| e.into_inner());
    import_handoff(&db, &dir)
}

#[tauri::command]
pub fn list_queued_captures(state: tauri::State<'_, DbState>) -> AppResult<Vec<QueuedCapture>> {
    let db = state.lock().unwrap_or_else(|e| e.into_inner());
    queued(&db)
}

/// Forget a capture, once its cap is on disk.
///
/// Called after the write rather than before it: an interruption between
/// the two leaves a duplicate, and a duplicate is recoverable in a way a
/// lost thought is not.
#[tauri::command]
pub fn drop_queued_capture(state: tauri::State<'_, DbState>, id: String) -> AppResult<()> {
    if !id.starts_with(QUEUE_PREFIX) {
        return Err(crate::error::AppError::General(format!(
            "'{id}' is not a queued capture"
        )));
    }
    let db = state.lock().unwrap_or_else(|e| e.into_inner());
    db.delete_kv(&id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::DbBridge;

    fn db() -> DbBridge {
        DbBridge::new_in_memory_full().unwrap()
    }

    fn input(text: &str) -> QueuedCaptureInput {
        QueuedCaptureInput {
            text: text.to_string(),
            source: None,
        }
    }

    // ── the URL every surface routes through ────────────────

    #[test]
    fn reads_text_out_of_a_capture_url() {
        let got =
            capture_from_url("synabit://quickcap/new?text=h%E1%BB%8Dp%20l%C3%BAc%203h").unwrap();
        assert_eq!(got.text, "họp lúc 3h");
        assert_eq!(got.source, None);
    }

    /// What Android and the desktop actually register. A capture arriving on
    /// the registered scheme is the normal case, not the exotic one.
    #[test]
    fn reads_the_scheme_the_platforms_register() {
        let got = capture_from_url(
            "com.synabit.app://quickcap/new?text=chia%20s%E1%BA%BB&source=share-sheet",
        )
        .unwrap();
        assert_eq!(got.text, "chia sẻ");
        assert_eq!(got.source.as_deref(), Some("share-sheet"));
    }

    #[test]
    fn reads_the_source_alongside_it() {
        let got = capture_from_url("synabit://quickcap/new?text=hi&source=share-sheet").unwrap();
        assert_eq!(got.source.as_deref(), Some("share-sheet"));
    }

    /// Android's share sheet and a browser's `URLSearchParams` disagree about
    /// how to encode a space. Both have to work.
    #[test]
    fn accepts_either_spelling_of_a_space() {
        assert_eq!(
            capture_from_url("synabit://quickcap/new?text=two+words")
                .unwrap()
                .text,
            "two words"
        );
        assert_eq!(
            capture_from_url("synabit://quickcap/new?text=two%20words")
                .unwrap()
                .text,
            "two words"
        );
    }

    /// The scheme is shared with the Google Drive OAuth redirect, which must
    /// keep going where it was going.
    #[test]
    fn ignores_urls_this_is_not_for() {
        assert!(capture_from_url("synabit://oauth/callback?code=abc").is_none());
        assert!(capture_from_url("com.synabit.app://oauth2redirect?code=abc").is_none());
        assert!(capture_from_url("https://example.com/quickcap/new?text=hi").is_none());
        assert!(capture_from_url("synabit://quickcap/edit?text=hi").is_none());
    }

    #[test]
    fn a_url_with_nothing_to_capture_is_not_a_capture() {
        assert!(capture_from_url("synabit://quickcap/new").is_none());
        assert!(capture_from_url("synabit://quickcap/new?source=widget").is_none());
        assert!(capture_from_url("synabit://quickcap/new?text=").is_none());
        assert!(capture_from_url("synabit://quickcap/new?text=%20%20").is_none());
    }

    /// A stray `%` is somebody's actual note, not a reason to drop it.
    #[test]
    fn a_broken_escape_keeps_the_text_rather_than_losing_it() {
        assert_eq!(
            capture_from_url("synabit://quickcap/new?text=100%25%20done")
                .unwrap()
                .text,
            "100% done"
        );
        assert_eq!(
            capture_from_url("synabit://quickcap/new?text=50%zz")
                .unwrap()
                .text,
            "50%zz"
        );
        assert_eq!(
            capture_from_url("synabit://quickcap/new?text=ends%2")
                .unwrap()
                .text,
            "ends%2"
        );
    }

    #[test]
    fn survives_an_ampersand_inside_the_text() {
        // Correctly encoded, which is what every sender is expected to do.
        assert_eq!(
            capture_from_url("synabit://quickcap/new?text=a%26b&source=widget")
                .unwrap()
                .text,
            "a&b"
        );
    }

    // ── the queue ───────────────────────────────────────────

    #[test]
    fn a_queued_capture_comes_back_intact() {
        let db = db();
        enqueue(
            &db,
            &QueuedCaptureInput {
                text: "ý tưởng lúc 2h sáng".into(),
                source: Some("widget".into()),
            },
        )
        .unwrap();

        let waiting = queued(&db).unwrap();
        assert_eq!(waiting.len(), 1);
        assert_eq!(waiting[0].text, "ý tưởng lúc 2h sáng");
        assert_eq!(waiting[0].source.as_deref(), Some("widget"));
    }

    /// A share sheet sending several items lands them in one millisecond,
    /// and the order they were written in is the order they must come back.
    #[test]
    fn keeps_arrival_order() {
        let db = db();
        for n in 0..5 {
            enqueue(&db, &input(&format!("cap {n}"))).unwrap();
        }

        let texts: Vec<String> = queued(&db).unwrap().into_iter().map(|c| c.text).collect();
        assert_eq!(texts, ["cap 0", "cap 1", "cap 2", "cap 3", "cap 4"]);
    }

    #[test]
    fn dropping_one_leaves_the_rest() {
        let db = db();
        enqueue(&db, &input("giữ")).unwrap();
        let doomed = enqueue(&db, &input("bỏ")).unwrap();
        enqueue(&db, &input("giữ nữa")).unwrap();

        db.delete_kv(&doomed).unwrap();

        let texts: Vec<String> = queued(&db).unwrap().into_iter().map(|c| c.text).collect();
        assert_eq!(texts, ["giữ", "giữ nữa"]);
    }

    /// The queue shares `kv_store` with the device identity, sync cursors and
    /// migration flags. It must see none of them, and they must survive it.
    #[test]
    fn ignores_everything_else_in_the_key_value_store() {
        let db = db();
        db.set_kv("device_id", "not-a-capture").unwrap();
        db.set_kv("migration:quickcap", "done").unwrap();
        enqueue(&db, &input("thật")).unwrap();

        let waiting = queued(&db).unwrap();
        assert_eq!(waiting.len(), 1);
        assert_eq!(waiting[0].text, "thật");
        assert_eq!(
            db.get_kv("device_id").unwrap().as_deref(),
            Some("not-a-capture")
        );
    }

    /// A row that cannot be read is skipped rather than failing the drain,
    /// so one bad entry cannot block every capture behind it.
    #[test]
    fn a_corrupt_entry_does_not_hold_up_the_others() {
        let db = db();
        enqueue(&db, &input("trước")).unwrap();
        db.set_kv(&format!("{QUEUE_PREFIX}000000000999"), "{not json")
            .unwrap();
        enqueue(&db, &input("sau")).unwrap();

        let texts: Vec<String> = queued(&db).unwrap().into_iter().map(|c| c.text).collect();
        assert_eq!(texts, ["trước", "sau"]);
    }

    // ── the Android handoff ─────────────────────────────────

    fn handoff(dir: &std::path::Path, name: &str, body: &str) {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(dir.join(name), body).unwrap();
    }

    #[test]
    fn a_handed_off_capture_becomes_a_queued_one() {
        let tmp = tempfile::tempdir().unwrap();
        let db = db();
        handoff(
            tmp.path(),
            "000000000000.json",
            r#"{"text":"bôi đen lúc đọc","source":"selected-text","received_at":1}"#,
        );

        assert_eq!(import_handoff(&db, tmp.path()).unwrap(), 1);

        let waiting = queued(&db).unwrap();
        assert_eq!(waiting.len(), 1);
        assert_eq!(waiting[0].text, "bôi đen lúc đọc");
        assert_eq!(waiting[0].source.as_deref(), Some("selected-text"));
    }

    /// The file is the only copy until its row exists, so it must not survive
    /// the import — a second launch would create the cap twice.
    #[test]
    fn an_imported_file_is_taken_away() {
        let tmp = tempfile::tempdir().unwrap();
        let db = db();
        handoff(tmp.path(), "000000000000.json", r#"{"text":"một"}"#);

        import_handoff(&db, tmp.path()).unwrap();

        assert!(!tmp.path().join("000000000000.json").exists());
        assert_eq!(import_handoff(&db, tmp.path()).unwrap(), 0);
        assert_eq!(queued(&db).unwrap().len(), 1);
    }

    /// The Android side zero-pads a counter precisely so that sorting the
    /// filenames is sorting by arrival.
    #[test]
    fn handoff_files_are_imported_in_arrival_order() {
        let tmp = tempfile::tempdir().unwrap();
        let db = db();
        for (name, text) in [
            ("000000000002.json", "ba"),
            ("000000000000.json", "một"),
            ("000000000001.json", "hai"),
        ] {
            handoff(tmp.path(), name, &format!(r#"{{"text":"{text}"}}"#));
        }

        import_handoff(&db, tmp.path()).unwrap();

        let texts: Vec<String> = queued(&db).unwrap().into_iter().map(|c| c.text).collect();
        assert_eq!(texts, ["một", "hai", "ba"]);
    }

    /// A half-written file is somebody's note. Leaving it is recoverable;
    /// deleting it is not.
    #[test]
    fn an_unreadable_handoff_is_left_where_it_is() {
        let tmp = tempfile::tempdir().unwrap();
        let db = db();
        handoff(tmp.path(), "000000000000.json", "{not json");
        handoff(tmp.path(), "000000000001.json", r#"{"text":"đọc được"}"#);

        assert_eq!(import_handoff(&db, tmp.path()).unwrap(), 1);
        assert!(tmp.path().join("000000000000.json").exists());
        assert_eq!(queued(&db).unwrap().len(), 1);
    }

    /// `CaptureActivity` writes to `<name>.part` and renames. A staging file
    /// caught mid-write must not be read as a capture.
    #[test]
    fn a_partial_write_is_not_mistaken_for_a_capture() {
        let tmp = tempfile::tempdir().unwrap();
        let db = db();
        handoff(tmp.path(), "000000000000.json.part", r#"{"text":"chưa xo"#);

        assert_eq!(import_handoff(&db, tmp.path()).unwrap(), 0);
        assert!(queued(&db).unwrap().is_empty());
        assert!(tmp.path().join("000000000000.json.part").exists());
    }

    #[test]
    fn no_handoff_directory_is_not_an_error() {
        let tmp = tempfile::tempdir().unwrap();
        assert_eq!(import_handoff(&db(), &tmp.path().join("nope")).unwrap(), 0);
    }

    #[test]
    fn an_empty_queue_is_empty_rather_than_an_error() {
        assert!(queued(&db()).unwrap().is_empty());
    }

    /// Draining does not rewind the counter, so a capture arriving after a
    /// drain still sorts after one that arrived before it.
    #[test]
    fn order_survives_a_drain() {
        let db = db();
        let first = enqueue(&db, &input("trước khi dọn")).unwrap();
        db.delete_kv(&first).unwrap();
        enqueue(&db, &input("sau khi dọn")).unwrap();

        let waiting = queued(&db).unwrap();
        assert_eq!(waiting.len(), 1);
        assert!(
            waiting[0].id > first,
            "a later capture must not reuse an earlier key: {} vs {}",
            waiting[0].id,
            first
        );
    }
}
