use super::*;
use crate::timeline::TimelineStore;

fn vault() -> (tempfile::TempDir, String) {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().canonicalize().unwrap().to_string_lossy().to_string();
    (dir, path)
}

/// One proposal, as a reading would have written it.
fn proposed() -> Extracted {
    Extracted {
        id: "x0123456789abcdef0123".into(),
        kind: "moment".into(),
        happened_from: "2024-06-01".into(),
        happened_to: "2024-06-01".into(),
        precision: "day".into(),
        recorded: "2024-06-02".into(),
        source: "extract".into(),
        evidence: vec![Evidence { node: "Notes/2024-06-02.md".into(), hash: "b-hash".into(), span: Some([0, 38]) }],
        extractor: Extractor { version: EXTRACTOR_VERSION, model: "m".into() },
        confidence: 0.9,
        payload: Payload {
            title: "Khám mắt cho mẹ".into(),
            people: vec!["People/me-ruot.md".into()],
            names: vec!["bác sĩ Long".into()],
            place: Some("bệnh viện".into()),
            quote: "Hôm qua đưa mẹ đi khám mắt ở bệnh viện".into(),
            category: Some("health".into()),
            amount: Some(Amount { value: 300000.0, unit: "VND".into() }),
            about: vec![],
            time: Some("09:30".into()),
            date_basis: Some("relative".into()),
            also: vec![],
        },
        superseded_by: None,
        about_moment: None,
        verdict: None,
    }
}

fn run_of(item: &Extracted, at: DateTime<Utc>) -> SourceRun {
    SourceRun {
        node: "day:2024-06-02".into(),
        hash: "bag-hash".into(),
        version: EXTRACTOR_VERSION,
        model: "m".into(),
        at: crate::utils::timestamp::canonical(at),
        items: vec![item.id.clone()],
        dropped: 0,
        chars: 120,
        ms: 1,
        blocks: vec!["b-hash".into()],
    }
}

const DAILY: &str = "Hôm qua đưa mẹ đi khám mắt ở bệnh viện, bác sĩ bảo phải mổ. Sáng nay chạy 5km.";

#[test]
fn a_day_that_keeps_failing_rests_between_automatic_runs() {
    let key = "extract:test-failing";
    assert!(!resting(key));
    note_failure(key);
    assert!(resting(key));
    clear_failure(key);
    assert!(!resting(key));
    assert_eq!(backoff(1).as_secs(), 30 * 60);
    assert_eq!(backoff(2).as_secs(), 60 * 60);
    assert_eq!(backoff(20).as_secs(), 24 * 60 * 60);
}

#[test]
fn two_writers_to_one_month_file_both_keep_their_change() {
    let (_dir, vault) = vault();
    let now = Utc::now();
    let writers: Vec<_> = (0..8)
        .map(|n| {
            let vault = vault.clone();
            std::thread::spawn(move || {
                let surrogate = crate::timeline::media::Surrogate {
                    node: format!("Files/{n:08}.md"),
                    hash: format!("{n:08}"),
                    kind: "caption".into(),
                    version: 1,
                    model: "m".into(),
                    at: String::new(),
                    text: format!("picture {n}"),
                    segments: Vec::new(),
                    language: None,
                    duration: None,
                    ms: 0,
                };
                record_surrogate(&vault, "dev", surrogate, now).unwrap();
            })
        })
        .collect();
    for writer in writers {
        writer.join().unwrap();
    }
    let path = month_path(&vault, &now.format("%Y-%m").to_string(), "dev");
    let file: MonthFile = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    assert_eq!(file.surrogates.len(), 8);
}

/// What a reading read, block by block, goes to the month file and back —
/// the part of it another device needs so as not to read it again.
#[test]
fn a_reading_keeps_the_blocks_it_read_and_every_new_field() {
    let (_dir, vault) = vault();
    let one = proposed();
    let now = DateTime::parse_from_rfc3339("2024-06-03T00:00:00Z").unwrap().with_timezone(&Utc);
    record(&vault, "dev", run_of(&one, now), std::slice::from_ref(&one), now).unwrap();
    let store = TimelineStore::open_in_memory().unwrap();
    load(store.conn(), &vault).unwrap();

    let blocks: String = store
        .conn()
        .query_row("SELECT blocks FROM extract_runs", [], |r| r.get(0))
        .unwrap();
    assert_eq!(blocks, r#"["b-hash"]"#);
    assert_eq!(item(store.conn(), &one.id).unwrap(), Some(one.clone()));
}

/// A `timeline.db` from before readings were by block still opens, and
/// gains the column rather than failing every query that names it.
#[test]
fn an_older_store_gains_the_blocks_column() {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE extract_runs (month_file TEXT NOT NULL, device TEXT NOT NULL, node_id TEXT NOT NULL,
            hash TEXT NOT NULL, version INTEGER NOT NULL, model TEXT NOT NULL, at TEXT NOT NULL,
            items TEXT NOT NULL, dropped INTEGER NOT NULL, chars INTEGER NOT NULL, ms INTEGER NOT NULL);
         INSERT INTO extract_runs VALUES ('f', 'd', 'Notes/a.md', 'h', 2, 'm', '2026-01-01', '[]', 0, 0, 0);",
    )
    .unwrap();
    ensure_schema(&conn).unwrap();
    ensure_schema(&conn).unwrap();
    let blocks: String = conn.query_row("SELECT blocks FROM extract_runs", [], |r| r.get(0)).unwrap();
    assert_eq!(blocks, "[]");
}

