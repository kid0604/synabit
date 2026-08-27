use super::*;

/// The first contact in a file, or a failure naming what came back instead.
fn one(text: &str) -> ImportedContact {
    let contacts = import(text);
    assert_eq!(contacts.len(), 1, "expected one card, got {:?}", contacts);
    contacts.into_iter().next().unwrap()
}

/// The details on a contact, as `(label, value)` pairs.
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

// ─── Reading what the world actually sends ──────────────────

#[test]
fn a_card_from_apple_contacts() {
    // vCard 3.0, which is what macOS and iOS export.
    let contact = one(
        "BEGIN:VCARD\r\n\
         VERSION:3.0\r\n\
         N:Nguyễn;An;;;\r\n\
         FN:An Nguyễn\r\n\
         ORG:Acme Corp;Engineering\r\n\
         TITLE:Staff Engineer\r\n\
         EMAIL;type=INTERNET;type=WORK:an@acme.example\r\n\
         TEL;type=CELL;type=VOICE:+84 90 123 4567\r\n\
         BDAY:1994-03-02\r\n\
         NOTE:Met at the Hanoi meetup.\r\n\
         CATEGORIES:work,vietnam\r\n\
         END:VCARD\r\n",
    );

    assert_eq!(contact.title, "An Nguyễn");
    assert_eq!(prop_str(&contact, "birthday"), "1994-03-02");
    assert_eq!(contact.body, "Met at the Hanoi meetup.");
    assert_eq!(contact.properties["tags"], json!(["work", "vietnam"]));
    assert_eq!(
        details(&contact),
        [
            ("Work Email".into(), "an@acme.example".into()),
            ("Mobile Phone".into(), "+84 90 123 4567".into()),
        ]
    );
    assert_eq!(
        contact.properties["experiences"],
        json!([{
            "company": "Acme Corp — Engineering",
            "role": "Staff Engineer",
            "start": "", "end": "", "current": true,
        }])
    );
}

#[test]
fn a_card_from_an_older_phone() {
    // vCard 2.1: bare type parameters, and quoted-printable for anything
    // that is not ASCII — which for this address book is the name itself.
    let contact = one(
        "BEGIN:VCARD\r\n\
         VERSION:2.1\r\n\
         N;CHARSET=UTF-8;ENCODING=QUOTED-PRINTABLE:=4C=C3=AA;=41=6E=68\r\n\
         FN;CHARSET=UTF-8;ENCODING=QUOTED-PRINTABLE:=4C=C3=AA =41=6E=68\r\n\
         TEL;HOME;VOICE:+84 24 3333 4444\r\n\
         TEL;CELL:+84 90 555 6666\r\n\
         EMAIL;INTERNET:anh@example.com\r\n\
         END:VCARD\r\n",
    );

    assert_eq!(contact.title, "Lê Anh");
    assert_eq!(
        details(&contact),
        [
            ("Home Phone".into(), "+84 24 3333 4444".into()),
            ("Mobile Phone".into(), "+84 90 555 6666".into()),
            ("Email".into(), "anh@example.com".into()),
        ]
    );
}

#[test]
fn a_quoted_printable_value_continued_across_lines() {
    // 2.1's own continuation rule: a trailing `=` and no leading space on the
    // next line. The ordinary folding rule does not see it, and a card that
    // uses it came apart into a truncated name and a stray line.
    let contact = one(
        "BEGIN:VCARD\r\n\
         VERSION:2.1\r\n\
         FN;ENCODING=QUOTED-PRINTABLE:=54=72=E1=BA=A7=6E=20=\r\n\
         =56=C4=83=6E=20=42=\r\n\
         =C3=ACnh\r\n\
         END:VCARD\r\n",
    );
    assert_eq!(contact.title, "Trần Văn Bình");
}

#[test]
fn a_card_with_only_a_structured_name_is_still_a_person() {
    // Older exporters write `N` and no `FN`. Refusing these drops contacts.
    let contact = one("BEGIN:VCARD\r\nVERSION:2.1\r\nN:Trần;Bình;Văn;Mr.;PhD\r\nEND:VCARD\r\n");
    assert_eq!(contact.title, "Mr. Bình Văn Trần PhD");
}

