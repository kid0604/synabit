use super::*;

/// Predictable file names, so a plan can be asserted whole.
fn ids() -> impl FnMut() -> String {
    let mut n = 0;
    move || {
        n += 1;
        format!("id{}", n)
    }
}

fn plan(properties: Value) -> PersonPlan {
    plan_person("People/an.md", "An Nguyễn", "uuid-an", &properties, ids())
}

#[test]
fn each_interaction_becomes_its_own_file() {
    // The whole point: two devices adding a coffee at once then write two
    // different files, and there is nothing to merge.
    let got = plan(json!({
        "interactions": [
            { "id": "old-1", "type": "coffee", "date": "2026-08-20", "note": "Talked about the job", "mood": "good" },
            { "id": "old-2", "type": "call", "date": "2026-07-01", "note": "Quick catch-up" },
        ]
    }));

    assert_eq!(got.interactions.len(), 2);
    assert_eq!(got.interactions[0].rel_path, "People/Interactions/id1.md");
    assert_eq!(got.interactions[0].title, "Coffee · An Nguyễn");
    assert_eq!(got.interactions[0].node_type, "interaction");
    assert_eq!(got.interactions[0].content, "Talked about the job");
    assert_eq!(got.interactions[0].properties["date"], json!("2026-08-20"));
    assert_eq!(got.interactions[0].properties["mood"], json!("good"));
    // The person's identity, not their path: the link has to survive their
    // file being moved.
    assert_eq!(got.interactions[0].properties["person_id"], json!("uuid-an"));

    assert_eq!(got.interactions[1].title, "Call · An Nguyễn");
    // No mood recorded means no key, rather than an empty one.
    assert!(got.interactions[1].properties.get("mood").is_none());
}

#[test]
fn the_person_gives_up_the_list_once_the_files_exist() {
    let got = plan(json!({
        "interactions": [{ "type": "coffee", "date": "2026-08-20", "note": "hello" }]
    }));
    assert_eq!(got.patch["interactions"], Value::Null);
}

#[test]
fn nothing_is_taken_out_of_a_person_who_has_nothing_to_give() {
    // An empty patch means the file is not touched at all.
    let got = plan(json!({ "nickname": "Ann", "tags": ["work"] }));
    assert!(got.is_empty(), "{:?}", got);
}

#[test]
fn an_entry_that_records_nothing_does_not_become_a_file() {
    // A stray line of YAML should not turn into a permanent row in somebody's
    // timeline.
    let got = plan(json!({
        "interactions": [
            { "type": "other" },
            { "type": "coffee", "date": "2026-08-20", "note": "real one" },
        ]
    }));
    assert_eq!(got.interactions.len(), 1);
    assert_eq!(got.interactions[0].content, "real one");
}

#[test]
fn an_interaction_with_a_date_and_no_note_is_still_a_meeting() {
    let got = plan(json!({ "interactions": [{ "type": "meeting", "date": "2026-08-20" }] }));
    assert_eq!(got.interactions.len(), 1);
    assert_eq!(got.interactions[0].content, "");
}

#[test]
fn an_empty_list_is_cleared_without_writing_anything() {
    let got = plan(json!({ "interactions": [] }));
    assert!(got.interactions.is_empty());
    assert_eq!(got.patch["interactions"], Value::Null);
}

#[test]
fn the_duplicate_copy_of_the_links_goes() {
    // `relations` held the same links again as markdown mentions, purely so
    // the edge index would notice them. It reads the links directly now.
    let got = plan(json!({
        "relations": ["[Binh](synabit://person/People/binh.md)"],
        "connections": [{ "person_id": "People/binh.md", "relation_type": "friend" }],
    }));
    assert_eq!(got.patch["relations"], Value::Null);
    // The connection itself is left alone: it is the surviving copy.
    assert!(!got.patch.contains_key("connections"), "{:?}", got.patch);
}

#[test]
fn a_name_cached_inside_a_link_is_dropped_and_the_link_kept() {
    let got = plan(json!({
        "connections": [
            { "person_id": "uuid-binh", "name": "Bình", "relation_type": "friend" },
            { "person_id": "uuid-cuong", "relation_type": "colleague" },
        ]
    }));
    assert_eq!(
        got.patch["connections"],
        json!([
            { "person_id": "uuid-binh", "relation_type": "friend" },
            { "person_id": "uuid-cuong", "relation_type": "colleague" },
        ])
    );
}

#[test]
fn links_that_never_cached_a_name_are_left_untouched() {
    // Nothing to fix means nothing written, which means no sync round and no
    // modified time changed for a file that is already correct.
    let got = plan(json!({
        "connections": [{ "person_id": "uuid-binh", "relation_type": "friend" }]
    }));
    assert!(got.is_empty(), "{:?}", got);
}

#[test]
fn running_it_twice_finds_nothing_the_second_time() {
    // What the first pass leaves behind has to be what the second pass calls
    // finished, or every launch rewrites every person.
    let before = json!({
        "interactions": [{ "type": "coffee", "date": "2026-08-20", "note": "hello" }],
        "relations": ["[Binh](synabit://person/People/binh.md)"],
        "connections": [{ "person_id": "uuid-binh", "name": "Bình", "relation_type": "friend" }],
    });
    let first = plan(before.clone());
    assert!(!first.is_empty());

    // Apply the patch the way a write would.
    let mut after = before.as_object().cloned().unwrap();
    for (key, value) in &first.patch {
        if value.is_null() {
            after.remove(key);
        } else {
            after.insert(key.clone(), value.clone());
        }
    }

    let second = plan(Value::Object(after));
    assert!(second.is_empty(), "second pass still wants to do {:?}", second);
}