#[test]
fn forgetting_a_conflict_copy_keeps_the_rows_of_the_file_it_copied() {
    let (_dir, vault) = vault();
    let one = proposed();
    let now = DateTime::parse_from_rfc3339("2024-06-03T00:00:00Z").unwrap().with_timezone(&Utc);
    record(&vault, "dev", run_of(&one, now), std::slice::from_ref(&one), now).unwrap();
    let original = Path::new(&vault).join("Timeline/2024/2024-06.dev.json");
    let copy = Path::new(&vault).join("Timeline/2024/2024-06.dev (conflict 1).json");
    std::fs::copy(&original, &copy).unwrap();

    let store = TimelineStore::open_in_memory().unwrap();
    load(store.conn(), &vault).unwrap();
    std::fs::remove_file(&copy).unwrap();
    load(store.conn(), &vault).unwrap();
    assert!(item(store.conn(), &one.id).unwrap().is_some());
}

#[test]
fn a_month_file_that_cannot_be_read_is_never_overwritten() {
    let (_dir, vault_path) = vault();
    let path = Path::new(&vault_path).join("Timeline/2024/2024-06.dev-a.json");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, "{ half a file").unwrap();

    let one = proposed();
    let now = DateTime::parse_from_rfc3339("2024-06-03T00:00:00Z").unwrap().with_timezone(&Utc);
    assert!(record(&vault_path, "dev-a", run_of(&one, now), std::slice::from_ref(&one), now).is_err());
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "{ half a file");

    let store = TimelineStore::open_in_memory().unwrap();
    let loaded = load(store.conn(), &vault_path).unwrap();
    assert_eq!(loaded.unreadable.len(), 1, "{loaded:?}");
}

/// A proposal is often nearly right. Keep-or-discard makes a nearly right one
/// either kept wrong or thrown away; this is the third thing.
#[test]
fn every_field_is_the_persons_to_put_right() {
    let proposed = proposed();
    let edits = Edits {
        title: Some("  Đưa mẹ đi khám mắt  ".into()),
        happened_from: Some("2024-06-02".into()),
        time: Some(String::new()),
        people: Some(vec!["People/me-ruot.md".into(), "chị Yến".into()]),
        place: Some("bệnh viện Mắt".into()),
        category: Some("family".into()),
        amount: Some(250000.0),
        unit: Some("vnd".into()),
        ..Edits::default()
    };
    let (mine, hand) = as_kept(&proposed, &edits).unwrap();
    assert_eq!(mine.payload.title, "Đưa mẹ đi khám mắt", "trimmed, and mine");
    assert_eq!(mine.happened_from, "2024-06-02");
    assert_eq!(mine.payload.time, None, "an empty box takes the clock time off");
    assert_eq!(mine.payload.people, vec!["People/me-ruot.md"]);
    assert_eq!(mine.payload.names, vec!["chị Yến"], "a name nobody has claimed is still a name");
    assert_eq!(mine.payload.category.as_deref(), Some("family"));
    assert_eq!(mine.payload.amount, Some(Amount { value: 250000.0, unit: "VND".into() }));
    assert_eq!(hand, vec!["title", "happened", "time", "where", "category", "people", "amount"]);

    // The evidence is not theirs to write.
    assert_eq!(mine.payload.quote, proposed.payload.quote);
    assert_eq!(mine.evidence, proposed.evidence);
    assert_eq!(mine.id, proposed.id);

    let entry = moment_entry_with(&mine, &hand);
    assert_eq!(entry["title"], "Đưa mẹ đi khám mắt");
    assert_eq!(entry["extract"], proposed.id.as_str());
    assert_eq!(entry["hand"], json!(hand), "what they wrote is never proposed over again");
    assert_eq!(moment_entry(&mine)["id"], moment_entry(&proposed)["id"], "kept twice, reworded or not, is one moment");

    // Nothing sent, nothing changed, and nothing is theirs yet.
    let (untouched, hand) = as_kept(&proposed, &Edits::default()).unwrap();
    assert_eq!(untouched, proposed);
    assert!(hand.is_empty());
    let same = Edits { title: Some(proposed.payload.title.clone()), ..Edits::default() };
    assert!(as_kept(&proposed, &same).unwrap().1.is_empty(), "keeping the sentence is not writing it");
    assert!(as_kept(&proposed, &Edits { title: Some("   ".into()), ..Edits::default() }).is_err());
}

