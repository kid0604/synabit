//! Getting a contact list into the vault, and back out of it.
//!
//! The reading and writing of the formats themselves is in [`crate::people`].
//! What is here is the part that has to know about a vault: opening the file,
//! putting photos where assets go, and asking the database who is already
//! known.
//!
//! Nothing here writes a person. The caller writes each one through the same
//! `write_node_file` everything else uses, so a contact that arrived in a file
//! is indexed, synced and linked exactly like one typed in by hand — the same
//! division the calendar's `.ics` import follows.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::db::DbState;
use crate::error::{AppError, AppResult};
use crate::people::{csv, dedupe, vcard};

/// One person read out of a file, ready to be written.
///
/// Deserialised as well as serialised: the duplicate check takes back the
/// contacts it was given, so the front end can drop the ones somebody
/// deselected before asking who they clash with.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContactImport {
    pub title: String,
    pub properties: Value,
    /// The person's own notes.
    pub body: String,
}

/// What a file turned out to hold.
#[derive(Debug, Clone, Serialize)]
pub struct ContactBatch {
    /// `"vcard"` or `"csv"`.
    pub format: &'static str,
    pub contacts: Vec<ContactImport>,
    /// How many rows named nobody and were left out.
    pub skipped: usize,
    /// Columns that were not recognised, for a screen that offers to map them.
    /// Empty for vCard, which has no columns.
    pub unmapped: Vec<String>,
}

/// The shape of a spreadsheet, for the screen that maps its columns.
#[derive(Debug, Clone, Serialize)]
pub struct ContactTable {
    pub headers: Vec<String>,
    pub columns: Vec<csv::Column>,
    /// The first few rows, so somebody can see what a column actually holds.
    pub sample: Vec<Vec<String>>,
    pub total_rows: usize,
}

/// A duplicate, with enough about the other person to describe them.
#[derive(Debug, Clone, Serialize)]
pub struct DuplicateReport {
    pub incoming: usize,
    pub existing_id: Option<String>,
    pub existing_title: Option<String>,
    pub existing_incoming: Option<usize>,
    pub reason: dedupe::Reason,
    /// Certain enough to merge without asking. A shared name is not.
    pub certain: bool,
}

