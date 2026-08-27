//! Reading a contact list out of a spreadsheet.
//!
//! The other half of getting people into a vault. vCard is what phones export;
//! CSV is what everything else does — Google Contacts, Outlook, Notion, an
//! Airtable base, a column somebody kept by hand.
//!
//! Two of those shapes are recognised on sight. The rest arrive as a table
//! with headers, and the caller shows them to the user to map: guessing wrong
//! about an unknown column is worse than asking, because a wrong guess is
//! silent and lands in the vault looking deliberate.

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use super::vcard::ImportedContact;

/// A parsed file: the header row, and everything under it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Table {
    pub headers: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

/// What one column holds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "label", rename_all = "snake_case")]
pub enum Field {
    FullName,
    GivenName,
    MiddleName,
    FamilyName,
    Nickname,
    Company,
    Role,
    Birthday,
    Notes,
    Tags,
    /// A contact detail, carrying the label it will show under.
    Email(String),
    Phone(String),
    Url(String),
    Text(String),
}

/// One column of the file, and what to do with it.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Column {
    /// `None` means "leave this one out".
    pub field: Option<Field>,
    /// The column whose value names this one.
    ///
    /// Google and Outlook both write pairs — `E-mail 1 - Label` holding
    /// `Work` beside `E-mail 1 - Value` holding the address. Taking the label
    /// from the row rather than the header is what makes an imported address
    /// book keep the labels its owner chose.
    pub label_from: Option<usize>,
}

// ─── Reading the file ───────────────────────────────────────

/// Split a CSV file into rows, honouring quotes.
///
/// Written out rather than pulled in as a dependency because the whole of
/// RFC 4180 is the three rules below: a quoted field may contain commas and
/// newlines, and a doubled quote inside one is a literal quote.
fn read_rows(text: &str) -> Vec<Vec<String>> {
    let mut rows: Vec<Vec<String>> = Vec::new();
    let mut row: Vec<String> = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut chars = text.chars().peekable();

    while let Some(c) = chars.next() {
        if quoted {
            match c {
                '"' if chars.peek() == Some(&'"') => {
                    chars.next();
                    field.push('"');
                }
                '"' => quoted = false,
                _ => field.push(c),
            }
            continue;
        }
        match c {
            '"' => quoted = true,
            ',' => row.push(std::mem::take(&mut field)),
            '\r' => {
                // A bare \r ends a line too; \r\n must not end two.
                if chars.peek() == Some(&'\n') {
                    chars.next();
                }
                row.push(std::mem::take(&mut field));
                rows.push(std::mem::take(&mut row));
            }
            '\n' => {
                row.push(std::mem::take(&mut field));
                rows.push(std::mem::take(&mut row));
            }
            _ => field.push(c),
        }
    }

    if !field.is_empty() || !row.is_empty() {
        row.push(field);
        rows.push(row);
    }

    // A trailing newline leaves one empty row behind, and so does a file that
    // pads its end with blank lines.
    rows.retain(|r| r.iter().any(|f| !f.trim().is_empty()));
    rows
}

/// The header row and the rows beneath it.
pub fn parse(text: &str) -> Table {
    // A spreadsheet saved by Excel opens with a byte-order mark, which would
    // otherwise become part of the first header's name and stop it matching.
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);

    let mut rows = read_rows(text);
    if rows.is_empty() {
        return Table::default();
    }
    let headers = rows
        .remove(0)
        .into_iter()
        .map(|h| h.trim().to_string())
        .collect();
    Table { headers, rows }
}

// ─── Working out what the columns mean ──────────────────────

/// Strip the `1` out of `E-mail 1 - Value`, so one rule covers all of them.
fn normalize(header: &str) -> String {
    header
        .to_ascii_lowercase()
        .replace(['_', '-'], " ")
        .split_whitespace()
        .filter(|word| !word.chars().all(|c| c.is_ascii_digit()))
        .collect::<Vec<_>>()
        .join(" ")
}

/// The stem a `… - Label` column shares with its `… - Value` column.
fn pair_stem(header: &str) -> Option<(String, &'static str)> {
    let lower = header.to_ascii_lowercase();
    for (suffix, role) in [
        (" - value", "value"),
        (" - label", "label"),
        (" - type", "label"),
    ] {
        if let Some(stem) = lower.strip_suffix(suffix) {
            return Some((stem.trim().to_string(), role));
        }
    }
    None
}

/// Whether this column exists only to name another one.
///
/// `E-mail 1 - Label` holds `Work`, and is read through `E-mail 1 - Value`.
/// It has no field of its own, which is not the same as not being understood:
/// telling somebody it was unrecognised would send them to map a column that
/// is already doing its job.
pub fn is_label_column(header: &str) -> bool {
    matches!(pair_stem(header), Some((_, "label")))
}