/// Everything the card showed reaches the moment's file.
#[test]
fn a_kept_moment_carries_everything_that_was_read() {
    let entry = moment_entry(&proposed());
    assert_eq!(entry["people"], json!(["People/me-ruot.md", "bác sĩ Long"]));
    assert_eq!(entry["where"], "bệnh viện");
    assert_eq!(entry["category"], "health");
    assert_eq!(entry["amount"], json!({ "value": 300000.0, "unit": "VND" }));
    assert_eq!(entry["time"], "09:30");
    assert_eq!(entry["happened"], "2024-06-01");
}

#[test]
fn keeping_reads_the_note_as_it_is_now() {
    let one = proposed();
    assert!(still_reads_as_read(DAILY, &one));
    assert!(still_reads_as_read(&format!("{DAILY} Tối ăn phở."), &one), "an edit elsewhere does not lock Keep");
    assert!(!still_reads_as_read("Chỉ còn một câu khác hẳn.", &one));
}

/// A link reads as its label, and the model quotes it that way.
#[test]
fn a_quote_is_found_in_a_note_with_links_in_it() {
    let text = "Chiều nay [Phan Hương Lê](synabit://node/People/le.md) gọi **trao đổi** về [[Hợp đồng MDP|hợp đồng]] mới.";
    let [start, end] = find_quote(text, "Phan Hương Lê gọi trao đổi về hợp đồng mới").expect("found").expect("and placed");
    let marked: String = text.chars().skip(start).take(end - start).collect();
    assert!(marked.starts_with("Phan Hương Lê") && marked.ends_with("hợp đồng]] mới"), "{marked}");
    assert!(find_quote(text, "Phan Hương Lê gọi xin nghỉ việc").is_none());
}

#[test]
fn every_note_is_read_now_and_nothing_the_reader_wrote() {
    let off = Config::default();
    for t in ["interaction", "person", "quickcap", "event", "note", "task"] {
        assert!(wanted(t, &json!({}), "x.md", &off), "{t}");
    }
    assert!(!wanted("moment", &json!({ "timeline": true }), "Moments/x.md", &off), "never a moment");
    assert!(!wanted("syn_memory", &json!({ "timeline": true }), "m.md", &off));
    // A task is read on the day it was finished; one still open is a plan,
    // and `recorded` gives it no day of its own.
    assert_eq!(
        recorded("task", &json!({ "completed_at": "2026-07-21T10:00:00+07:00" }), "2026-07-01T00:00:00Z")
            .map(|(day, by_day)| (when::iso(day), by_day)),
        Some(("2026-07-21".into(), true))
    );
    assert!(!wanted("note", &json!({ "date": "2024-06-02", "timeline": false }), "d.md", &off));
    let on = Config { folders: vec!["Books/".into()], ..Config::default() };
    assert!(wanted("book", &json!({}), "Books/a.md", &on), "a kind of one's own, by folder");
}

/// A moment kept from a conversation cannot be derived from the vault again,
/// so the links it names are written when it is loaded. Sealing reads those
/// links.
#[test]
fn an_accepted_moment_names_everyone_it_named() {
    let (_dir, vault_path) = vault();
    let mut moment = proposed();
    moment.payload.people = vec!["uuid-a".into(), "uuid-sealed".into(), "uuid-c".into()];
    let now = Utc::now();
    decide(
        &vault_path,
        "macbook",
        Decision {
            item: moment.id.clone(),
            decision: "accepted".into(),
            node: "Syn/chat.md".into(),
            at: crate::utils::timestamp::canonical(now),
            moment: Some(moment.clone()),
        },
        now,
    )
    .unwrap();

    let timeline = TimelineStore::open_in_memory().unwrap();
    load(timeline.conn(), &vault_path).unwrap();
    let row = format!("{}#accepted", moment.id);
    let mut stmt = timeline
        .conn()
        .prepare("SELECT node_id FROM event_links WHERE event_id = ?1 AND role = 'with' ORDER BY node_id")
        .unwrap();
    let named: Vec<String> = stmt.query_map([&row], |r| r.get(0)).unwrap().flatten().collect();
    assert_eq!(named, vec!["uuid-a", "uuid-c", "uuid-sealed"]);
}

/// What was left out is kept on this device, the latest reading of a place
/// replacing the one before.
#[test]
fn what_was_left_out_is_kept_with_its_reason() {
    let store = TimelineStore::open_in_memory().unwrap();
    let drops = |gate: &'static str| Drops {
        node: "day:2026-07-09".into(),
        hash: "h".into(),
        model: "m".into(),
        at: "2026-09-23T00:00:00.000Z".into(),
        each: vec![(gate, json!({ "title": "Gọi cho mẹ", "time": "21:03" }))],
    };
    remember_drops(store.conn(), &[drops("undated")]).unwrap();
    remember_drops(store.conn(), &[drops("no_quote")]).unwrap();
    let kept: Vec<(String, String)> = store
        .conn()
        .prepare("SELECT gate, raw FROM extract_drops")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .flatten()
        .collect();
    assert_eq!(kept.len(), 1);
    assert_eq!(kept[0].0, "no_quote");
    assert!(kept[0].1.contains("21:03"));
}