#[test]
fn a_card_with_no_name_at_all_is_not_imported() {
    // There is nothing to call this person and nothing to show in a list.
    assert!(import("BEGIN:VCARD\r\nVERSION:3.0\r\nTEL:+84 90 000 0000\r\nEND:VCARD\r\n").is_empty());
}

#[test]
fn several_cards_in_one_file() {
    let text = "BEGIN:VCARD\r\nVERSION:3.0\r\nFN:An\r\nEND:VCARD\r\n\
                BEGIN:VCARD\r\nVERSION:3.0\r\nFN:Bình\r\nEND:VCARD\r\n\
                BEGIN:VCARD\r\nVERSION:3.0\r\nFN:Cường\r\nEND:VCARD\r\n";
    let names: Vec<String> = import(text).into_iter().map(|c| c.title).collect();
    assert_eq!(names, ["An", "Bình", "Cường"]);
}

#[test]
fn a_file_that_stops_halfway_keeps_what_it_had() {
    // Downloads get truncated. The cards before the cut are still good.
    let text = "BEGIN:VCARD\r\nVERSION:3.0\r\nFN:An\r\nEND:VCARD\r\n\
                BEGIN:VCARD\r\nVERSION:3.0\r\nFN:Bình\r\nTEL:+84 90";
    let names: Vec<String> = import(text).into_iter().map(|c| c.title).collect();
    assert_eq!(names, ["An", "Bình"]);
}

#[test]
fn rubbish_around_the_cards_is_skipped_rather_than_refused() {
    let text = "Content-Type: text/vcard\r\n\r\n\
                BEGIN:VCARD\r\nVERSION:3.0\r\nFN:An\r\nEND:VCARD\r\n\
                -- \r\nsent from a phone\r\n";
    assert_eq!(import(text).len(), 1);
}

// ─── Nothing is lost ────────────────────────────────────────

#[test]
fn a_property_nobody_here_knows_becomes_a_labelled_detail() {
    // Every vendor invents its own. Dropping them is how an import quietly
    // loses the one field somebody cared about.
    let contact = one(
        "BEGIN:VCARD\r\nVERSION:3.0\r\nFN:An\r\n\
         X-SKYPE:an.nguyen\r\n\
         X-ABDATE;type=pref:2019-06-01\r\n\
         GENDER:F\r\n\
         END:VCARD\r\n",
    );
    let got = details(&contact);
    assert!(got.contains(&("Skype".into(), "an.nguyen".into())), "{:?}", got);
    assert!(got.contains(&("Abdate".into(), "2019-06-01".into())), "{:?}", got);
    assert!(got.contains(&("Gender".into(), "F".into())), "{:?}", got);
}

#[test]
fn an_escaped_separator_stays_inside_the_value_it_belongs_to() {
    let contact = one(
        "BEGIN:VCARD\r\nVERSION:3.0\r\nFN:An\r\n\
         ORG:Acme\\, Ltd;Engineering\r\n\
         CATEGORIES:vip\\, key account,work\r\n\
         END:VCARD\r\n",
    );
    assert_eq!(prop_str(&contact, "company"), "Acme, Ltd");
    assert_eq!(contact.properties["tags"], json!(["vip, key account", "work"]));
}

#[test]
fn an_address_arrives_as_one_readable_line() {
    let contact = one(
        "BEGIN:VCARD\r\nVERSION:3.0\r\nFN:An\r\n\
         ADR;TYPE=home:;;12 Phố Huế;Hà Nội;;100000;Việt Nam\r\n\
         END:VCARD\r\n",
    );
    assert_eq!(
        details(&contact),
        [("Home Address".into(), "12 Phố Huế, Hà Nội, 100000, Việt Nam".into())]
    );
}

#[test]
fn a_link_is_named_by_where_it_points() {
    let contact = one(
        "BEGIN:VCARD\r\nVERSION:4.0\r\nFN:An\r\n\
         URL:https://www.linkedin.com/in/an\r\n\
         URL:https://github.com/an\r\n\
         URL:https://an.example\r\n\
         END:VCARD\r\n",
    );
    assert_eq!(
        details(&contact),
        [
            ("LinkedIn".into(), "https://www.linkedin.com/in/an".into()),
            ("GitHub".into(), "https://github.com/an".into()),
            ("Website".into(), "https://an.example".into()),
        ]
    );
}