/// Guess what each column is, well enough that Google and Outlook need no
/// mapping at all.
pub fn detect(headers: &[String]) -> Vec<Column> {
    // Where each `… - Label` column sits, so a `… - Value` can find its pair.
    let mut labels: Vec<(String, usize)> = Vec::new();
    for (i, header) in headers.iter().enumerate() {
        if let Some((stem, "label")) = pair_stem(header) {
            labels.push((stem, i));
        }
    }

    headers
        .iter()
        .map(|header| {
            let paired = pair_stem(header);
            if matches!(paired, Some((_, "label"))) {
                // Read through its value column, never on its own.
                return Column::default();
            }
            let label_from = paired.as_ref().and_then(|(stem, _)| {
                labels.iter().find(|(s, _)| s == stem).map(|(_, i)| *i)
            });
            let name = normalize(paired.as_ref().map(|(s, _)| s.as_str()).unwrap_or(header));

            Column {
                field: field_for(&name),
                label_from,
            }
        })
        .collect()
}

fn field_for(name: &str) -> Option<Field> {
    let has = |needle: &str| name.contains(needle);

    // Order matters: "first name" has to be tested before "name".
    let field = match () {
        _ if name == "name" || has("full name") || has("display name") => Field::FullName,
        _ if has("first name") || has("given name") => Field::GivenName,
        _ if has("middle name") => Field::MiddleName,
        _ if has("last name") || has("family name") || has("surname") => Field::FamilyName,
        _ if has("nickname") => Field::Nickname,

        _ if has("e mail") || has("email") => Field::Email("Email".into()),
        _ if has("phone") || has("mobile") || has("tel") => Field::Phone("Phone".into()),
        _ if has("website") || has("web page") || has("url") || has("homepage") => {
            Field::Url("Website".into())
        }
        _ if has("linkedin") => Field::Url("LinkedIn".into()),
        _ if has("twitter") => Field::Url("Twitter".into()),
        _ if has("github") => Field::Url("GitHub".into()),

        _ if has("job title") || has("organization title") || name == "title" || has("role") => {
            Field::Role
        }
        _ if has("company") || has("organization") || has("organisation") || has("employer") => {
            Field::Company
        }

        _ if has("birthday") || has("birth date") || has("date of birth") => Field::Birthday,
        _ if has("note") || has("comment") => Field::Notes,
        _ if has("label") || has("categor") || has("tag") || has("group membership") => Field::Tags,

        _ if has("address") || has("street") || has("city") => Field::Text("Address".into()),
        _ => return None,
    };
    Some(field)
}

// ─── Turning rows into people ───────────────────────────────

/// A label as Google writes it: `* myContacts ::: Friends`.
fn clean_labels(raw: &str) -> Vec<String> {
    raw.split(":::")
        .flat_map(|part| part.split(','))
        .map(|part| part.trim().trim_start_matches('*').trim())
        .filter(|part| !part.is_empty())
        // Google puts every contact in this one; it says nothing about anybody.
        .filter(|part| !part.eq_ignore_ascii_case("myContacts") && !part.eq_ignore_ascii_case("starred"))
        .map(|part| part.to_string())
        .collect()
}

/// `1994-03-02`, `03/02/1994`, `--03-02` → what the vault stores.
fn parse_date(raw: &str) -> Option<String> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }
    // Google writes a birthday with no year exactly this way.
    if let Some(rest) = raw.strip_prefix("--") {
        let digits: String = rest.chars().filter(|c| c.is_ascii_digit()).collect();
        if digits.len() == 4 {
            return Some(format!("{}-{}", &digits[..2], &digits[2..]));
        }
        return None;
    }

    let parts: Vec<&str> = raw.split(['-', '/', '.']).map(str::trim).collect();
    let nums: Vec<u32> = parts.iter().filter_map(|p| p.parse().ok()).collect();
    if nums.len() != parts.len() {
        return None;
    }

    match nums.as_slice() {
        // A four-digit part is the year, wherever it sits.
        [y, m, d] if *y > 31 => Some(format!("{:04}-{:02}-{:02}", y, m, d)),
        [m, d, y] if *y > 31 => Some(format!("{:04}-{:02}-{:02}", y, m, d)),
        _ => None,
    }
}

fn kind_of(field: &Field) -> &'static str {
    match field {
        Field::Email(_) => "email",
        Field::Phone(_) => "phone",
        Field::Url(_) => "url",
        _ => "text",
    }
}

