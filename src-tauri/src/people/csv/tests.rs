use super::*;

fn contacts_of(text: &str) -> Vec<ImportedContact> {
    let table = parse(text);
    let columns = detect(&table.headers);
    to_contacts(&table, &columns)
}

fn details(contact: &ImportedContact) -> Vec<(String, String)> {
    contact
        .properties
        .get("details")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .map(|d| {
                    (
                        d["label"].as_str().unwrap_or("").to_string(),
                        d["value"].as_str().unwrap_or("").to_string(),
                    )
                })
                .collect()
        })
        .unwrap_or_default()
}

fn prop_str<'a>(contact: &'a ImportedContact, key: &str) -> &'a str {
    contact.properties.get(key).and_then(Value::as_str).unwrap_or("")
}

// ─── The file format ────────────────────────────────────────

#[test]
fn a_quoted_field_may_hold_the_separator_itself() {
    let table = parse("name,note\n\"Nguyễn, An\",\"said \"\"hello\"\", then left\"\n");
    assert_eq!(table.headers, ["name", "note"]);
    assert_eq!(
        table.rows,
        [["Nguyễn, An", "said \"hello\", then left"]]
    );
}

#[test]
fn a_quoted_field_may_run_over_several_lines() {
    let table = parse("name,note\nAn,\"first line\nsecond line\"\n");
    assert_eq!(table.rows, [["An", "first line\nsecond line"]]);
}

#[test]
fn line_endings_are_read_whichever_way_they_were_written() {
    for text in [
        "name,email\nAn,an@example.com\n",
        "name,email\r\nAn,an@example.com\r\n",
        "name,email\rAn,an@example.com\r",
    ] {
        let table = parse(text);
        assert_eq!(table.rows, [["An", "an@example.com"]], "{:?}", text);
    }
}

#[test]
fn a_spreadsheet_saved_by_excel_still_matches_its_own_headers() {
    // Excel opens the file with a byte-order mark, which otherwise becomes
    // part of the first header's name and stops every rule from matching it.
    let table = parse("\u{feff}Name,Email\nAn,an@example.com\n");
    assert_eq!(table.headers[0], "Name");
    assert_eq!(detect(&table.headers)[0].field, Some(Field::FullName));
}

#[test]
fn blank_lines_are_not_people() {
    let table = parse("name,email\nAn,an@example.com\n\n\n,,\n");
    assert_eq!(table.rows.len(), 1);
}

#[test]
fn an_empty_file_is_an_empty_table() {
    assert_eq!(parse(""), Table::default());
    assert_eq!(parse("\n\n"), Table::default());
}

// ─── Google Contacts ────────────────────────────────────────

const GOOGLE: &str = "First Name,Middle Name,Last Name,Nickname,Birthday,Notes,Labels,\
Organization Name,Organization Title,\
E-mail 1 - Label,E-mail 1 - Value,E-mail 2 - Label,E-mail 2 - Value,\
Phone 1 - Label,Phone 1 - Value,Phone 2 - Label,Phone 2 - Value\n\
An,Văn,Nguyễn,Ann,1994-03-02,Met at the meetup,* myContacts ::: Friends ::: Work,\
Acme Corp,Staff Engineer,\
Work,an@acme.example,Home,an@personal.example,\
Mobile,+84 90 123 4567,Work,+84 24 3333 4444\n";

#[test]
fn a_google_export_needs_no_mapping_at_all() {
    let contacts = contacts_of(GOOGLE);
    assert_eq!(contacts.len(), 1);
    let contact = &contacts[0];

    assert_eq!(contact.title, "An Văn Nguyễn");
    assert_eq!(prop_str(contact, "nickname"), "Ann");
    assert_eq!(prop_str(contact, "birthday"), "1994-03-02");
    assert_eq!(contact.body, "Met at the meetup");
    assert_eq!(prop_str(contact, "company"), "Acme Corp");
    assert_eq!(
        contact.properties["experiences"][0]["role"],
        json!("Staff Engineer")
    );
}