#[test]
fn the_flat_fields_the_sidebar_reads_are_filled_in() {
    // A number labelled "Mobile Phone" has to satisfy a lookup for "phone",
    // or an imported contact shows no number beside their name.
    let contact = one(
        "BEGIN:VCARD\r\nVERSION:3.0\r\nFN:An\r\n\
         TEL;TYPE=CELL:+84 90 123 4567\r\n\
         EMAIL;TYPE=WORK:an@acme.example\r\n\
         ORG:Acme Corp\r\n\
         END:VCARD\r\n",
    );
    assert_eq!(prop_str(&contact, "phone"), "+84 90 123 4567");
    assert_eq!(prop_str(&contact, "email"), "an@acme.example");
    assert_eq!(prop_str(&contact, "company"), "Acme Corp");
}

// ─── Dates ──────────────────────────────────────────────────

#[test]
fn a_birthday_is_read_in_every_shape_it_is_written() {
    let bday = |raw: &str| {
        let text = format!("BEGIN:VCARD\r\nVERSION:4.0\r\nFN:An\r\nBDAY:{}\r\nEND:VCARD\r\n", raw);
        one(&text).properties.get("birthday").and_then(Value::as_str).map(str::to_string)
    };
    assert_eq!(bday("19940302").as_deref(), Some("1994-03-02"));
    assert_eq!(bday("1994-03-02").as_deref(), Some("1994-03-02"));
    assert_eq!(bday("1994-03-02T00:00:00Z").as_deref(), Some("1994-03-02"));
    // vCard 4's "this day, year unknown" — the shape the reminder engine
    // reads as MM-DD.
    assert_eq!(bday("--0302").as_deref(), Some("03-02"));
    // A year alone names no day, so there is nothing to announce.
    assert_eq!(bday("1994"), None);
    assert_eq!(bday("sometime"), None);
}

#[test]
fn an_anniversary_becomes_a_key_date() {
    let contact = one(
        "BEGIN:VCARD\r\nVERSION:4.0\r\nFN:An\r\nANNIVERSARY:20190601\r\nEND:VCARD\r\n",
    );
    assert_eq!(
        contact.properties["important_dates"],
        json!([{ "label": "Anniversary", "date": "2019-06-01" }])
    );
}

// ─── Photos ─────────────────────────────────────────────────

/// The smallest valid PNG, so the sniffing has real bytes to work on.
const PNG: &[u8] = &[
    0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F, 0x15, 0xC4,
    0x89, 0x00, 0x00, 0x00, 0x0A, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0x00, 0x01, 0x00, 0x00,
    0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE,
    0x42, 0x60, 0x82,
];

fn encoded_png() -> String {
    base64::engine::general_purpose::STANDARD.encode(PNG)
}

#[test]
fn a_photo_arrives_however_the_version_wrote_it() {
    let inline_3 = format!(
        "BEGIN:VCARD\r\nVERSION:3.0\r\nFN:An\r\nPHOTO;ENCODING=b;TYPE=JPEG:{}\r\nEND:VCARD\r\n",
        encoded_png()
    );
    let uri_4 = format!(
        "BEGIN:VCARD\r\nVERSION:4.0\r\nFN:An\r\nPHOTO:data:image/png;base64,{}\r\nEND:VCARD\r\n",
        encoded_png()
    );

    for text in [inline_3, uri_4] {
        let photo = one(&text).photo.expect("a photo");
        assert_eq!(photo.bytes, PNG);
        // The bytes decide the format, not the TYPE parameter — plenty of
        // exporters label every photo JPEG whatever it is.
        assert_eq!(photo.extension, "png");
    }
}

#[test]
fn a_photo_that_is_only_a_link_is_left_alone() {
    // Following it would make importing a file reach out to the network.
    let contact = one(
        "BEGIN:VCARD\r\nVERSION:4.0\r\nFN:An\r\nPHOTO:https://example.com/an.jpg\r\nEND:VCARD\r\n",
    );
    assert!(contact.photo.is_none());
}

