use super::*;
use crate::models::node::NodeMetadata;
use serde_json::json;

fn person(id: &str, title: &str, properties: Value, body: &str) -> NodeMetadata {
    NodeMetadata {
        id: id.to_string(),
        node_type: "person".into(),
        title: title.to_string(),
        content: body.to_string(),
        properties,
        created_at: "2026-01-01 00:00:00".into(),
        updated_at: "2026-01-01 00:00:00".into(),
        timestamp: 0,
        blocks: None,
    }
}

#[test]
fn a_file_is_read_by_what_is_in_it_not_what_it_is_called() {
    // Address books arrive as `contacts.txt`, and files downloaded from a
    // phone are routinely `.csv` holding vCards.
    assert!(looks_like_vcard("BEGIN:VCARD\r\nVERSION:3.0\r\nFN:An\r\nEND:VCARD\r\n"));
    assert!(looks_like_vcard("\u{feff}begin:vcard\r\n"));
    assert!(looks_like_vcard("\n\n  BEGIN:VCARD\r\n"));
    assert!(!looks_like_vcard("Name,Email\nAn,an@example.com\n"));
    assert!(!looks_like_vcard(""));
}

// ─── Writing a spreadsheet ──────────────────────────────────

#[test]
fn an_exported_spreadsheet_reads_back_as_the_same_people() {
    // The acceptance test for the CSV half: out and back in, unchanged.
    let people = vec![
        person(
            "People/an.md",
            "An Nguyễn",
            json!({
                "nickname": "Ann",
                "birthday": "1994-03-02",
                "tags": ["work", "vietnam"],
                "experiences": [{ "company": "Acme Corp", "role": "Staff Engineer", "current": true }],
                "details": [
                    { "label": "Work Email", "value": "an@acme.example", "type": "email" },
                    { "label": "Mobile Phone", "value": "+84 90 123 4567", "type": "phone" },
                ],
            }),
            "Met at the meetup",
        ),
        person("People/binh.md", "Bình Trần", json!({}), ""),
    ];

    let text = write_csv(&people);
    let table = crate::people::csv::parse(&text);
    let columns = crate::people::csv::detect(&table.headers);
    let back = crate::people::csv::to_contacts(&table, &columns);

    assert_eq!(back.len(), 2);
    assert_eq!(back[0].title, "An Nguyễn");
    assert_eq!(back[0].properties["nickname"], json!("Ann"));
    assert_eq!(back[0].properties["birthday"], json!("1994-03-02"));
    assert_eq!(back[0].properties["tags"], json!(["work", "vietnam"]));
    assert_eq!(back[0].properties["company"], json!("Acme Corp"));
    assert_eq!(back[0].body, "Met at the meetup");
    assert_eq!(back[0].properties["email"], json!("an@acme.example"));
    assert_eq!(back[0].properties["phone"], json!("+84 90 123 4567"));
    assert_eq!(back[1].title, "Bình Trần");
}

#[test]
fn punctuation_in_a_field_does_not_become_a_column() {
    let people = vec![person(
        "People/an.md",
        "Nguyễn, An",
        json!({}),
        "She said \"hello\",\nthen left",
    )];

    let text = write_csv(&people);
    let table = crate::people::csv::parse(&text);
    let row = &table.rows[0];
    assert_eq!(row[0], "Nguyễn, An");
    assert_eq!(row[table.headers.len() - 1], "She said \"hello\",\nthen left");
    // Every row still has exactly as many fields as there are headers.
    assert!(
        table.rows.iter().all(|r| r.len() == table.headers.len()),
        "{:?}",
        table.rows
    );
}

#[test]
fn the_header_row_is_one_this_app_can_read_back_unaided() {
    let text = write_csv(&[]);
    let table = crate::people::csv::parse(&text);
    let columns = crate::people::csv::detect(&table.headers);
    let unmapped: Vec<&String> = table
        .headers
        .iter()
        .zip(&columns)
        .filter(|(h, c)| c.field.is_none() && !crate::people::csv::is_label_column(h))
        .map(|(h, _)| h)
        .collect();
    assert!(unmapped.is_empty(), "would need mapping: {:?}", unmapped);
}

