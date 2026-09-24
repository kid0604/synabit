use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Mutex;

use serde_json::json;

use super::*;
use crate::models::node::NodeMetadata;
use crate::models::syn::{ModelInfo, ProviderStatus, SynProvider};
use crate::syn::provider::{ChatReply, StreamSink};
use crate::timeline::extract::{self, Decision};
use crate::timeline::TimelineStore;

fn day(text: &str) -> NaiveDate {
    NaiveDate::parse_from_str(text, "%Y-%m-%d").unwrap()
}

fn node(id: &str, node_type: &str, content: &str, properties: Value) -> NodeMetadata {
    NodeMetadata {
        id: id.into(),
        node_type: node_type.into(),
        title: properties
            .get("title")
            .and_then(Value::as_str)
            .map(String::from)
            .unwrap_or_else(|| id.rsplit('/').next().unwrap_or(id).trim_end_matches(".md").into()),
        content: content.into(),
        properties,
        created_at: "2026-07-01T08:00:00.000Z".into(),
        updated_at: "2026-07-01T08:00:00.000Z".into(),
        timestamp: 0,
        blocks: None,
    }
}

fn db_with(nodes: Vec<NodeMetadata>) -> DbBridge {
    let db = DbBridge::new_in_memory_full().expect("schema");
    for n in nodes {
        db.upsert_node(&n).expect("seeded");
    }
    db
}

fn vault() -> (tempfile::TempDir, String) {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().canonicalize().unwrap().to_string_lossy().to_string();
    (dir, path)
}

fn no_history(_: &str) -> History {
    History::default()
}

fn plan_of(db: &DbBridge, store: &TimelineStore, vault_path: &str, today: &str) -> (Plan, Directory) {
    let config = Config { enabled: true, ..Config::default() };
    plan_in(db, store.conn(), vault_path, &config, day(today), None, &no_history).expect("plan")
}

const DAILY: &str = "Chiều họp UAT v2 với chị Yến, chốt lại ngày làm việc trên giao diện.\n\nTrưa ăn bún chả với Nga ở Hàng Mành, 50k.\n\nMai golive.\n";

fn the_vault() -> DbBridge {
    db_with(vec![
        node("People/me.md", "person", "", json!({ "title": "Anh", "is_owner": true })),
        node("People/yen.md", "person", "", json!({ "title": "Nguyễn Thị Yến", "relationship_type": "Đồng Nghiệp" })),
        node("People/nga.md", "person", "", json!({ "title": "Nga", "nickname": "Nga béo", "relationship_type": "Bạn Đại Học" })),
        node("People/cam.md", "person", "", json!({ "title": "Cam", "nickname": "bé Cam", "relationship_type": "Con" })),
        node("People/hung.md", "person", "", json!({ "title": "Trần Văn Hùng", "relationship_type": "Đồng Nghiệp" })),
        node("Notes/2026-07-21.md", "note", DAILY, json!({ "title": "2026-07-21", "date": "2026-07-21" })),
        node("Events/uat.md", "event", "", json!({ "title": "UAT v2 — MDP", "start_at": "2026-07-21T14:00:00+07:00" })),
        node("Tasks/checklist.md", "task", "", json!({ "title": "Chốt checklist golive", "completed_at": "2026-07-21T10:00:00+07:00" })),
    ])
}

// ─── Who is who ──────────────────────────────────────────────────

#[test]
fn the_directory_knows_nicknames_family_and_who_is_writing() {
    let directory = Directory::read(&the_vault()).unwrap();
    assert_eq!(directory.writer.as_ref().map(|w| w.name.as_str()), Some("Anh"));
    assert!(directory.people.iter().all(|p| p.name != "Anh"), "the writer is not one of the people");

    let yen = directory.people.iter().find(|p| p.name == "Nguyễn Thị Yến").unwrap();
    assert!(yen.aliases.contains(&"Yến".to_string()), "the given name, nobody else's: {yen:?}");
    assert!(!yen.family);
    let cam = directory.people.iter().find(|p| p.name == "Cam").unwrap();
    assert!(cam.family, "a child is family: {cam:?}");

    assert_eq!(directory.find("chị Yến").map(|p| p.id.as_str()), Some("People/yen.md"));
    assert_eq!(directory.find("Nga béo").map(|p| p.id.as_str()), Some("People/nga.md"));
    assert!(directory.find("Đức").is_none());

    // Named, or family: the rest of the office is not in every bag.
    let named: Vec<String> = directory.in_text("chiều họp với chị Yến").into_iter().map(|p| p.name).collect();
    assert!(named.contains(&"Nguyễn Thị Yến".to_string()) && named.contains(&"Cam".to_string()), "{named:?}");
    assert!(!named.contains(&"Trần Văn Hùng".to_string()), "{named:?}");
    // A name inside another word is not the name.
    assert!(!directory.in_text("ngày hôm nay nganh").iter().any(|p| p.name == "Nga"));
}

// ─── Days ────────────────────────────────────────────────────────

/// A daily note is about its day, however late it was typed (§4.2): six of
/// forty-one blocks on the real vault were typed the morning after.
#[test]
fn a_daily_note_puts_every_block_on_its_day() {
    let content = "Tối đưa Cam đi bơi.\n\nSáng nay họp.\n";
    let row = NodeRow {
        id: "Notes/2026-07-21.md",
        node_type: "note",
        title: "2026-07-21",
        content,
        properties: &json!({ "date": "2026-07-21" }),
        created_at: "2026-07-21T08:00:00Z",
    };
    let mut history = History::default();
    // Typed four days later, still the 21st's.
    for block in blocks::split(content) {
        history.first.insert(block.hash, 1_784_900_000_000);
    }
    let read = source(&row, history, day("2026-09-01")).unwrap();
    assert!(read.blocks.iter().all(|(_, d, estimated)| *d == day("2026-07-21") && !estimated), "{:?}", read.blocks);
}

/// Any other note is written over many days, and each block is on the day
/// its words first appeared — unless they were there when the history
/// began, when the day the file was made is the better guess.
#[test]
fn a_block_of_any_other_note_is_on_the_day_it_was_written() {
    let content = "Dự án Golive TCB.\n\nHôm nay chốt checklist với chị Yến.\n\nHôm nay golive thành công.\n";
    let blocks = blocks::split(content);
    let at = |d: &str| {
        day(d).and_hms_opt(10, 0, 0).unwrap().and_local_timezone(Local).unwrap().timestamp_millis()
    };
    let mut history = History { began: Some(at("2026-07-10")), ..History::default() };
    history.first.insert(blocks[0].hash.clone(), at("2026-07-10"));
    history.first.insert(blocks[1].hash.clone(), at("2026-07-21"));
    history.first.insert(blocks[2].hash.clone(), at("2026-08-01"));
    let row = NodeRow {
        id: "Projects/golive.md",
        node_type: "note",
        title: "Golive TCB",
        content,
        properties: &json!({}),
        created_at: "2026-07-05T08:00:00Z",
    };
    let read = source(&row, history, day("2026-09-01")).unwrap();
    let days: Vec<(String, bool)> = read.blocks.iter().map(|(_, d, e)| (when::iso(*d), *e)).collect();
    assert_eq!(
        days,
        vec![("2026-07-05".into(), true), ("2026-07-21".into(), false), ("2026-08-01".into(), false)],
        "the first block predates the history; the other two are each their own day"
    );
}

// ─── What is left to read ────────────────────────────────────────