/// Every row that names somebody, as a person ready to be written.
pub fn to_contacts(table: &Table, columns: &[Column]) -> Vec<ImportedContact> {
    table
        .rows
        .iter()
        .filter_map(|row| row_to_contact(row, columns))
        .collect()
}

fn row_to_contact(row: &[String], columns: &[Column]) -> Option<ImportedContact> {
    let cell = |i: usize| row.get(i).map(|s| s.trim()).unwrap_or("");

    let mut full_name = String::new();
    let (mut given, mut middle, mut family) = (String::new(), String::new(), String::new());
    let mut nickname = String::new();
    let mut company = String::new();
    let mut role = String::new();
    let mut birthday: Option<String> = None;
    let mut body = String::new();
    let mut tags: Vec<String> = Vec::new();
    let mut details: Vec<Value> = Vec::new();

    for (i, column) in columns.iter().enumerate() {
        let Some(field) = &column.field else { continue };
        let value = cell(i);
        if value.is_empty() {
            continue;
        }

        match field {
            Field::FullName => full_name = value.to_string(),
            Field::GivenName => given = value.to_string(),
            Field::MiddleName => middle = value.to_string(),
            Field::FamilyName => family = value.to_string(),
            Field::Nickname => nickname = value.to_string(),
            Field::Company if company.is_empty() => company = value.to_string(),
            Field::Role if role.is_empty() => role = value.to_string(),
            Field::Company | Field::Role => {}
            Field::Birthday => birthday = parse_date(value),
            Field::Notes => {
                if !body.is_empty() {
                    body.push_str("\n\n");
                }
                body.push_str(value);
            }
            Field::Tags => tags.extend(clean_labels(value).into_iter().map(|t| t.to_lowercase())),
            Field::Email(default) | Field::Phone(default) | Field::Url(default) | Field::Text(default) => {
                // The row's own label wins, so `Work` beside an address
                // becomes "Work Email" rather than a second plain "Email".
                let from_row = column.label_from.map(cell).filter(|l| !l.is_empty());
                let label = match from_row {
                    Some(from_row) => decorate(from_row, default),
                    None => default.clone(),
                };
                details.push(json!({
                    "label": label,
                    "value": value,
                    "type": kind_of(field),
                }));
            }
        }
    }

    let title = if !full_name.is_empty() {
        full_name
    } else {
        [given.as_str(), middle.as_str(), family.as_str()]
            .iter()
            .filter(|s| !s.is_empty())
            .cloned()
            .collect::<Vec<_>>()
            .join(" ")
    };
    let title = title.trim().to_string();
    if title.is_empty() {
        // Nothing to call them and nothing to show in a list.
        return None;
    }

    let mut properties = Map::new();
    properties.insert("display_name".into(), json!("fullname"));
    if !nickname.is_empty() {
        properties.insert("nickname".into(), json!(nickname));
    }
    if let Some(bday) = birthday {
        properties.insert("birthday".into(), json!(bday));
    }
    if !tags.is_empty() {
        tags.dedup();
        properties.insert("tags".into(), json!(tags));
    }
    if !company.is_empty() || !role.is_empty() {
        properties.insert(
            "experiences".into(),
            json!([{
                "company": company, "role": role,
                "start": "", "end": "", "current": true,
            }]),
        );
        if !company.is_empty() {
            properties.insert("company".into(), json!(company));
        }
    }
    if !details.is_empty() {
        let first_of = |needle: &str| -> Option<String> {
            details.iter().find_map(|d| {
                let label = d.get("label")?.as_str()?.to_ascii_lowercase();
                label
                    .contains(needle)
                    .then(|| d.get("value")?.as_str().map(str::to_string))
                    .flatten()
            })
        };
        if let Some(email) = first_of("email") {
            properties.insert("email".into(), json!(email));
        }
        if let Some(phone) = first_of("phone") {
            properties.insert("phone".into(), json!(phone));
        }
        properties.insert("details".into(), json!(details));
    }

    Some(ImportedContact {
        title,
        properties,
        body,
        photo: None,
    })
}

/// `Work` + `Email` → `Work Email`, without repeating a word already there.
fn decorate(from_row: &str, default: &str) -> String {
    let row = from_row.trim();
    if row.is_empty() {
        return default.to_string();
    }
    if row.to_ascii_lowercase().contains(&default.to_ascii_lowercase()) {
        return row.to_string();
    }
    // Keeping the default word in the label is what lets the People screen
    // find "the first detail whose label contains phone".
    format!("{} {}", row, default)
}

#[cfg(test)]
mod tests;