// ─── Scale ──────────────────────────────────────────────────

/// What two thousand contacts cost to write, on the path they really take.
///
/// Parsing is measured in the format modules and is around thirty
/// milliseconds for this many; it was never going to be the slow part. The
/// slow part is that every contact is written the same way one typed in by
/// hand is — file, identity, CRDT, index — which is what makes an imported
/// person searchable, syncable and linkable rather than a row in a table.
///
/// This measures that work directly rather than through `write_node_file`,
/// which takes a `tauri::AppHandle` a mock runtime cannot satisfy. It leaves
/// out the command's own frontmatter round-trip and its reads, so the real
/// figure is somewhat higher than this one.
#[test]
fn writing_two_thousand_contacts_stays_inside_the_budget() {
    use crate::db::DbBridge;
    use tauri::Manager;

    let holder = tempfile::tempdir().expect("tempdir");
    let vault = holder.path().join("vault");
    std::fs::create_dir_all(vault.join("People")).expect("vault dir");
    let vault_path = vault.to_string_lossy().to_string();

    let app = tauri::test::mock_builder()
        .build(tauri::test::mock_context(tauri::test::noop_assets()))
        .expect("mock app");
    let handle = app.handle().clone();
    handle.manage(crate::db::DbState::new(
        DbBridge::new_in_memory_full().expect("schema"),
    ));
    let state = handle.state::<crate::db::DbState>();

    let identity =
        crate::sync::core::identity::load_or_register_vault_identity(&handle, &vault_path)
            .expect("vault identity");
    let vault_id = identity.vault_id.to_string();

    // The shape a Google export produces, so the frontmatter is realistic.
    let properties = |i: usize| -> Value {
        json!({
            "display_name": "fullname",
            "birthday": "1994-03-02",
            "tags": ["work", "imported"],
            "email": format!("person{i}@example.com"),
            "phone": format!("+84 90 {i:04} 000"),
            "details": [
                { "label": "Work Email", "value": format!("person{i}@example.com"), "type": "email" },
                { "label": "Mobile Phone", "value": format!("+84 90 {i:04} 000"), "type": "phone" },
                { "label": "Home Address", "value": format!("{i} Phố Huế, Hà Nội"), "type": "text" },
            ],
            "experiences": [{ "company": format!("Company {i}"), "role": "Engineer", "current": true }],
        })
    };

    let started = std::time::Instant::now();
    for i in 0..2000 {
        let rel_path = format!("People/person-{i}.md");
        let abs_path = vault.join(&rel_path);
        let title = format!("Person{i} Nguyễn");
        let text = crate::commands::nodes::markdown_with_frontmatter(
            &title,
            "person",
            &properties(i),
            "Imported from a phone export.",
        );

        std::fs::write(&abs_path, &text).expect("write person");
        let node_id = crate::sync::core::identity::get_or_assign_node_id(&vault, &abs_path)
            .expect("node id");

        let db = state.lock().unwrap_or_else(|e| e.into_inner());
        db.upsert_document_path(&vault_id, &node_id, &rel_path)
            .expect("document path");
        let written = std::fs::read_to_string(&abs_path).expect("read back");
        crate::commands::nodes::crdt_apply_safe(&db, &vault_id, &node_id, &written)
            .expect("crdt");
        let node = crate::utils::node_parser::parse_file_to_node(&vault_path, &abs_path)
            .expect("parse");
        db.upsert_node(&node).expect("index");
    }
    let elapsed = started.elapsed();

    let count = {
        let db = state.lock().unwrap_or_else(|e| e.into_inner());
        db.get_nodes_by_type("person").expect("read back").len()
    };
    assert_eq!(count, 2000);

    println!("wrote 2000 contacts in {:?}", elapsed);
    // The gate in the roadmap. Generous against a loaded machine, but it
    // fails loudly if this path ever becomes ten times slower.
    assert!(elapsed.as_secs() < 10, "took {:?}", elapsed);
}