/// Everything written on a day goes into that day's bag: the note's
/// sentences, the task ticked off, the calendar entry — each a block, each
/// something the reading may quote (§14, chốt 2026-09-23).
#[test]
fn new_blocks_go_into_the_bag_of_their_day() {
    let (_dir, vault_path) = vault();
    let db = the_vault();
    let store = TimelineStore::open_in_memory().unwrap();
    let (plan, _) = plan_of(&db, &store, &vault_path, "2026-09-01");
    let bag = plan.pending.iter().find(|b| b.day == day("2026-07-21")).expect("the 21st");
    let read: Vec<&str> = bag.read.iter().map(|b| b.block.text.as_str()).collect();
    assert_eq!(read.len(), 5, "{read:?}");
    assert!(read.contains(&"Chốt checklist golive"), "the task ticked off: {read:?}");
    assert!(read.contains(&"UAT v2 — MDP (14:00)"), "the calendar entry, with its hour: {read:?}");
    assert_eq!(bag.writer.as_deref(), Some("Anh"));
    assert!(plan.old_version.is_empty() && plan.done == 0);

    // A task is on the day it was finished, not the day it was written down.
    let sent = message(bag);
    assert!(sent.contains("· task, finished"), "{sent}");
    assert!(sent.contains("· calendar"), "{sent}");
}

fn read_block(hash: &str) -> Read {
    Read { blocks: [hash.to_string()].into_iter().collect(), ..Read::default() }
}

#[test]
fn a_block_read_before_or_edited_a_little_is_not_read_again() {
    let content = "Chiều họp UAT v2 với chị Yến, chốt lại ngày làm việc trên giao diện.";
    let row = NodeRow { id: "Notes/a.md", node_type: "note", title: "a", content, properties: &json!({ "date": "2026-07-21" }), created_at: "" };
    let block = blocks::split(content).remove(0);

    let read = read_block(&block.hash);
    let unchanged = source(&row, History::default(), day("2026-09-01")).unwrap();
    assert_eq!(standing(&unchanged, &unchanged.blocks[0].0, &read), Standing::Read);

    // A word added: the same block. Known because the history holds the old one.
    let edited = "Chiều họp UAT v2 với chị Yến, chốt lại ngày làm việc trên giao diện mới.";
    let mut history = History::default();
    history.text.insert(block.hash.clone(), block.text.clone());
    let row = NodeRow { content: edited, ..row };
    let now = source(&row, history.clone(), day("2026-09-01")).unwrap();
    assert_eq!(standing(&now, &now.blocks[0].0, &read), Standing::Read);

    // A new thought in its place is new.
    let row = NodeRow { content: "Tối đưa Cam đi bơi ở bể Hoàng Mai.", ..row };
    let other = source(&row, history, day("2026-09-01")).unwrap();
    assert_eq!(standing(&other, &other.blocks[0].0, &read), Standing::New);
}

/// Upgrading must not send the whole vault back to the model on its own. A
/// note the second reader read is offered for reading again, not read.
#[test]
fn what_the_reader_before_read_waits_to_be_asked_for() {
    let content = "Sáng nay chạy 5km quanh hồ Tây.\n\nChiều cà phê với Tuấn.";
    let row = NodeRow { id: "Notes/b.md", node_type: "note", title: "b", content, properties: &json!({}), created_at: "2026-07-01T08:00:00Z" };
    let blocks = blocks::split(content);
    let mut history = History::default();
    history.first.insert(blocks[0].hash.clone(), 1_000);
    history.first.insert(blocks[1].hash.clone(), 5_000);
    let read_whole = Read { notes: [("Notes/b.md".to_string(), 3_000)].into_iter().collect(), ..Read::default() };
    let now = source(&row, history, day("2026-09-01")).unwrap();
    assert_eq!(standing(&now, &now.blocks[0].0, &read_whole), Standing::ReadBefore, "there when it was read");
    assert_eq!(standing(&now, &now.blocks[1].0, &read_whole), Standing::New, "written after");

    let same_text = Read { whole: [now.fingerprint.clone()].into_iter().collect(), ..Read::default() };
    assert_eq!(standing(&now, &now.blocks[1].0, &same_text), Standing::ReadBefore, "the whole note, as it is, was read");
}

#[test]
fn a_long_day_is_read_in_more_than_one_call() {
    // One paragraph per sentence, so each note is many blocks.
    let long = |n: usize| (0..60).map(|i| format!("Câu {n}.{i} kể một chuyện dài dòng trong ngày hôm đó.")).collect::<Vec<_>>().join("\n\n");
    let db = db_with(vec![
        node("Notes/a.md", "note", &long(1), json!({ "date": "2026-07-21" })),
        node("Notes/b.md", "note", &long(2), json!({ "date": "2026-07-21" })),
        node("Notes/c.md", "note", &long(3), json!({ "date": "2026-07-21" })),
    ]);
    let (_dir, vault_path) = vault();
    let store = TimelineStore::open_in_memory().unwrap();
    let (plan, _) = plan_of(&db, &store, &vault_path, "2026-09-01");
    assert!(plan.pending.len() >= 2, "{:?}", plan.pending.iter().map(|b| (&b.key, b.chars())).collect::<Vec<_>>());
    assert!(plan.pending.iter().all(|b| b.key.starts_with("2026-07-21#")));
    for bag in &plan.pending {
        assert!(bag.chars() <= MOST_READ, "{} holds {} chars", bag.key, bag.chars());
    }
}

/// Gate: no assistant turn reaches the model. Checked on
/// what is sent, not on what comes back.
#[test]
fn what_is_sent_never_holds_what_syn_said() {
    let (_dir, vault_path) = vault();
    let db = db_with(vec![
        node("Notes/open.md", "note", "Đi ăn tối với Tuấn ở quán cũ.", json!({ "date": "2026-09-10" })),
        node("Notes/essay.md", "note", "Một bài viết về cách pha cà phê ngon.", json!({})),
        node("Notes/off.md", "note", "MARK-TIMELINE-FALSE hôm nay.", json!({ "date": "2026-09-12", "timeline": false })),
        node("Moments/6f3c.md", "moment", "MARK-A-MOMENT viết thêm.", json!({ "title": "Ăn tối", "happened": "2026-09-10" })),
    ]);
    std::fs::create_dir_all(Path::new(&vault_path).join("Syn")).unwrap();
    std::fs::write(
        Path::new(&vault_path).join("Syn/c1.json"),
        json!({
            "id": "c1",
            "title": "Nhà mới",
            "messages": [
                { "id": "1", "role": "user", "content": "Hôm qua tao chuyển sang nhà mới ở Cầu Giấy.", "timestamp": "2026-09-14T03:00:00Z" },
                { "id": "2", "role": "assistant", "content": "MARK-WHAT-SYN-SAID chúc mừng", "timestamp": "2026-09-14T03:00:05Z" },
                { "id": "3", "role": "system", "content": "MARK-SYSTEM", "timestamp": "2026-09-14T03:00:00Z" }
            ]
        })
        .to_string(),
    )
    .unwrap();

    let store = TimelineStore::open_in_memory().unwrap();
    let config = Config { enabled: true, conversations: true, ..Config::default() };
    let (plan, _) = plan_in(&db, store.conn(), &vault_path, &config, day("2026-09-15"), None, &no_history).unwrap();
    let sent: String = plan.pending.iter().map(message).collect::<Vec<_>>().join("\n");

    assert!(sent.contains("Tuấn") && sent.contains("Cầu Giấy"), "what should be read is: {sent}");
    assert!(sent.contains("pha cà phê"), "every note is read now, not only dated ones: {sent}");
    for mark in ["MARK-WHAT-SYN-SAID", "MARK-SYSTEM", "MARK-TIMELINE-FALSE", "MARK-A-MOMENT"] {
        assert!(!sent.contains(mark), "{mark} reached the model");
    }

    let config = Config { enabled: true, ..Config::default() };
    let (without, _) = plan_in(&db, store.conn(), &vault_path, &config, day("2026-09-15"), None, &no_history).unwrap();
    assert!(
        without.pending.iter().flat_map(|b| &b.read).all(|b| !b.node.starts_with("Syn/")),
        "conversations are off by default"
    );
}

// ─── Asking ──────────────────────────────────────────────────────

