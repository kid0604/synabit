use super::*;
use serde_json::Map;

fn contact(title: &str, properties: Value) -> ImportedContact {
    ImportedContact {
        title: title.to_string(),
        properties: properties.as_object().cloned().unwrap_or_default(),
        body: String::new(),
        photo: None,
    }
}

fn with_email(title: &str, email: &str) -> ImportedContact {
    contact(
        title,
        json!({ "details": [{ "label": "Email", "value": email, "type": "email" }] }),
    )
}

fn with_phone(title: &str, phone: &str) -> ImportedContact {
    contact(
        title,
        json!({ "details": [{ "label": "Mobile Phone", "value": phone, "type": "phone" }] }),
    )
}

// ─── Keys ───────────────────────────────────────────────────

#[test]
fn two_spellings_of_one_address_are_one_key() {
    assert_eq!(email_key("An@Example.COM").as_deref(), Some("an@example.com"));
    assert_eq!(email_key("  an@example.com ").as_deref(), Some("an@example.com"));
    assert_eq!(email_key("mailto:an@example.com").as_deref(), Some("an@example.com"));
}

#[test]
fn something_that_is_not_an_address_is_not_a_key() {
    // Matching on these would join people who have nothing to do with each other.
    for bad in ["", "an", "an@", "@example.com", "an@localhost", "not an email"] {
        assert!(email_key(bad).is_none(), "{:?}", bad);
    }
}

#[test]
fn one_number_written_five_ways_is_one_key() {
    // The case that matters: the same Vietnamese mobile as a phone exports it,
    // as a person types it, and as a spreadsheet mangles it.
    let key = phone_key("+84 90 123 4567");
    assert!(key.is_some());
    for spelling in ["+84901234567", "0901234567", "090 123 4567", "(090) 123-4567", "84901234567"] {
        assert_eq!(phone_key(spelling), key, "{:?}", spelling);
    }
}

#[test]
fn something_too_short_to_identify_anybody_is_not_a_key() {
    // Extensions and service numbers are shared by whole offices.
    for bad in ["", "123", "1900", "1900555", "ext. 42"] {
        assert!(phone_key(bad).is_none(), "{:?}", bad);
    }
}

// ─── Finding ────────────────────────────────────────────────

#[test]
fn an_import_run_twice_adds_nobody_the_second_time() {
    // The acceptance test for the whole module.
    let vault_props = json!({ "details": [{ "label": "Email", "value": "an@example.com", "type": "email" }] });
    let existing = vec![Existing {
        id: "People/an.md",
        title: "An Nguyễn",
        properties: &vault_props,
    }];

    let incoming = vec![with_email("An Nguyễn", "AN@EXAMPLE.COM")];
    let found = find_duplicates(&incoming, &existing);

    assert_eq!(found.len(), 1);
    assert_eq!(found[0].incoming, 0);
    assert_eq!(found[0].existing_id.as_deref(), Some("People/an.md"));
    assert_eq!(found[0].reason, Reason::Email("an@example.com".into()));
    assert!(found[0].reason.is_certain());
}

#[test]
fn a_number_matches_even_when_the_name_changed() {
    let vault_props = json!({ "phone": "0901234567" });
    let existing = vec![Existing {
        id: "People/an.md",
        title: "An",
        properties: &vault_props,
    }];
    let incoming = vec![with_phone("An Nguyễn (work)", "+84 90 123 4567")];

    let found = find_duplicates(&incoming, &existing);
    assert_eq!(found.len(), 1);
    assert!(matches!(found[0].reason, Reason::Phone(_)));
}

#[test]
fn a_file_that_lists_somebody_twice_creates_them_once() {
    let incoming = vec![
        with_email("An Nguyễn", "an@example.com"),
        with_email("Bình Trần", "binh@example.com"),
        with_email("An N.", "AN@example.com"),
    ];

    let found = find_duplicates(&incoming, &[]);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].incoming, 2);
    // Matched against an earlier row of the same file, not against the vault.
    assert_eq!(found[0].existing_id, None);
    assert_eq!(found[0].existing_incoming, Some(0));
}

#[test]
fn the_same_name_is_a_question_not_an_answer() {
    // There are a great many people called Nguyễn Văn An.
    let vault_props = json!({ "email": "an.one@example.com" });
    let existing = vec![Existing {
        id: "People/an.md",
        title: "Nguyễn Văn An",
        properties: &vault_props,
    }];
    let incoming = vec![with_email("nguyễn văn an", "an.two@example.com")];

    let found = find_duplicates(&incoming, &existing);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].reason, Reason::Name("nguyễn văn an".into()));
    assert!(!found[0].reason.is_certain(), "somebody has to decide this one");
}

#[test]
fn an_address_in_common_is_reported_instead_of_the_name() {
    // Asking about the name as well would be noise on a certain match.
    let vault_props = json!({ "email": "an@example.com" });
    let existing = vec![Existing {
        id: "People/an.md",
        title: "An Nguyễn",
        properties: &vault_props,
    }];
    let incoming = vec![with_email("An Nguyễn", "an@example.com")];

    let found = find_duplicates(&incoming, &existing);
    assert_eq!(found.len(), 1);
    assert!(found[0].reason.is_certain());
}