#[test]
fn something_that_is_not_an_image_is_not_taken_as_one() {
    let text = format!(
        "BEGIN:VCARD\r\nVERSION:3.0\r\nFN:An\r\nPHOTO;ENCODING=b:{}\r\nEND:VCARD\r\n",
        base64::engine::general_purpose::STANDARD.encode(b"not an image at all")
    );
    assert!(one(&text).photo.is_none());
}

// ─── Round trip ─────────────────────────────────────────────

#[test]
fn a_contact_survives_leaving_and_coming_back() {
    // The acceptance test for the export: what goes out has to come home as
    // the same person. Everything the contact form can hold is in here.
    let properties = json!({
        "display_name": "fullname",
        "nickname": "Ann",
        "birthday": "1994-03-02",
        "relationship_type": "Friend, Colleague",
        "contact_frequency": "monthly",
        "tags": ["work", "vietnam"],
        "important_dates": [
            { "label": "Anniversary", "date": "2019-06-01" },
            { "label": "First met", "date": "2018-11-20" },
        ],
        "experiences": [{
            "company": "Acme Corp", "role": "Staff Engineer",
            "start": "", "end": "", "current": true,
        }],
        "details": [
            { "label": "Work Email", "value": "an@acme.example", "type": "email" },
            { "label": "Mobile Phone", "value": "+84 90 123 4567", "type": "phone" },
            { "label": "LinkedIn", "value": "https://linkedin.com/in/an", "type": "url" },
            { "label": "Home Address", "value": "12 Phố Huế, Hà Nội", "type": "text" },
            { "label": "How We Met", "value": "At a meetup; in 2018, in Hà Nội", "type": "text" },
        ],
    });
    let photo = Photo { bytes: PNG.to_vec(), extension: "png".into() };

    let written = export(&[ExportContact {
        title: "An Nguyễn",
        properties: &properties,
        body: "Likes long walks,\nand short meetings.",
        photo: Some(&photo),
    }]);

    let back = one(&written);

    assert_eq!(back.title, "An Nguyễn");
    assert_eq!(prop_str(&back, "nickname"), "Ann");
    assert_eq!(prop_str(&back, "birthday"), "1994-03-02");
    // A list on the way back, whichever shape went out: the vault used to
    // hold one comma-separated string, which lost any relationship whose own
    // name contained a comma.
    assert_eq!(back.properties["relationship_type"], json!(["Friend", "Colleague"]));
    assert_eq!(prop_str(&back, "contact_frequency"), "monthly");
    assert_eq!(back.properties["tags"], json!(["work", "vietnam"]));
    assert_eq!(back.body, "Likes long walks,\nand short meetings.");
    assert_eq!(back.photo.as_ref().map(|p| p.bytes.as_slice()), Some(PNG));

    assert_eq!(
        back.properties["important_dates"],
        json!([
            { "label": "Anniversary", "date": "2019-06-01" },
            { "label": "First met", "date": "2018-11-20" },
        ])
    );
    assert_eq!(
        back.properties["experiences"][0]["company"],
        json!("Acme Corp")
    );
    assert_eq!(back.properties["experiences"][0]["role"], json!("Staff Engineer"));

    // Labels and values both survive, including the punctuation that is
    // syntax in this format.
    assert_eq!(
        details(&back),
        [
            ("Work Email".to_string(), "an@acme.example".to_string()),
            ("Mobile Phone".to_string(), "+84 90 123 4567".to_string()),
            ("LinkedIn".to_string(), "https://linkedin.com/in/an".to_string()),
            ("Home Address".to_string(), "12 Phố Huế, Hà Nội".to_string()),
            ("How We Met".to_string(), "At a meetup; in 2018, in Hà Nội".to_string()),
        ]
    );
}