#[test]
fn the_message_says_what_the_model_needs_and_no_more() {
    let (_dir, vault_path) = vault();
    let db = the_vault();
    let store = TimelineStore::open_in_memory().unwrap();
    let (plan, _) = plan_of(&db, &store, &vault_path, "2026-09-01");
    let bag = plan.pending.iter().find(|b| b.day == day("2026-07-21")).unwrap();
    let sent = message(bag);
    assert!(sent.contains("WRITER: Anh"), "{sent}");
    assert!(sent.contains("DAY: 2026-07-21, Tuesday."), "{sent}");
    assert!(sent.contains("Nguyễn Thị Yến") && sent.contains("also: Yến"), "named: {sent}");
    assert!(sent.contains("Cam") && sent.contains("family"), "family is always there: {sent}");
    assert!(!sent.contains("Trần Văn Hùng"), "nobody named him: {sent}");
    // The calendar entry and the task ticked off are blocks like any other,
    // each saying what it is.
    assert!(sent.contains("UAT v2 — MDP (14:00)") && sent.contains("· calendar"), "{sent}");
    assert!(sent.contains("Chốt checklist golive") && sent.contains("· task, finished"), "{sent}");
    assert!(sent.contains("[b1 · READ]    2026-07-21\nChiều họp UAT v2 với chị Yến"), "{sent}");
}

#[test]
fn the_schema_asks_for_what_the_checks_read() {
    let kinds = Config::default().categories();
    let shape = schema(&kinds);
    let item = &shape["properties"]["moments"]["items"];
    for field in ["title", "date", "date_basis", "category", "quote"] {
        assert!(item["required"].as_array().unwrap().contains(&json!(field)), "{field}");
    }
    assert_eq!(item["properties"]["category"]["enum"], json!(DEFAULT_CATEGORIES));
    assert!(INSTRUCTIONS.contains("{\"moments\": []} is a good answer"));
}

/// The kinds are the vault's, not the code's: a kind somebody added is one
/// the model may answer with, and one nobody listed is not.
#[test]
fn the_kinds_a_moment_can_be_come_from_the_vault() {
    let (_dir, vault_path) = vault();
    let db = the_vault();
    let store = TimelineStore::open_in_memory().unwrap();
    let config = Config {
        enabled: true,
        categories: vec!["ăn uống".into(), "  Sự Cố ".into(), "".into()],
        ..Config::default()
    };
    let (plan, directory) =
        plan_in(&db, store.conn(), &vault_path, &config, day("2026-09-01"), None, &no_history).unwrap();
    let bag = plan.pending.iter().find(|b| b.day == day("2026-07-21")).unwrap();
    // Trimmed, lowercased, and "other" is always there: a reading needs a way
    // to say it does not know.
    assert_eq!(bag.categories, vec!["ăn uống", "sự cố", "other"]);
    assert!(message(bag).contains("KINDS (category): ăn uống, sự cố, other"), "{}", message(bag));
    assert_eq!(schema(&bag.categories)["properties"]["moments"]["items"]["properties"]["category"]["enum"], json!(["ăn uống", "sự cố", "other"]));

    let (items, _) = settle(
        bag,
        raw(json!([
            { "title": "Họp UAT", "date": "2026-07-21", "category": "Sự cố", "quote": { "source": "b1", "text": "Chiều họp UAT v2" } },
            { "title": "Ăn trưa", "date": "2026-07-21", "category": "meeting", "quote": { "source": "b2", "text": "Trưa ăn bún chả" } }
        ])),
        &directory,
        "m",
    );
    assert_eq!(items[0].payload.category.as_deref(), Some("sự cố"));
    assert_eq!(items[1].payload.category.as_deref(), Some("other"), "a kind this vault does not keep is not a kind");

    // And an empty list is the defaults, not no kinds at all.
    assert_eq!(Config::default().categories(), DEFAULT_CATEGORIES.iter().map(|k| k.to_string()).collect::<Vec<_>>());
}

#[test]
fn a_reply_is_read_through_fences_and_chatter() {
    let reply = "Here you go:\n```json\n{\"moments\": [{\"title\": \"Chạy\", \"date\": \"2026-07-21\", \"quote\": {\"source\": \"b1\", \"text\": \"chạy 5km\"}}]}\n```";
    assert_eq!(parse_reply(reply).unwrap()[0].title, "Chạy");
    assert_eq!(parse_reply("{\"moments\": []}"), Some(vec![]));
    assert_eq!(parse_reply("I could not find anything."), None);
}

// ─── Checking ────────────────────────────────────────────────────

fn day_bag() -> (Bag, Directory) {
    let (_dir, vault_path) = vault();
    let db = the_vault();
    let store = TimelineStore::open_in_memory().unwrap();
    let (plan, directory) = plan_of(&db, &store, &vault_path, "2026-09-01");
    let mut bag = plan.pending.into_iter().find(|b| b.day == day("2026-07-21")).unwrap();
    bag.context.push(("2026-07-20".into(), "Hôm qua đi Đà Lạt với Nga.".into()));
    (bag, directory)
}

fn raw(value: Value) -> Vec<RawMoment> {
    serde_json::from_value::<Vec<RawMoment>>(value).unwrap()
}

#[test]
fn a_good_reply_becomes_proposals_with_everything_it_said() {
    let (bag, directory) = day_bag();
    // Family first, then whoever is named: the code is whatever the bag gave.
    let yen = bag.people.iter().find(|(_, p)| p.name == "Nguyễn Thị Yến").map(|(c, _)| c.clone()).unwrap();
    assert_eq!(bag.people[0].1.name, "Cam", "family leads: {:?}", bag.people);
    let (items, dropped) = settle(
        &bag,
        raw(json!([
            { "title": "Họp UAT v2 với chị Yến", "date": "2026-07-21", "time": "14:00", "date_basis": "the_day",
              "people": [{ "ref": yen }, { "name": "Đức" }], "about": ["UAT v2", "MDP"], "category": "meeting",
              "quote": { "source": "b1", "text": "Chiều họp UAT v2 với chị Yến" }, "also": ["b4"] },
            { "title": "Ăn trưa bún chả với Nga", "date": "2026-07-21", "date_basis": "the_day",
              "people": [{ "name": "Nga béo" }], "place": "Hàng Mành", "category": "meal",
              "amount": { "value": 50000, "unit": "vnd" },
              "quote": { "source": "b2", "text": "Trưa ăn bún chả với Nga ở Hàng Mành, 50k." } }
        ])),
        &directory,
        "m",
    );
    assert_eq!(dropped.total(), 0, "{dropped:?}");
    let meeting = &items[0];
    assert_eq!(meeting.payload.people, vec!["People/yen.md"]);
    assert_eq!(meeting.payload.names, vec!["Đức"], "a name that matches nobody is kept as written");
    assert_eq!(meeting.payload.time.as_deref(), Some("14:00"));
    assert_eq!(meeting.payload.category.as_deref(), Some("meeting"));
    assert_eq!(meeting.payload.also, vec!["Events/uat.md"]);
    assert_eq!(meeting.evidence[0].node, "Notes/2026-07-21.md");
    assert_eq!(meeting.recorded, "2026-07-21");

    let lunch = &items[1];
    assert_eq!(lunch.payload.people, vec!["People/nga.md"], "found by nickname");
    assert_eq!(lunch.payload.amount, Some(Amount { value: 50000.0, unit: "VND".into() }));
    assert_eq!(lunch.payload.place.as_deref(), Some("Hàng Mành"));
    // Where the quote sits in the note, for marking it.
    let [from, to] = lunch.evidence[0].span.unwrap();
    let marked: String = DAILY.chars().skip(from).take(to - from).collect();
    assert_eq!(marked, "Trưa ăn bún chả với Nga ở Hàng Mành, 50k.");
}