/// Read a file, whatever its encoding claims to be.
///
/// Lossy on purpose: a 2.1 card that says `CHARSET=ISO-8859-1` and lies is
/// common, and one mangled character in one name is a better outcome than
/// refusing the other nine hundred contacts in the file.
fn read_text(source: &str) -> AppResult<String> {
    let bytes = std::fs::read(source)
        .map_err(|e| AppError::General(format!("Could not read that file: {}", e)))?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

fn looks_like_vcard(text: &str) -> bool {
    text.trim_start()
        .trim_start_matches('\u{feff}')
        .to_ascii_uppercase()
        .starts_with("BEGIN:VCARD")
}

/// The columns of a spreadsheet, and what each one looks like it holds.
///
/// Only for a file whose columns are not all recognised. A Google or Outlook
/// export needs no such screen, and showing one anyway would put a step
/// between somebody and the thing they asked for.
#[tauri::command]
pub fn read_contact_columns(source: String) -> AppResult<ContactTable> {
    let text = read_text(&source)?;
    let table = csv::parse(&text);
    let columns = csv::detect(&table.headers);
    let sample = table.rows.iter().take(5).cloned().collect();

    Ok(ContactTable {
        headers: table.headers,
        columns,
        sample,
        total_rows: table.rows.len(),
    })
}

/// Every contact in a file.
///
/// The format is worked out from the contents rather than the file's name:
/// plenty of address books arrive as `contacts.txt`, and plenty of `.csv`
/// files downloaded from a phone are vCards.
///
/// Photos are written into the vault's assets as they are read, and the
/// person carries the path. That happens here rather than in the caller
/// because the alternative is sending every photo across to the front end and
/// back again — on a two-thousand-contact export, most of the file twice.
#[tauri::command]
pub fn read_contacts(
    vault_path: String,
    source: String,
    columns: Option<Vec<csv::Column>>,
) -> AppResult<ContactBatch> {
    let text = read_text(&source)?;

    let (format, contacts, total, unmapped) = if looks_like_vcard(&text) {
        let contacts = vcard::import(&text);
        let total = text.to_ascii_uppercase().matches("BEGIN:VCARD").count();
        ("vcard", contacts, total, Vec::new())
    } else {
        let table = csv::parse(&text);
        let columns = columns.unwrap_or_else(|| csv::detect(&table.headers));
        let unmapped = table
            .headers
            .iter()
            .zip(&columns)
            .filter(|(header, column)| {
                column.field.is_none() && !csv::is_label_column(header)
            })
            .map(|(header, _)| header.clone())
            .filter(|header| !header.trim().is_empty())
            .collect();
        let total = table.rows.len();
        ("csv", csv::to_contacts(&table, &columns), total, unmapped)
    };

    let skipped = total.saturating_sub(contacts.len());

    let contacts = contacts
        .into_iter()
        .map(|contact| {
            let mut properties = Value::Object(contact.properties);
            if let Some(photo) = contact.photo {
                // A failed photo is not a failed contact.
                match super::nodes::save_asset(
                    vault_path.clone(),
                    format!("contact.{}", photo.extension),
                    photo.bytes,
                ) {
                    Ok(rel_path) => {
                        if let Some(map) = properties.as_object_mut() {
                            map.insert("avatar".into(), Value::String(rel_path));
                        }
                    }
                    Err(e) => log::warn!("Could not store a contact photo: {}", e),
                }
            }
            ContactImport {
                title: contact.title,
                properties,
                body: contact.body,
            }
        })
        .collect();

    Ok(ContactBatch {
        format,
        contacts,
        skipped,
        unmapped,
    })
}

/// Which of these contacts the vault already has, and why it thinks so.
#[tauri::command]
pub fn find_contact_duplicates(
    state: tauri::State<'_, DbState>,
    contacts: Vec<ContactImport>,
) -> AppResult<Vec<DuplicateReport>> {
    let people = {
        let db = state.lock().unwrap_or_else(|e| e.into_inner());
        db.get_nodes_by_type("person")?
    };

    let existing: Vec<dedupe::Existing> = people
        .iter()
        .filter(|p| p.properties.get("is_owner") != Some(&Value::Bool(true)))
        .map(|p| dedupe::Existing {
            id: &p.id,
            title: &p.title,
            properties: &p.properties,
        })
        .collect();

    // `find_duplicates` works on what a parser produced, so the contacts come
    // back into that shape rather than the matching being written twice.
    let incoming: Vec<vcard::ImportedContact> = contacts
        .into_iter()
        .map(|c| vcard::ImportedContact {
            title: c.title,
            properties: c.properties.as_object().cloned().unwrap_or_default(),
            body: c.body,
            photo: None,
        })
        .collect();

    let titles: std::collections::HashMap<&str, &str> = people
        .iter()
        .map(|p| (p.id.as_str(), p.title.as_str()))
        .collect();

    Ok(dedupe::find_duplicates(&incoming, &existing)
        .into_iter()
        .map(|d| DuplicateReport {
            existing_title: d
                .existing_id
                .as_deref()
                .and_then(|id| titles.get(id).map(|t| t.to_string())),
            certain: d.reason.is_certain(),
            incoming: d.incoming,
            existing_id: d.existing_id,
            existing_incoming: d.existing_incoming,
            reason: d.reason,
        })
        .collect())
}

/// When each person was last involved in anything, by vault path.
///
/// The People screen needs the same answer the reminder engine works from.
/// Without it the dot beside somebody's name counts from the last interaction
/// anybody typed in by hand, while the notification counts from the last note
/// that mentioned them — and the two disagree about the same person.
#[tauri::command]
pub fn last_contact_dates(
    state: tauri::State<'_, DbState>,
) -> AppResult<std::collections::HashMap<String, String>> {
    let db = state.lock().unwrap_or_else(|e| e.into_inner());
    db.last_contact_by_person()
}

/// What a migration pass did.
#[derive(Debug, Clone, Default, Serialize)]
pub struct MigrationReport {
    pub people_changed: usize,
    pub interactions_moved: usize,
    /// People whose move could not be finished. They keep what they had and
    /// the next pass tries again.
    pub failed: Vec<String>,
}

/// Move interactions out of people's frontmatter and into files of their own.
///
/// Safe to run at any time and safe to run twice: a person with nothing left
/// to move produces an empty plan, and an empty plan touches no file. See
/// [`crate::people::migrate`] for what is moved and why.
///
/// Interactions are written before the person is changed, never after. If the
/// pass dies halfway, the worst case is a file that exists twice over — once
/// as a node and once still inside the person — which the next pass resolves.
/// The other order would lose them.
#[tauri::command]
pub fn migrate_people_storage(
    app_handle: tauri::AppHandle,
    state: tauri::State<'_, DbState>,
    vault_path: String,
) -> AppResult<MigrationReport> {
    let people = {
        let db = state.lock().unwrap_or_else(|e| e.into_inner());
        db.get_nodes_by_type("person")?
    };

    let mut report = MigrationReport::default();

    for person in people {
        let plan = crate::people::migrate::plan_person(
            &person.id,
            &person.title,
            person.stable_id(),
            &person.properties,
            || uuid::Uuid::new_v4().to_string(),
        );
        if plan.is_empty() {
            continue;
        }

        let mut wrote_all = true;
        for write in &plan.interactions {
            let result = super::nodes::write_node_file(
                app_handle.clone(),
                state.clone(),
                vault_path.clone(),
                write.rel_path.clone(),
                write.title.clone(),
                write.node_type.to_string(),
                write.properties.clone(),
                Some(write.content.clone()),
            );
            match result {
                Ok(()) => report.interactions_moved += 1,
                Err(e) => {
                    log::error!("Could not move an interaction for {}: {}", person.title, e);
                    wrote_all = false;
                    break;
                }
            }
        }

        if !wrote_all {
            // They keep what they had rather than losing it.
            report.failed.push(person.title.clone());
            continue;
        }

        let patched = super::nodes::write_node_file(
            app_handle.clone(),
            state.clone(),
            vault_path.clone(),
            person.id.clone(),
            person.title.clone(),
            "person".to_string(),
            Value::Object(plan.patch.clone()),
            None,
        );
        match patched {
            Ok(()) => report.people_changed += 1,
            Err(e) => {
                log::error!("Could not tidy {}: {}", person.title, e);
                report.failed.push(person.title.clone());
            }
        }
    }

    if report.people_changed > 0 || report.interactions_moved > 0 {
        log::info!(
            "People storage: {} interactions moved out of {} people",
            report.interactions_moved,
            report.people_changed
        );
    }
    Ok(report)
}

/// Everything the vault knows about one person, in one answer.
///
/// What the card shown before a meeting reads, and what the assistant reads
/// when asked about somebody. One place, so the two cannot disagree.
#[tauri::command]
pub fn person_brief(
    state: tauri::State<'_, DbState>,
    person_id: String,
) -> AppResult<Option<crate::people::brief::PersonBrief>> {
    let db = state.lock().unwrap_or_else(|e| e.into_inner());
    db.person_brief(&person_id, chrono::Local::now().date_naive())
}

/// How you know somebody: the shortest chain of links between two people.
#[tauri::command]
pub fn path_between_people(
    state: tauri::State<'_, DbState>,
    from: String,
    to: String,
) -> AppResult<Vec<crate::db::edges::PersonConnection>> {
    let db = state.lock().unwrap_or_else(|e| e.into_inner());
    let path = db.path_between_people(&from, &to)?;

    // Answered with names rather than paths: a chain of file names is not a
    // sentence anybody can read.
    let mut out = Vec::new();
    for (i, id) in path.iter().enumerate() {
        let Some(node) = db.get_node(id)? else { continue };
        // The relationship on each step is how the person before them is
        // linked, which is what makes the chain read as a sentence.
        let relation = if i == 0 {
            String::new()
        } else {
            db.person_connections(&path[i - 1])?
                .into_iter()
                .find(|c| &c.person_id == id)
                .map(|c| c.relation_type)
                .unwrap_or_default()
        };
        out.push(crate::db::edges::PersonConnection {
            person_id: node.id,
            name: node.title,
            relation_type: relation,
        });
    }
    Ok(out)
}

/// Every interaction recorded with this person, newest first.
#[tauri::command]
pub fn person_interactions(
    state: tauri::State<'_, DbState>,
    person_id: String,
) -> AppResult<Vec<crate::models::node::NodeMetadata>> {
    let db = state.lock().unwrap_or_else(|e| e.into_inner());
    db.nodes_about_person(&person_id, "interaction")
}

/// Who this person is linked to, resolved to who those people are now.
#[tauri::command]
pub fn person_connections(
    state: tauri::State<'_, DbState>,
    person_id: String,
) -> AppResult<Vec<crate::db::edges::PersonConnection>> {
    let db = state.lock().unwrap_or_else(|e| e.into_inner());
    db.person_connections(&person_id)
}

/// What to change on a person already in the vault, given an incoming copy.
///
/// A patch, in the sense `write_node_file` means: keys it does not name are
/// left alone. Nothing already recorded is overwritten — see
/// [`crate::people::dedupe::merge`].
#[tauri::command]
pub fn merge_contact(existing: Value, incoming: Value) -> Value {
    let incoming = incoming.as_object().cloned().unwrap_or_default();
    Value::Object(dedupe::merge(&existing, &incoming))
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ExportFormat {
    Vcard,
    Csv,
}

/// Write every person in the vault to a file.
///
/// The whole address book, not a filtered view: an export is for taking the
/// contacts somewhere else, and a filter would silently leave people behind.
#[tauri::command]
pub fn export_contacts(
    state: tauri::State<'_, DbState>,
    vault_path: String,
    destination: String,
    format: ExportFormat,
) -> AppResult<usize> {
    let people = {
        let db = state.lock().unwrap_or_else(|e| e.into_inner());
        db.get_nodes_by_type("person")?
    };
    // The owner's own card is the vault's idea of "me", not a contact.
    let people: Vec<_> = people
        .into_iter()
        .filter(|p| p.properties.get("is_owner") != Some(&Value::Bool(true)))
        .filter(|p| !p.title.trim().is_empty())
        .collect();

    let text = match format {
        ExportFormat::Vcard => {
            let photos: Vec<Option<vcard::Photo>> =
                people.iter().map(|p| read_avatar(&vault_path, p)).collect();
            let cards: Vec<vcard::ExportContact> = people
                .iter()
                .zip(&photos)
                .map(|(p, photo)| vcard::ExportContact {
                    title: &p.title,
                    properties: &p.properties,
                    body: &p.content,
                    photo: photo.as_ref(),
                })
                .collect();
            vcard::export(&cards)
        }
        ExportFormat::Csv => write_csv(&people),
    };

    let path = std::path::Path::new(&destination);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| AppError::General(format!("Could not prepare the folder: {}", e)))?;
    }
    std::fs::write(path, text)
        .map_err(|e| AppError::General(format!("Could not write the contacts: {}", e)))?;

    Ok(people.len())
}