#[test]
fn somebody_genuinely_new_is_not_reported() {
    let vault_props = json!({ "email": "an@example.com" });
    let existing = vec![Existing {
        id: "People/an.md",
        title: "An Nguyễn",
        properties: &vault_props,
    }];
    let incoming = vec![with_email("Cường Phạm", "cuong@example.com")];
    assert!(find_duplicates(&incoming, &existing).is_empty());
}

#[test]
fn a_contact_with_nothing_to_match_on_is_left_alone() {
    let incoming = vec![contact("", json!({}))];
    assert!(find_duplicates(&incoming, &[]).is_empty());
}

// ─── Merging ────────────────────────────────────────────────

#[test]
fn what_somebody_typed_is_not_overwritten_by_an_export() {
    let existing = json!({
        "nickname": "Ann",
        "birthday": "1994-03-02",
        "relationship_type": "Friend",
    });
    let incoming = json!({ "nickname": "A.", "birthday": "1994-03-03" })
        .as_object()
        .cloned()
        .unwrap();

    let patch = merge(&existing, &incoming);
    // Named nowhere in the patch, so the write leaves them as they are.
    assert!(!patch.contains_key("nickname"), "{:?}", patch);
    assert!(!patch.contains_key("birthday"), "{:?}", patch);
}

#[test]
fn a_gap_in_the_vault_is_filled_from_the_import() {
    let existing = json!({ "nickname": "" });
    let incoming = json!({ "nickname": "Ann", "birthday": "1994-03-02" })
        .as_object()
        .cloned()
        .unwrap();

    let patch = merge(&existing, &incoming);
    assert_eq!(patch["nickname"], json!("Ann"));
    assert_eq!(patch["birthday"], json!("1994-03-02"));
}

#[test]
fn a_new_detail_is_added_and_a_known_one_is_not_repeated() {
    let existing = json!({ "details": [
        { "label": "Email", "value": "an@example.com", "type": "email" },
    ]});
    let incoming = json!({ "details": [
        // The same address, spelled differently — not a second address.
        { "label": "Work Email", "value": "AN@Example.com", "type": "email" },
        { "label": "Mobile Phone", "value": "+84 90 123 4567", "type": "phone" },
    ]})
    .as_object()
    .cloned()
    .unwrap();

    let patch = merge(&existing, &incoming);
    let details = patch["details"].as_array().unwrap();
    assert_eq!(details.len(), 2, "{:#?}", details);
    assert_eq!(details[0]["value"], json!("an@example.com"));
    assert_eq!(details[1]["value"], json!("+84 90 123 4567"));
    // The flat copy the sidebar reads follows the details.
    assert_eq!(patch["phone"], json!("+84 90 123 4567"));
}

#[test]
fn the_same_number_written_differently_is_not_added_twice() {
    let existing = json!({ "details": [
        { "label": "Mobile Phone", "value": "0901234567", "type": "phone" },
    ]});
    let incoming = json!({ "details": [
        { "label": "Mobile Phone", "value": "+84 90 123 4567", "type": "phone" },
    ]})
    .as_object()
    .cloned()
    .unwrap();

    assert!(!merge(&existing, &incoming).contains_key("details"));
}

#[test]
fn tags_and_key_dates_are_pooled() {
    let existing = json!({
        "tags": ["work"],
        "important_dates": [{ "label": "Anniversary", "date": "2019-06-01" }],
    });
    let incoming = json!({
        "tags": ["work", "vietnam"],
        "important_dates": [
            { "label": "Anniversary", "date": "2019-06-01" },
            { "label": "First met", "date": "2018-11-20" },
        ],
    })
    .as_object()
    .cloned()
    .unwrap();

    let patch = merge(&existing, &incoming);
    assert_eq!(patch["tags"], json!(["work", "vietnam"]));
    assert_eq!(patch["important_dates"].as_array().unwrap().len(), 2);
}

#[test]
fn a_job_history_is_not_overwritten_by_a_single_snapshot() {
    // A card knows where somebody works now. The vault may know where they
    // worked for the last ten years, and that is worth more.
    let existing = json!({ "experiences": [
        { "company": "Acme", "role": "Engineer", "current": true },
        { "company": "Globex", "role": "Intern", "current": false },
    ]});
    let incoming = json!({ "experiences": [{ "company": "Acme Corp", "role": "Staff", "current": true }] })
        .as_object()
        .cloned()
        .unwrap();

    assert!(!merge(&existing, &incoming).contains_key("experiences"));
}

#[test]
fn a_person_with_no_job_on_file_takes_the_one_from_the_card() {
    let existing = json!({});
    let incoming = json!({ "experiences": [{ "company": "Acme", "role": "Staff", "current": true }] })
        .as_object()
        .cloned()
        .unwrap();

    assert_eq!(
        merge(&existing, &incoming)["experiences"][0]["company"],
        json!("Acme")
    );
}

#[test]
fn merging_nothing_new_changes_nothing() {
    // Importing the same file twice: the second run has nothing to say, and a
    // patch with no keys leaves the file untouched.
    let existing = json!({
        "nickname": "Ann",
        "tags": ["work"],
        "details": [{ "label": "Email", "value": "an@example.com", "type": "email" }],
    });
    let incoming = existing.as_object().cloned().unwrap();
    assert_eq!(merge(&existing, &incoming), Map::new());
}