#[test]
fn the_checks_leave_out_what_does_not_stand_up_and_say_why() {
    let (bag, directory) = day_bag();
    let (items, dropped) = settle(
        &bag,
        raw(json!([
            { "title": "Golive", "date": "2026-07-22", "date_basis": "relative", "category": "work",
              "quote": { "source": "b3", "text": "Mai golive." } },
            { "title": "Đi Đà Lạt", "date": "2026-07-20", "date_basis": "relative", "category": "trip",
              "quote": { "source": "x1", "text": "Hôm qua đi Đà Lạt với Nga." } },
            { "title": "Họp", "date": "2026-07-21", "date_basis": "the_day", "category": "meeting",
              "quote": { "source": "b1", "text": "Họp với sếp tổng về lương" } },
            { "title": "Họp", "date": "hôm nào đó", "date_basis": "the_day", "category": "meeting",
              "quote": { "source": "b1", "text": "Chiều họp UAT v2" } },
            { "title": "  ", "date": "2026-07-21", "quote": { "source": "b1", "text": "Chiều họp UAT v2" } },
            { "title": "Họp UAT", "date": "2026-07-21", "category": "meeting", "quote": { "source": "b1", "text": "Chiều họp UAT v2" } },
            { "title": "họp  UAT", "date": "2026-07-21", "category": "meeting", "quote": { "source": "b1", "text": "Chiều họp UAT v2" } }
        ])),
        &directory,
        "m",
    );
    assert_eq!(items.len(), 1, "{items:?}");
    let gates: Vec<&str> = dropped.each.iter().map(|(gate, _)| *gate).collect();
    assert_eq!(gates, vec!["not_yet", "no_quote", "no_quote", "undated", "untitled", "repeated"]);
    assert_eq!(dropped.each[1].1["quote"]["source"], "x1", "what the model said is kept, as it said it");
}

#[test]
fn what_is_already_kept_that_day_is_not_proposed_again() {
    let (mut bag, directory) = day_bag();
    bag.kept.push("Họp UAT v2 với chị Yến".into());
    let (items, dropped) = settle(
        &bag,
        raw(json!([{ "title": "Họp UAT v2 với chị Yến", "date": "2026-07-21", "category": "meeting",
                     "quote": { "source": "b1", "text": "Chiều họp UAT v2" } }])),
        &directory,
        "m",
    );
    assert!(items.is_empty());
    assert_eq!(dropped.repeated, 1);
}

#[test]
fn stretches_of_time_are_kept_as_stretches() {
    let (bag, directory) = day_bag();
    let (items, _) = settle(
        &bag,
        raw(json!([
            { "title": "Học đại học Bách Khoa", "date": "2009", "date_to": "2013", "precision": "year", "date_basis": "explicit",
              "category": "milestone", "quote": { "source": "b1", "text": "Chiều họp UAT v2" } },
            { "title": "Làm ở MDP", "date": "2026-03", "ongoing": true, "precision": "month", "date_basis": "explicit",
              "category": "work", "quote": { "source": "b1", "text": "chốt lại ngày làm việc" } },
            { "title": "Sửa giao diện", "date": "2026-07-13", "precision": "week", "date_basis": "relative",
              "category": "work", "quote": { "source": "b1", "text": "trên giao diện" } }
        ])),
        &directory,
        "m",
    );
    let spans: Vec<(&str, &str)> = items.iter().map(|i| (i.happened_from.as_str(), i.happened_to.as_str())).collect();
    assert_eq!(spans, vec![("2009-01-01", "2013-12-31"), ("2026-03-01", "2026-07-21"), ("2026-07-13", "2026-07-19")]);
}

#[test]
fn the_writer_is_never_one_of_the_people() {
    let (bag, directory) = day_bag();
    let (items, _) = settle(
        &bag,
        raw(json!([{ "title": "Họp", "date": "2026-07-21", "category": "meeting",
                     "people": [{ "name": "tôi" }, { "name": "Anh" }, "chị Yến"],
                     "quote": { "source": "b1", "text": "Chiều họp UAT v2" } }])),
        &directory,
        "m",
    );
    assert_eq!(items[0].payload.people, vec!["People/yen.md"], "a bare name is read too");
    assert!(items[0].payload.names.is_empty(), "{:?}", items[0].payload.names);
}

#[test]
fn a_category_nobody_listed_is_other_and_a_clock_that_is_not_one_is_dropped() {
    let (bag, directory) = day_bag();
    let (items, _) = settle(
        &bag,
        raw(json!([{ "title": "Họp", "date": "2026-07-21", "category": "gathering", "time": "chiều",
                     "date_basis": "inferred", "quote": { "source": "b1", "text": "Chiều họp UAT v2" } },
                   { "title": "Ăn trưa", "date": "2026-07-21", "category": "meal", "time": "9:05",
                     "date_basis": "the_day", "quote": { "source": "b2", "text": "Trưa ăn bún chả" } }])),
        &directory,
        "m",
    );
    assert_eq!(items[0].payload.category.as_deref(), Some("other"));
    assert_eq!(items[0].payload.time, None);
    assert_eq!(items[1].payload.time.as_deref(), Some("09:05"));
    assert!(items[0].confidence < items[1].confidence, "a guessed day is less sure");
}

// ─── Running ─────────────────────────────────────────────────────

/// A provider that fails the test if it is asked anything.
struct Refusing;

#[async_trait::async_trait]
impl ChatProvider for Refusing {
    fn id(&self) -> SynProvider {
        SynProvider::Ollama
    }
    async fn check_status(&self) -> AppResult<ProviderStatus> {
        unreachable!()
    }
    async fn list_models(&self) -> AppResult<Vec<ModelInfo>> {
        unreachable!()
    }
    async fn chat(&self, _: ChatRequest<'_>) -> AppResult<ChatReply> {
        panic!("a model was asked")
    }
    async fn chat_streaming(&self, _: ChatRequest<'_>, _: &StreamSink<'_>) -> AppResult<ChatReply> {
        panic!("a model was asked")
    }
}

/// A provider that gives the same reply, and remembers what it was asked. It
/// can be made to refuse a schema, the way a server that does not know the
/// field does.
struct Replying {
    reply: &'static str,
    refuses_schema: bool,
    asked: Mutex<Vec<(bool, String)>>,
}

impl Replying {
    fn new(reply: &'static str) -> Self {
        Replying { reply, refuses_schema: false, asked: Mutex::new(Vec::new()) }
    }
}

#[async_trait::async_trait]
impl ChatProvider for Replying {
    fn id(&self) -> SynProvider {
        SynProvider::Ollama
    }
    async fn check_status(&self) -> AppResult<ProviderStatus> {
        unreachable!()
    }
    async fn list_models(&self) -> AppResult<Vec<ModelInfo>> {
        unreachable!()
    }
    async fn chat(&self, req: ChatRequest<'_>) -> AppResult<ChatReply> {
        let sent = req.messages.iter().map(|m| m.content.clone()).collect::<Vec<_>>().join("\n");
        self.asked.lock().unwrap().push((req.json_schema.is_some(), sent));
        if self.refuses_schema && req.json_schema.is_some() {
            return Err(AppError::General("unknown field response_format".into()));
        }
        Ok(ChatReply { content: self.reply.to_string(), tool_calls: Vec::new(), usage: Default::default(), duration_ms: Some(1200) })
    }
    async fn chat_streaming(&self, req: ChatRequest<'_>, _: &StreamSink<'_>) -> AppResult<ChatReply> {
        self.chat(req).await
    }
}

const REPLY: &str = r#"{"moments": [{"title": "Ăn trưa bún chả với Nga", "date": "2026-07-21", "date_basis": "the_day", "category": "meal", "quote": {"source": "b2", "text": "Trưa ăn bún chả với Nga ở Hàng Mành"}}]}"#;

fn files_outside_timeline(vault_path: &str) -> BTreeMap<String, Vec<u8>> {
    crate::sync::utils::collect_local_files(vault_path)
        .into_iter()
        .filter(|rel| !crate::timeline::is_timeline_path(rel))
        .map(|rel| {
            let bytes = std::fs::read(Path::new(vault_path).join(&rel)).unwrap_or_default();
            (rel, bytes)
        })
        .collect()
}