fn read_avatar(vault_path: &str, person: &crate::models::node::NodeMetadata) -> Option<vcard::Photo> {
    let rel = person.properties.get("avatar")?.as_str()?;
    if rel.trim().is_empty() {
        return None;
    }
    let path = crate::path_utils::resolve_safe_path(vault_path, rel).ok()?;
    let bytes = std::fs::read(path).ok()?;
    let extension = infer::get(&bytes)
        .filter(|kind| kind.mime_type().starts_with("image/"))
        .map(|kind| kind.extension().to_string())?;
    Some(vcard::Photo { bytes, extension })
}

/// A spreadsheet of the address book, in the shape this app can read back.
fn write_csv(people: &[crate::models::node::NodeMetadata]) -> String {
    let headers = [
        "Name",
        "Nickname",
        "Company",
        "Job Title",
        "E-mail 1 - Label",
        "E-mail 1 - Value",
        "Phone 1 - Label",
        "Phone 1 - Value",
        "Birthday",
        "Labels",
        "Notes",
    ];

    let mut out = String::new();
    out.push_str(&headers.join(","));
    out.push_str("\r\n");

    for person in people {
        let props = &person.properties;
        let details = props.get("details").and_then(Value::as_array);
        let first = |needle: &str| -> (String, String) {
            details
                .and_then(|arr| {
                    arr.iter().find_map(|d| {
                        let label = d.get("label")?.as_str()?;
                        if !label.to_ascii_lowercase().contains(needle) {
                            return None;
                        }
                        let value = d.get("value")?.as_str()?;
                        Some((label.to_string(), value.to_string()))
                    })
                })
                .unwrap_or_default()
        };
        let (email_label, email) = first("email");
        let (phone_label, phone) = first("phone");

        let job = props
            .get("experiences")
            .and_then(Value::as_array)
            .and_then(|jobs| jobs.iter().find(|j| j.get("current") == Some(&Value::Bool(true))).or(jobs.first()));
        let job_field = |key: &str| {
            job.and_then(|j| j.get(key))
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string()
        };

        let str_of = |key: &str| {
            props
                .get(key)
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string()
        };
        let tags = props
            .get("tags")
            .and_then(Value::as_array)
            .map(|t| {
                t.iter()
                    .filter_map(Value::as_str)
                    .collect::<Vec<_>>()
                    .join(" ::: ")
            })
            .unwrap_or_default();

        let row = [
            person.title.clone(),
            str_of("nickname"),
            job_field("company"),
            job_field("role"),
            email_label,
            email,
            phone_label,
            phone,
            str_of("birthday"),
            tags,
            person.content.trim().to_string(),
        ];
        out.push_str(
            &row.iter()
                .map(|f| quote_csv(f))
                .collect::<Vec<_>>()
                .join(","),
        );
        out.push_str("\r\n");
    }

    out
}

/// Quote a field only when it needs it, and double any quote inside it.
fn quote_csv(field: &str) -> String {
    if field.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", field.replace('"', "\"\""))
    } else {
        field.to_string()
    }
}

#[cfg(test)]
mod tests;