#[test]
fn google_labels_become_tags_without_its_bookkeeping() {
    // Every contact is in `myContacts`; it says nothing about anybody.
    let contact = &contacts_of(GOOGLE)[0];
    assert_eq!(contact.properties["tags"], json!(["friends", "work"]));
}

#[test]
fn the_label_beside_a_value_is_the_label_it_keeps() {
    // Google writes the label in its own column. Reading it from the header
    // instead would give four details all called "Email" and "Phone".
    let contact = &contacts_of(GOOGLE)[0];
    assert_eq!(
        details(contact),
        [
            ("Work Email".to_string(), "an@acme.example".to_string()),
            ("Home Email".to_string(), "an@personal.example".to_string()),
            ("Mobile Phone".to_string(), "+84 90 123 4567".to_string()),
            ("Work Phone".to_string(), "+84 24 3333 4444".to_string()),
        ]
    );
}

#[test]
fn the_flat_fields_the_sidebar_reads_are_filled_in() {
    let contact = &contacts_of(GOOGLE)[0];
    assert_eq!(prop_str(contact, "email"), "an@acme.example");
    assert_eq!(prop_str(contact, "phone"), "+84 90 123 4567");
}

#[test]
fn a_column_that_is_empty_for_this_row_adds_nothing() {
    let text = "First Name,Last Name,E-mail 1 - Label,E-mail 1 - Value,E-mail 2 - Label,E-mail 2 - Value\n\
                An,Nguyễn,Work,an@acme.example,,\n";
    assert_eq!(
        details(&contacts_of(text)[0]),
        [("Work Email".to_string(), "an@acme.example".to_string())]
    );
}

// ─── Outlook ────────────────────────────────────────────────

const OUTLOOK: &str = "First Name,Middle Name,Last Name,Company,Job Title,\
E-mail Address,Home Phone,Business Phone,Mobile Phone,Birthday,Notes,Categories,Web Page\n\
Bình,,Trần,Globex,Designer,\
binh@globex.example,+84 24 1111 2222,+84 24 3333 4444,+84 90 555 6666,3/2/1990,Design lead,Vendors,https://binh.example\n";

#[test]
fn an_outlook_export_needs_no_mapping_either() {
    let contact = &contacts_of(OUTLOOK)[0];
    assert_eq!(contact.title, "Bình Trần");
    assert_eq!(prop_str(contact, "company"), "Globex");
    assert_eq!(contact.properties["experiences"][0]["role"], json!("Designer"));
    assert_eq!(contact.properties["tags"], json!(["vendors"]));
    assert_eq!(contact.body, "Design lead");
}

#[test]
fn outlooks_own_column_names_carry_their_own_labels() {
    // No paired label column here; the header itself says which is which.
    let got = details(&contacts_of(OUTLOOK)[0]);
    assert!(got.contains(&("Email".into(), "binh@globex.example".into())), "{:?}", got);
    assert!(got.contains(&("Phone".into(), "+84 24 1111 2222".into())), "{:?}", got);
    assert!(got.contains(&("Website".into(), "https://binh.example".into())), "{:?}", got);
}

#[test]
fn a_date_is_read_whichever_way_round_it_was_written() {
    let date = |raw: &str| {
        let text = format!("Name,Birthday\nAn,{}\n", raw);
        contacts_of(&text)[0]
            .properties
            .get("birthday")
            .and_then(Value::as_str)
            .map(str::to_string)
    };
    assert_eq!(date("1994-03-02").as_deref(), Some("1994-03-02"));
    assert_eq!(date("3/2/1994").as_deref(), Some("1994-03-02"));
    assert_eq!(date("1994/03/02").as_deref(), Some("1994-03-02"));
    // Google's "day and month, year unknown".
    assert_eq!(date("--03-02").as_deref(), Some("03-02"));
    // Ambiguous or unreadable is left unset rather than guessed at: a wrong
    // birthday is worse than no birthday, and it would go off every year.
    assert_eq!(date("March 2nd"), None);
    assert_eq!(date("1994"), None);
}