#[tokio::test]
async fn a_request_is_held_to_the_schema_and_asked_again_without_when_refused() {
    let (bag, directory) = day_bag();
    let held = Replying::new(REPLY);
    read_bag(&held, "m", 8192, &bag, &directory, Utc::now()).await.unwrap();
    let asked = held.asked.lock().unwrap().clone();
    assert_eq!(asked.len(), 1);
    assert!(asked[0].0, "the schema was sent");
    assert!(asked[0].1.starts_with("You read a person's own notes"), "the instructions come first, for caching");

    let old_server = Replying { refuses_schema: true, ..Replying::new(REPLY) };
    let (items, _, _) = read_bag(&old_server, "m", 8192, &bag, &directory, Utc::now()).await.unwrap();
    assert_eq!(items.len(), 1);
    let asked: Vec<bool> = old_server.asked.lock().unwrap().iter().map(|(schema, _)| *schema).collect();
    assert_eq!(asked, vec![true, false]);
}

/// Gate: nothing is written into the vault's own notes by reading, and what
/// was read is not read again — here or on any device that syncs the month
/// file.
#[tokio::test]
async fn reading_writes_only_under_timeline_and_is_not_repeated() {
    let (_dir, vault_path) = vault();
    std::fs::create_dir_all(Path::new(&vault_path).join("Notes")).unwrap();
    std::fs::write(Path::new(&vault_path).join("Notes/2026-07-21.md"), format!("---\ndate: 2026-07-21\n---\n{DAILY}")).unwrap();
    let before = files_outside_timeline(&vault_path);

    let db = the_vault();
    let store = TimelineStore::open_in_memory().unwrap();
    let (plan, directory) = plan_of(&db, &store, &vault_path, "2026-09-01");
    let report = read_all(&Replying::new(REPLY), "m", 8192, &plan.pending, &directory, &vault_path, "dev-a").await;
    assert_eq!((report.failed.len(), report.items), (0, 1), "{report:?}");
    assert_eq!(files_outside_timeline(&vault_path), before, "a note changed before anything was kept");

    extract::load(store.conn(), &vault_path).unwrap();
    let (again, _) = plan_of(&db, &store, &vault_path, "2026-09-01");
    assert!(again.pending.is_empty(), "{:?}", again.pending.iter().map(|b| &b.key).collect::<Vec<_>>());
    assert!(again.done >= 3);
    let report = read_all(&Refusing, "m", 8192, &again.pending, &directory, &vault_path, "dev-a").await;
    assert_eq!(report.read, 0);
}

fn note_in(db: &DbBridge) -> impl Fn(&str, &str) -> Option<(String, extract::NoteNow)> + '_ {
    move |id: &str, quote: &str| {
        let found = db.get_node(id).ok().flatten().or_else(|| {
            db.get_all_nodes()
                .ok()?
                .into_iter()
                .filter(|n| n.node_type != "moment")
                .find(|n| extract::find_quote(&n.content, quote).is_some())
        })?;
        Some((found.id.clone(), extract::NoteNow { title: found.title, node_type: found.node_type, content: found.content }))
    }
}

#[tokio::test]
async fn proposals_follow_the_note_and_say_when_their_words_are_gone() {
    let (_dir, vault_path) = vault();
    let db = the_vault();
    let store = TimelineStore::open_in_memory().unwrap();
    let (plan, directory) = plan_of(&db, &store, &vault_path, "2026-09-01");
    read_all(&Replying::new(REPLY), "m", 8192, &plan.pending, &directory, &vault_path, "dev-a").await;
    extract::load(store.conn(), &vault_path).unwrap();
    let title_of = |id: &str| directory.title(id).map(String::from);

    let shown = extract::proposals(store.conn(), &note_in(&db), &extract::reviewed(&vault_path), &title_of).unwrap();
    assert_eq!(shown.len(), 1);
    assert_eq!(shown[0].category.as_deref(), Some("meal"));
    assert!(!shown[0].stale);

    // An edit elsewhere in the note: still good to keep.
    db.upsert_node(&node("Notes/2026-07-21.md", "note", &format!("{DAILY}\nTối ngủ sớm."), json!({ "date": "2026-07-21" }))).unwrap();
    let shown = extract::proposals(store.conn(), &note_in(&db), &extract::reviewed(&vault_path), &title_of).unwrap();
    assert!(!shown[0].stale);

    // The words it stands on taken out: it stays, marked.
    db.upsert_node(&node("Notes/2026-07-21.md", "note", &DAILY.replace("Trưa ăn bún chả với Nga ở Hàng Mành, 50k.", "Trưa ở nhà."), json!({ "date": "2026-07-21" }))).unwrap();
    let shown = extract::proposals(store.conn(), &note_in(&db), &extract::reviewed(&vault_path), &title_of).unwrap();
    assert!(shown[0].stale, "{shown:?}");

    // Moved: found where it is now, by its words, and kept into it there.
    db.delete_node("Notes/2026-07-21.md").unwrap();
    db.upsert_node(&node("Journal/2026-07-21.md", "note", DAILY, json!({ "date": "2026-07-21" }))).unwrap();
    let shown = extract::proposals(store.conn(), &note_in(&db), &extract::reviewed(&vault_path), &title_of).unwrap();
    assert_eq!(shown[0].node_id, "Journal/2026-07-21.md");
    assert!(!shown[0].stale);

    // Gone altogether: so is what was read from it.
    db.delete_node("Journal/2026-07-21.md").unwrap();
    let gone = extract::proposals(store.conn(), &note_in(&db), &extract::reviewed(&vault_path), &title_of).unwrap();
    assert!(gone.is_empty(), "{gone:?}");
    db.upsert_node(&node("Journal/2026-07-21.md", "note", DAILY, json!({ "date": "2026-07-21" }))).unwrap();

    // Declined on another device is declined.
    extract::decide(
        &vault_path,
        "dev-b",
        Decision { item: shown[0].id.clone(), decision: "declined".into(), node: shown[0].node_id.clone(), at: "2026-09-15T00:00:00.000Z".into(), moment: None },
        Utc::now(),
    )
    .unwrap();
    let shown = extract::proposals(store.conn(), &note_in(&db), &extract::reviewed(&vault_path), &title_of).unwrap();
    assert!(shown.is_empty());
}

/// Not in any answer the timeline gives until it is kept.
#[tokio::test]
async fn a_proposal_is_in_no_answer_until_it_is_kept() {
    let (_dir, vault_path) = vault();
    let db = the_vault();
    let store = TimelineStore::open_in_memory().unwrap();
    let (plan, directory) = plan_of(&db, &store, &vault_path, "2026-09-01");
    read_all(&Replying::new(REPLY), "m", 8192, &plan.pending, &directory, &vault_path, "dev-a").await;
    extract::load(store.conn(), &vault_path).unwrap();
    let answered = store.query(when::parse("2026-07").unwrap(), day("2026-09-15")).unwrap();
    assert!(answered.is_empty(), "{answered:?}");
}

#[test]
fn one_line_is_read_the_way_a_day_is() {
    let directory = Directory::read(&the_vault()).unwrap();
    let bag = bag_of_line("hôm qua ăn trưa với chị Yến ở Kim Mã 150k", day("2026-09-16"), &directory).unwrap();
    assert_eq!(bag.read.len(), 1);
    assert!(message(&bag).contains("Nguyễn Thị Yến"));
    assert!(bag_of_line("   ", day("2026-09-16"), &directory).is_none());
}