#[test]
fn relationships_go_out_and_come_back_as_a_list() {
    // Written as a list, and read as one.
    let properties = json!({ "relationship_type": ["Friend", "Đồng nghiệp cũ"] });
    let written = export(&[ExportContact {
        title: "An",
        properties: &properties,
        body: "",
        photo: None,
    }]);
    assert_eq!(
        one(&written).properties["relationship_type"],
        json!(["Friend", "Đồng nghiệp cũ"])
    );
}

#[test]
fn a_vault_still_holding_the_old_string_still_exports() {
    // Reading only the new shape would silently drop the relationship of
    // every person written before this.
    let properties = json!({ "relationship_type": "Friend, Colleague" });
    let written = export(&[ExportContact {
        title: "An",
        properties: &properties,
        body: "",
        photo: None,
    }]);
    assert!(written.contains("X-SYNABIT-RELATIONSHIP:Friend\\, Colleague"), "{}", written);
    assert_eq!(
        one(&written).properties["relationship_type"],
        json!(["Friend", "Colleague"])
    );
}

#[test]
fn a_birthday_with_no_year_survives_the_round_trip() {
    let properties = json!({ "birthday": "03-02" });
    let written = export(&[ExportContact {
        title: "An",
        properties: &properties,
        body: "",
        photo: None,
    }]);
    assert!(written.contains("BDAY:--0302"), "{}", written);
    assert_eq!(prop_str(&one(&written), "birthday"), "03-02");
}

#[test]
fn what_is_written_is_a_well_formed_card() {
    let properties = json!({ "details": [
        { "label": "Email", "value": "an@example.com", "type": "email" },
    ]});
    let written = export(&[ExportContact {
        title: "An Nguyễn",
        properties: &properties,
        body: "",
        photo: None,
    }]);

    assert!(written.starts_with("BEGIN:VCARD\r\n"));
    assert!(written.ends_with("END:VCARD\r\n"));
    assert!(written.contains("VERSION:4.0\r\n"));
    // Every version requires `N`, and a reader that finds none may drop the
    // card outright.
    assert!(written.contains("N:An Nguyễn;;;;\r\n"), "{}", written);
    assert!(written.lines().all(|l| l.trim_end().len() <= 75), "{}", written);
}

#[test]
fn a_whole_address_book_goes_out_in_one_file() {
    let a = json!({});
    let b = json!({});
    let written = export(&[
        ExportContact { title: "An", properties: &a, body: "", photo: None },
        ExportContact { title: "Bình", properties: &b, body: "", photo: None },
    ]);
    assert_eq!(written.matches("BEGIN:VCARD").count(), 2);
    assert_eq!(
        import(&written).into_iter().map(|c| c.title).collect::<Vec<_>>(),
        ["An", "Bình"]
    );
}

// ─── Scale ──────────────────────────────────────────────────

#[test]
fn two_thousand_cards_are_read_in_well_under_a_second() {
    // The gate for the import as a whole is ten seconds for two thousand
    // contacts. Parsing is one part of that, and it should not be the part
    // anybody notices — the bound below is loose enough not to fail on a
    // loaded machine, and the real number is printed for whoever changes this.
    let mut text = String::new();
    for i in 0..2000 {
        text.push_str(&format!(
            "BEGIN:VCARD\r\nVERSION:3.0\r\n\
             N:Nguyễn;Person{i};;;\r\nFN:Person{i} Nguyễn\r\n\
             ORG:Company {i};Engineering\r\nTITLE:Engineer\r\n\
             EMAIL;TYPE=WORK:person{i}@example.com\r\n\
             TEL;TYPE=CELL:+84 90 {i:04} 000\r\n\
             ADR;TYPE=home:;;{i} Phố Huế;Hà Nội;;100000;Việt Nam\r\n\
             BDAY:1994-03-02\r\nCATEGORIES:work,imported\r\n\
             NOTE:Imported from a phone export.\r\n\
             END:VCARD\r\n"
        ));
    }

    let started = std::time::Instant::now();
    let contacts = import(&text);
    let elapsed = started.elapsed();

    assert_eq!(contacts.len(), 2000);
    assert_eq!(contacts[1999].title, "Person1999 Nguyễn");
    println!("parsed 2000 vCards in {:?}", elapsed);
    assert!(elapsed.as_secs() < 2, "took {:?}", elapsed);
}