// ─── Anything else ──────────────────────────────────────────

#[test]
fn an_unknown_column_is_left_for_the_user_to_map() {
    // Guessing here would be silent and would land in the vault looking
    // deliberate. The caller shows these and asks.
    let table = parse("Name,Loyalty Tier,Preferred Pronoun\nAn,Gold,she\n");
    let columns = detect(&table.headers);
    assert_eq!(columns[0].field, Some(Field::FullName));
    assert_eq!(columns[1].field, None);
    assert_eq!(columns[2].field, None);
}

#[test]
fn a_column_the_user_maps_by_hand_is_honoured() {
    let table = parse("Name,Loyalty Tier\nAn,Gold\n");
    let mut columns = detect(&table.headers);
    columns[1].field = Some(Field::Text("Loyalty Tier".into()));

    let contacts = to_contacts(&table, &columns);
    assert_eq!(details(&contacts[0]), [("Loyalty Tier".to_string(), "Gold".to_string())]);
}

#[test]
fn a_row_with_no_name_is_not_a_person() {
    let text = "First Name,Last Name,E-mail Address\n,,orphan@example.com\nAn,Nguyễn,an@example.com\n";
    let contacts = contacts_of(text);
    assert_eq!(contacts.len(), 1);
    assert_eq!(contacts[0].title, "An Nguyễn");
}

#[test]
fn a_short_row_does_not_take_the_whole_import_down() {
    // Hand-edited files have ragged rows.
    let text = "First Name,Last Name,E-mail Address\nAn,Nguyễn\nBình,Trần,binh@example.com\n";
    let names: Vec<String> = contacts_of(text).into_iter().map(|c| c.title).collect();
    assert_eq!(names, ["An Nguyễn", "Bình Trần"]);
}

#[test]
fn several_note_columns_are_kept_one_after_another() {
    let text = "Name,Notes,Comments\nAn,first note,second note\n";
    assert_eq!(contacts_of(text)[0].body, "first note\n\nsecond note");
}

#[test]
fn a_full_name_column_beats_the_parts() {
    // Google's older export has both, and the parts are sometimes blank.
    let text = "Name,Given Name,Family Name\nAn Văn Nguyễn,An,Nguyễn\n";
    assert_eq!(contacts_of(text)[0].title, "An Văn Nguyễn");
}

#[test]
fn a_thousand_rows_is_not_a_thousand_special_cases() {
    let mut text = String::from("First Name,Last Name,E-mail Address\n");
    for i in 0..1000 {
        text.push_str(&format!("Person{},Nguyễn,p{}@example.com\n", i, i));
    }
    let contacts = contacts_of(&text);
    assert_eq!(contacts.len(), 1000);
    assert_eq!(contacts[999].title, "Person999 Nguyễn");
    assert_eq!(prop_str(&contacts[999], "email"), "p999@example.com");
}

#[test]
fn two_thousand_rows_are_read_in_well_under_a_second() {
    // Same gate as the vCard side, on the shape Google exports.
    let mut text = String::from(
        "First Name,Last Name,Birthday,Notes,Labels,Organization Name,Organization Title,\
         E-mail 1 - Label,E-mail 1 - Value,Phone 1 - Label,Phone 1 - Value\n",
    );
    for i in 0..2000 {
        text.push_str(&format!(
            "Person{i},Nguyễn,1994-03-02,\"Met at a meetup, in Hà Nội\",* myContacts ::: Work,\
             Company {i},Engineer,Work,person{i}@example.com,Mobile,+84 90 {i:04} 000\n"
        ));
    }

    let started = std::time::Instant::now();
    let contacts = contacts_of(&text);
    let elapsed = started.elapsed();

    assert_eq!(contacts.len(), 2000);
    assert_eq!(contacts[1999].title, "Person1999 Nguyễn");
    println!("parsed 2000 CSV rows in {:?}", elapsed);
    assert!(elapsed.as_secs() < 2, "took {:?}", elapsed);
}