/// Counting what is left to read must not read anything.
///
/// Every screen that shows the timeline asks this, so it is answered a day at
/// a time from two queries rather than by working out the plan: opening the
/// settings used to walk the whole vault, which is three quarters of a second
/// on a real one and fifteen on a large one. What it gives up is written down
/// on `Left`.
#[test]
fn what_is_left_to_read_is_counted_by_the_day_without_opening_a_note() {
    let db = db_with(vec![
        node("Notes/2026-07-01.md", "note", DAILY, json!({ "date": "2026-07-01" })),
        node("Notes/2026-07-02.md", "note", "Sáng đi bơi với Cam.\n", json!({ "date": "2026-07-02" })),
        // Tomorrow is not late: a day that has not happened is not unread.
        node("Notes/2026-07-09.md", "note", "Hẹn khám mắt.\n", json!({ "date": "2026-07-09" })),
    ]);
    let store = TimelineStore::open_in_memory().unwrap();
    let config = Config { enabled: true, ..Config::default() };
    let before = db.conn().total_changes();

    let left = left_to_read(&db, store.conn(), &config, day("2026-07-08")).expect("counted");
    assert_eq!(left.days, 2, "two days written on and none read");
    assert!(left.chars > DAILY.chars().count(), "the writing on those days: {left:?}");
    assert_eq!(left.old_version, 0);
    assert_eq!(db.conn().total_changes(), before, "counting wrote to the vault");

    // A reading covers a day, and the day stops being left.
    extract::ensure_schema(store.conn()).unwrap();
    let mut mark = |key: &str, version: u32| {
        store
            .conn()
            .execute(
                "INSERT INTO extract_runs (month_file, device, node_id, hash, version, model, at, items, dropped, chars, ms, blocks)
                 VALUES ('m', 'dev-a', ?1, 'h', ?2, 'm', '2026-07-08T00:00:00.000Z', '[]', 0, 0, 0, '[]')",
                rusqlite::params![key, version],
            )
            .unwrap();
    };
    mark("day:2026-07-01", extract::EXTRACTOR_VERSION);
    let left = left_to_read(&db, store.conn(), &config, day("2026-07-08")).expect("counted");
    assert_eq!(left.days, 1, "{left:?}");

    // Covered by an older reader is not covered by this one, and says so
    // rather than hiding in the count of what is left.
    mark("day:2026-07-02", extract::EXTRACTOR_VERSION - 1);
    let left = left_to_read(&db, store.conn(), &config, day("2026-07-08")).expect("counted");
    assert_eq!((left.days, left.old_version), (1, 1), "{left:?}");

    // A long day read in two calls is one day either way.
    mark("day:2026-07-02#2", extract::EXTRACTOR_VERSION);
    let left = left_to_read(&db, store.conn(), &config, day("2026-07-08")).expect("counted");
    assert_eq!((left.days, left.old_version), (0, 0), "{left:?}");
}

/// The golden set of §10, read by the real model. Spends real credit, and
/// writes nothing: it prints what each day would propose and what each gate
/// left out, for a person to judge.
///
/// The days are the ones the review of 2026-09-22 found losing moments. Their
/// notes stay in the vault; nothing of them is copied into this repository.
///
/// ```bash
/// cp "$HOME/Library/Application Support/com.synabit.app/vault_cache.db" /tmp/cache.db
/// SYN_EVAL_CACHE=/tmp/cache.db cargo test --lib reader::tests::live -- --ignored --nocapture
/// ```
mod live {
    use super::*;

    const GOLDEN_DAYS: &[&str] = &["2026-07-09", "2026-06-06", "2026-09-11", "2026-05-26", "2026-07-06", "2026-07-08"];

    /// What this reader would read on a real vault, and how much of it —
    /// before anything is sent anywhere. No model is asked and nothing is
    /// written; the month files are loaded into a store in memory.
    #[test]
    #[ignore = "reads a real vault; run by hand"]
    fn what_the_real_vault_would_cost() {
        let cache_path = std::env::var("SYN_EVAL_CACHE").expect("a copy of the vault cache");
        let vault_path = std::env::var("SYN_EVAL_VAULT")
            .unwrap_or_else(|_| format!("{}/Documents/vault", std::env::var("HOME").unwrap_or_default()));
        let conn = rusqlite::Connection::open(&cache_path).expect("the cache");
        let db = DbBridge::init_with_conn(conn).expect("its schema");
        let config = extract::read_config(&vault_path);
            let store = TimelineStore::open_in_memory().unwrap();
        extract::load(store.conn(), &vault_path).expect("month files");
        let today = chrono::Local::now().date_naive();
        let (plan, directory) = plan_in(&db, store.conn(), &vault_path, &config, today, None, &no_history).expect("plan");
        let chars = |bags: &[Bag]| bags.iter().map(Bag::chars).sum::<usize>();
        let blocks = |bags: &[Bag]| bags.iter().map(|b| b.read.len()).sum::<usize>();
        let notes = |bags: &[Bag]| bags.iter().flat_map(|b| b.read.iter().map(|r| r.node.clone())).collect::<HashSet<_>>().len();
        eprintln!(
            "\npeople {} (family {}), writer {:?}\nnew:        {} bags, {} blocks, {} notes, {} chars\nread by v2: {} bags, {} blocks, {} notes, {} chars\nread by v3: {} blocks\n",
            directory.people.len(),
            directory.people.iter().filter(|p| p.family).count(),
            directory.writer.as_ref().map(|w| &w.name),
            plan.pending.len(), blocks(&plan.pending), notes(&plan.pending), chars(&plan.pending),
            plan.old_version.len(), blocks(&plan.old_version), notes(&plan.old_version), chars(&plan.old_version),
            plan.done,
        );
        let mut biggest: Vec<&Bag> = plan.pending.iter().chain(&plan.old_version).collect();
        biggest.sort_by_key(|b| std::cmp::Reverse(b.chars()));
        for bag in biggest.iter().take(6) {
            let longest = bag.read.iter().map(|r| r.block.text.chars().count()).max().unwrap_or(0);
            eprintln!("  biggest: {} · {} blocks · {} chars · longest block {}", bag.key, bag.read.len(), bag.chars(), longest);
        }
        for bag in plan.pending.iter().take(12) {
            let from: HashSet<&str> = bag.read.iter().map(|r| r.node.as_str()).collect();
            eprintln!("  {} · {} blocks · {} chars · {} people · {} notes", bag.key, bag.read.len(), bag.chars(), bag.people.len(), from.len());
        }
    }

    /// Where the time goes when the tray asks what is waiting.
    ///
    /// `timeline_extract_status` is one command, and the whole Nexus app waits
    /// on it: it holds the vault's connection while it walks every note. This
    /// times each piece of it against a real vault so the slow one is known
    /// rather than guessed at.
    #[test]
    #[ignore = "reads a real vault; run by hand"]
    fn where_the_time_goes_asking_what_is_waiting() {
        let cache_path = std::env::var("SYN_EVAL_CACHE").expect("a copy of the vault cache");
        let vault_path = std::env::var("SYN_EVAL_VAULT")
            .unwrap_or_else(|_| format!("{}/Documents/vault", std::env::var("HOME").unwrap_or_default()));
        let conn = rusqlite::Connection::open(&cache_path).expect("the cache");
        let db = DbBridge::init_with_conn(conn).expect("its schema");
        let config = extract::read_config(&vault_path);
        let today = chrono::Local::now().date_naive();
        let took = |what: &str, at: std::time::Instant| eprintln!("  {what:<28} {:>7} ms", at.elapsed().as_millis());

        // The vault this runs against: whichever vault_id has documents.
        let vault_id: Option<String> = db
            .conn()
            .prepare("SELECT vault_id, COUNT(*) c FROM sync_document_paths GROUP BY vault_id ORDER BY c DESC LIMIT 1")
            .and_then(|mut stmt| stmt.query_row([], |r| r.get(0)))
            .ok();
        eprintln!("\nvault_id {vault_id:?}");

        let at = std::time::Instant::now();
        let store = TimelineStore::open_in_memory().unwrap();
        extract::load(store.conn(), &vault_path).expect("month files");
        took("extract::load", at);
        // The memo of the plan is keyed on these; a load that writes when
        // nothing changed would make it miss every time.
        let after_first = store.conn().total_changes();
        extract::load(store.conn(), &vault_path).expect("month files again");
        eprintln!("  timeline changes {} then {}", after_first, store.conn().total_changes());

        let at = std::time::Instant::now();
        let directory = Directory::read(&db).expect("directory");
        took("Directory::read", at);

        let at = std::time::Instant::now();
        let _days = days(&db).expect("days");
        took("days", at);

        // Splitting every note the reader may look at. Not the database:
        // asking for each note's words one at a time, instead of in the one
        // query, made no difference at all — 154 ms against 155 — so this is
        // `pulldown_cmark` and the hashing, and that is what it costs.
        let at = std::time::Instant::now();
        let plain = sources(&db, &vault_path, &config, today, None, &no_history).expect("sources");
        took("sources (no history)", at);
        eprintln!("  {} sources", plain.len());

        let docs = std::cell::Cell::new(0usize);
        let history = |rel: &str| -> History {
            docs.set(docs.get() + 1);
            let Some(vault_id) = vault_id.as_deref() else { return History::default() };
            match db.get_node_id_by_path(vault_id, rel) {
                Ok(Some(doc_id)) => db
                    .get_crdt_doc(vault_id, &doc_id)
                    .map(|doc| History::of(&doc))
                    .unwrap_or_default(),
                _ => History::default(),
            }
        };
        // And replaying each note's edit history, which is two thirds of it.
        // Every note pays it, including the ones whose blocks all take the
        // day the note is about: `standing` reads a note's history to know an
        // edit of a block already read, whatever kind of note it is.
        let at = std::time::Instant::now();
        let with = sources(&db, &vault_path, &config, today, None, &history).expect("sources");
        took("sources (real history)", at);
        eprintln!("  {} sources, {} docs asked for", with.len(), docs.get());

        let at = std::time::Instant::now();
        let read = Read::so_far(store.conn()).expect("read");
        took("Read::so_far", at);

        let at = std::time::Instant::now();
        let kept = crate::timeline::moments::kept(&db).expect("kept");
        took("moments::kept", at);
        eprintln!("  {} kept", kept.len());

        let at = std::time::Instant::now();
        let the_days = days(&db).expect("days");
        let plan = plan(&with, &read, &the_days, &directory);
        took("plan", at);

        let at = std::time::Instant::now();
        let changed = changes(&with, &kept, &read, &directory);
        took("changes", at);
        eprintln!("  {} bags, {} changes", plan.pending.len(), changed.len());

        // And the cheap count the screens actually show, which reads no note.
        let at = std::time::Instant::now();
        let left = left_to_read(&db, store.conn(), &config, today).expect("what is left");
        took("left_to_read", at);
        eprintln!("  {left:?}");
    }

    #[tokio::test]
    #[ignore = "spends real API credit; run by hand"]
    async fn golden_days() {
        let cache_path = std::env::var("SYN_EVAL_CACHE").expect("a copy of the vault cache");
        let vault_path = std::env::var("SYN_EVAL_VAULT")
            .unwrap_or_else(|_| format!("{}/Documents/vault", std::env::var("HOME").unwrap_or_default()));
        let days: Vec<NaiveDate> = std::env::var("SYN_EVAL_DAYS")
            .map(|list| list.split(',').map(|d| day(d.trim())).collect())
            .unwrap_or_else(|_| GOLDEN_DAYS.iter().map(|d| day(d)).collect());

        let conn = rusqlite::Connection::open(&cache_path).expect("the cache");
        let db = DbBridge::init_with_conn(conn).expect("its schema");
        let settings = crate::syn::settings::load_settings(&vault_path).expect("the real Syn settings");
        let config = extract::read_config(&vault_path);
        let (settings, model) = extract::reader(&config, &settings);
        let model = model.expect("a model");
        assert!(
            crate::timeline::media::runs_here(&settings) || config.allow_cloud,
            "this vault has not allowed a model off this machine"
        );
        let provider = crate::syn::provider::for_settings(
            &settings,
            crate::secrets::SecretManager::get_syn_api_key(None, settings.provider.key_slot()),
        );

            let today = chrono::Local::now().date_naive();
        let directory = Directory::read(&db).expect("people");
        let every = sources(&db, &vault_path, &Config { enabled: true, ..config.clone() }, today, None, &no_history)
            .expect("sources");
        // As though nothing had been read: the point is what this reader makes of them.
        let plan = plan(&every, &Read::default(), &super::days(&db).expect("days"), &directory);
        let bags: Vec<&Bag> = plan.pending.iter().filter(|bag| days.contains(&bag.day)).collect();

        eprintln!("\n═══ golden days with {model} ═══  {} bags\n", bags.len());
        let (mut proposed, mut left_out) = (0, 0);
        for bag in bags {
            eprintln!("── {} · {} blocks · {} people offered", bag.key, bag.read.len(), bag.people.len());
            match read_bag(provider.as_ref(), &model, settings.num_ctx, bag, &directory, Utc::now()).await {
                Ok((items, _, dropped)) => {
                    for item in &items {
                        let who: Vec<String> = item
                            .payload
                            .people
                            .iter()
                            .map(|id| directory.title(id).unwrap_or(id).to_string())
                            .chain(item.payload.names.iter().map(|n| format!("?{n}")))
                            .collect();
                        eprintln!(
                            "   ✓ {} {} · {} · {:?} · {} · {:?}\n     “{}”",
                            item.happened_from,
                            item.payload.time.as_deref().unwrap_or(""),
                            item.payload.title,
                            who,
                            item.payload.category.as_deref().unwrap_or(""),
                            item.payload.amount,
                            item.payload.quote
                        );
                    }
                    for (gate, raw) in &dropped.each {
                        eprintln!("   ✗ {gate}: {raw}");
                    }
                    proposed += items.len();
                    left_out += dropped.total();
                }
                Err(e) => eprintln!("   ! {e}"),
            }
        }
        eprintln!("\nproposed {proposed}, left out {left_out}\n");
    }
}

// ─── When the source changes under a kept moment (§15) ──────────

/// The block of the day's note the moment below was read from.
fn first_block() -> String {
    blocks::split(DAILY).remove(0).hash
}

fn a_kept_moment(source_node: &str, quote: &str, hand: &[&str]) -> crate::timeline::moments::Kept {
    kept_moment_of(source_node, quote, hand, &first_block())
}

fn kept_moment_of(source_node: &str, quote: &str, hand: &[&str], block: &str) -> crate::timeline::moments::Kept {
    let fields: serde_json::Map<String, Value> = serde_json::from_value(json!({
        "type": "moment",
        "title": "Họp UAT v2 với chị Yến",
        "happened": "2026-07-21",
        "people": ["People/yen.md"],
        "category": "meeting",
        "source": { "node": source_node, "quote": quote, "block": block },
        "hand": hand,
    }))
    .unwrap();
    crate::timeline::moments::kept_from("Moments/6f3c.md".into(), fields).expect("a kept moment")
}

fn sources_of(db: &DbBridge, vault_path: &str) -> Vec<Source> {
    sources(db, vault_path, &Config { enabled: true, ..Config::default() }, day("2026-09-01"), None, &no_history)
        .expect("sources")
}

#[test]
fn a_moment_is_left_alone_while_the_words_it_stands_on_are_there() {
    let (_dir, vault_path) = vault();
    let db = the_vault();
    let directory = Directory::read(&db).unwrap();
    let moment = a_kept_moment("Notes/2026-07-21.md", "Chiều họp UAT v2 với chị Yến", &[]);
    let found = changes(&sources_of(&db, &vault_path), std::slice::from_ref(&moment), &Read::default(), &directory);
    assert!(found.is_empty(), "{found:?}");

    // Bold put round it: the same words, so the same block.
    let bold = DAILY.replace("Chiều họp UAT v2 với chị Yến", "**Chiều họp UAT v2 với chị Yến**");
    db.upsert_node(&node("Notes/2026-07-21.md", "note", &bold, json!({ "date": "2026-07-21" }))).unwrap();
    let found = changes(&sources_of(&db, &vault_path), std::slice::from_ref(&moment), &Read::default(), &directory);
    assert!(found.is_empty(), "markup is not a change: {found:?}");

    // And a moment moved out of a note before blocks were recorded has only
    // the words to go on.
    let old = kept_moment_of("Notes/2026-07-21.md", "Chiều họp UAT v2 với chị Yến", &[], "");
    let old = crate::timeline::moments::Kept { block: None, ..old };
    assert!(changes(&sources_of(&db, &vault_path), &[old], &Read::default(), &directory).is_empty());
}

#[test]
fn an_edited_sentence_is_asked_about_once_for_each_state_it_is_in() {
    let (_dir, vault_path) = vault();
    let db = the_vault();
    let directory = Directory::read(&db).unwrap();
    let moment = a_kept_moment("Notes/2026-07-21.md", "Chiều họp UAT v2 với chị Yến", &[]);
    db.upsert_node(&node(
        "Notes/2026-07-21.md",
        "note",
        &DAILY.replace("Chiều họp UAT v2 với chị Yến,", "Chiều họp UAT v2 với chị Yến và Đức,"),
        json!({ "date": "2026-07-21" }),
    ))
    .unwrap();

    let found = changes(&sources_of(&db, &vault_path), std::slice::from_ref(&moment), &Read::default(), &directory);
    assert_eq!(found.len(), 1);
    assert!(matches!(found[0].became, Became::Edited(_)), "{:?}", found[0].became);
    assert!(change_message(&found[0]).contains("Họp UAT v2 với chị Yến"), "the moment as it is kept");
    assert!(change_message(&found[0]).contains("và Đức"), "and the block as it is now");

    // Asked once: the same state is not asked about again.
    let asked = Read { changes: [format!("{}\0{}", found[0].key(), found[0].hash)].into_iter().collect(), ..Read::default() };
    assert!(changes(&sources_of(&db, &vault_path), std::slice::from_ref(&moment), &asked, &directory).is_empty());

    // Edited again: a different state, so a different question.
    db.upsert_node(&node(
        "Notes/2026-07-21.md",
        "note",
        &DAILY.replace("Chiều họp UAT v2 với chị Yến,", "Chiều họp UAT v2 với chị Yến, Đức và anh Hùng,"),
        json!({ "date": "2026-07-21" }),
    ))
    .unwrap();
    assert_eq!(changes(&sources_of(&db, &vault_path), std::slice::from_ref(&moment), &asked, &directory).len(), 1);
}

#[test]
fn words_that_are_gone_and_notes_that_are_gone_are_said_without_asking_a_model() {
    let (_dir, vault_path) = vault();
    let db = the_vault();
    let directory = Directory::read(&db).unwrap();
    let moment = a_kept_moment("Notes/2026-07-21.md", "Chiều họp UAT v2 với chị Yến", &["title"]);

    db.upsert_node(&node("Notes/2026-07-21.md", "note", "Trưa ở nhà, không đi đâu cả.", json!({ "date": "2026-07-21" }))).unwrap();
    let found = changes(&sources_of(&db, &vault_path), std::slice::from_ref(&moment), &Read::default(), &directory);
    assert_eq!(found[0].became, Became::Retracted);

    db.delete_node("Notes/2026-07-21.md").unwrap();
    let found = changes(&sources_of(&db, &vault_path), std::slice::from_ref(&moment), &Read::default(), &directory);
    assert_eq!(found[0].became, Became::NoteGone);

    // No model is asked, and the card carries the moment as it stands.
    let (items, run, _) = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(read_change(&Refusing, "m", 8192, &found[0], &directory, Utc::now()))
        .unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].verdict.as_deref(), Some("gone"));
    assert_eq!(items[0].about_moment.as_deref(), Some("Moments/6f3c.md"));
    assert_eq!(items[0].payload.title, "Họp UAT v2 với chị Yến");
    assert_eq!(run.node, "moment:Moments/6f3c.md");
}

#[tokio::test]
async fn an_edit_that_adds_somebody_is_offered_as_a_change_to_the_moment() {
    let (_dir, vault_path) = vault();
    let db = the_vault();
    let directory = Directory::read(&db).unwrap();
    db.upsert_node(&node(
        "Notes/2026-07-21.md",
        "note",
        &DAILY.replace("Chiều họp UAT v2 với chị Yến,", "Chiều họp UAT v2 với chị Yến và Nga,"),
        json!({ "date": "2026-07-21" }),
    ))
    .unwrap();
    let moment = a_kept_moment("Notes/2026-07-21.md", "Chiều họp UAT v2 với chị Yến", &["title"]);
    let found = changes(&sources_of(&db, &vault_path), std::slice::from_ref(&moment), &Read::default(), &directory);

    let said = r#"{"verdict": "changed", "moment": {"title": "Họp UAT v2 với chị Yến và Nga", "date": "2026-07-21", "category": "meeting", "people": [{"name": "chị Yến"}, {"name": "Nga"}], "quote": {"source": "b1", "text": "Chiều họp UAT v2 với chị Yến và Nga"}}}"#;
    let (items, run, _) = read_change(&Replying::new(said), "m", 8192, &found[0], &directory, Utc::now()).await.unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].verdict.as_deref(), Some("changed"));
    assert_eq!(items[0].payload.people, vec!["People/yen.md", "People/nga.md"]);
    assert_eq!(run.items, vec![items[0].id.clone()]);

    // A verdict of "unchanged" is a reading too — so nobody is asked twice —
    // and proposes nothing.
    let said = r#"{"verdict": "unchanged"}"#;
    let (items, run, _) = read_change(&Replying::new(said), "m", 8192, &found[0], &directory, Utc::now()).await.unwrap();
    assert!(items.is_empty());
    assert_eq!(run.hash, found[0].hash);
}

/// A change carries the moment's own fields; the tray holds them up against
/// what is kept. What the person wrote is named, so it can be left alone.
#[test]
fn a_change_says_which_fields_the_person_wrote_themselves() {
    let (_dir, vault_path) = vault();
    let db = the_vault();
    let directory = Directory::read(&db).unwrap();
    db.upsert_node(&node("Notes/2026-07-21.md", "note", &DAILY.replace("chị Yến,", "chị Yến và Đức,"), json!({ "date": "2026-07-21" }))).unwrap();
    let moment = a_kept_moment("Notes/2026-07-21.md", "Chiều họp UAT v2 với chị Yến", &["title"]);
    let found = changes(&sources_of(&db, &vault_path), std::slice::from_ref(&moment), &Read::default(), &directory);
    let sent = change_message(&found[0]);
    assert!(sent.contains("leave them as they are: title"), "{sent}");
    assert!(ABOUT_A_CHANGE.contains("\"retracted\""));
}

#[test]
fn what_the_person_put_right_is_told_to_the_next_reading() {
    let store = TimelineStore::open_in_memory().unwrap();
    for correction in [
        Correction { field: "title".into(), before: "Họp với team".into(), after: "Họp UAT v2 với MDP".into() },
        Correction { field: "person".into(), before: "Cam".into(), after: "Cam (con)".into() },
    ] {
        remember_correction(store.conn(), &correction).unwrap();
    }
    let learned = corrections(store.conn()).unwrap();
    assert_eq!(learned.len(), 2);

    let (_dir, vault_path) = vault();
    let db = the_vault();
    let (plan, _) = plan_of(&db, &store, &vault_path, "2026-09-01");
    let sent = message(plan.pending.iter().find(|b| b.day == day("2026-07-21")).unwrap());
    assert!(sent.contains("CORRECTIONS THE WRITER MADE BEFORE"), "{sent}");
    assert!(sent.contains("title \"Họp với team\" → \"Họp UAT v2 với MDP\""), "{sent}");
    assert!(sent.contains("people \"Cam\" → Cam (con)"), "{sent}");
}
