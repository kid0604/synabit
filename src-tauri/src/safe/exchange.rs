//! Moving in and out: other password managers' exports, and Safe's own.
//!
//! Every file here is read and written by Rust. The WebView names a path and
//! gets back counts — a thousand passwords never pass through it on their way
//! into the Safe.
//!
//! # What is read
//!
//! | From | File | Notes |
//! | --- | --- | --- |
//! | 1Password | `.1pux` | a zip with `export.data`; sections, one-time codes, archived items tagged |
//! | Bitwarden | `.json` | unencrypted export only — the password-protected one is refused with a reason |
//! | KeePass, KeePassXC | `.xml` | groups become tags; entries' own history and the recycle bin are skipped |
//! | Chrome, Edge, Firefox, Safari, Bitwarden, anything | `.csv` | columns recognised by name |
//! | Safe | `.safe-export` | everything, history included; needs the export's own password |
//!
//! Warnings name what was skipped and why. They never carry a value.
//!
//! # What is written
//!
//! `.safe-export`: every item, sealed with XChaCha20-Poly1305 under a key from
//! Argon2id of a password chosen for the export — not the master password, and
//! not the Secret Key, so the file can be opened on a machine that has
//! neither. And CSV, in the column order Chrome and most importers read,
//! for leaving: plaintext, which is why the screen makes that a deliberate act.

use std::collections::BTreeMap;

use serde_json::Value;

use super::crypto::{self, ctx, CryptoError, KdfParams, Key, NONCE_LEN, SALT_LEN};
use super::item::{
    EditValue, FieldEdit, FieldKind, ItemBody, ItemEdit, ItemKind, SecretString, TotpEdit, UrlMatch, UrlRule,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Format {
    OnePux,
    BitwardenJson,
    KeePassXml,
    Csv,
    SafeExport,
}

#[derive(Debug, thiserror::Error)]
pub enum ExchangeError {
    #[error("this file is not one Safe knows how to read")]
    Unknown,
    #[error("this is a password-protected Bitwarden export; export it again without a password")]
    EncryptedBitwarden,
    #[error("the export password is wrong, or the file was altered")]
    WrongExportPassword,
    #[error("the file is damaged: {0}")]
    Damaged(String),
    #[error(transparent)]
    Crypto(#[from] CryptoError),
}

/// What an import produced, before anything is written.
pub struct Parsed {
    pub items: Vec<ItemBody>,
    pub warnings: Vec<String>,
}

/// Which format `bytes` are, from the name first and the contents to confirm.
pub fn detect(file_name: &str, bytes: &[u8]) -> Option<Format> {
    let lower = file_name.to_ascii_lowercase();
    if bytes.starts_with(EXPORT_MAGIC) {
        return Some(Format::SafeExport);
    }
    if lower.ends_with(".1pux") || (bytes.starts_with(b"PK") && lower.ends_with(".zip")) {
        return Some(Format::OnePux);
    }
    if lower.ends_with(".xml") || bytes.windows(12).take(512).any(|w| w == b"<KeePassFile") {
        return Some(Format::KeePassXml);
    }
    if lower.ends_with(".json") {
        return Some(Format::BitwardenJson);
    }
    if lower.ends_with(".csv") {
        return Some(Format::Csv);
    }
    None
}

pub fn parse(format: Format, bytes: &[u8], password: Option<&str>, now: i64) -> Result<Parsed, ExchangeError> {
    match format {
        Format::OnePux => onepux(bytes, now),
        Format::BitwardenJson => bitwarden(bytes, now),
        Format::KeePassXml => keepass(bytes, now),
        Format::Csv => csv(bytes, now),
        Format::SafeExport => {
            let items = open_export(bytes, password.unwrap_or(""))?;
            Ok(Parsed { items, warnings: Vec::new() })
        }
    }
}

// ─── building items ──────────────────────────────────────

/// An item being assembled from somebody else's format.
#[derive(Default)]
struct Draft {
    kind: ItemKind,
    title: String,
    fields: Vec<(String, FieldKind, String)>,
    urls: Vec<String>,
    tags: Vec<String>,
    notes: String,
    totp: Option<String>,
    favorite: bool,
}

impl Draft {
    fn field(&mut self, label: &str, kind: FieldKind, value: &str) {
        if !value.trim().is_empty() {
            self.fields.push((label.to_string(), kind, value.to_string()));
        }
    }

    /// The finished item, or why it was skipped. A draft with nothing in it
    /// is skipped rather than imported as an empty row.
    fn build(mut self, now: i64, warnings: &mut Vec<String>) -> Option<ItemBody> {
        if self.title.trim().is_empty() {
            // Firefox's CSV has no title at all; the site is the next best
            // name. Never a field's value — that could be the password.
            self.title = self.urls.iter().find_map(|u| super::item::host_of(u)).unwrap_or_else(|| "Untitled".to_string());
        }
        if self.fields.is_empty() && self.notes.trim().is_empty() && self.urls.is_empty() && self.totp.is_none() {
            warnings.push(format!("“{}” was empty and was skipped", self.title));
            return None;
        }
        let title = self.title.clone();
        let totp = match self.totp.take() {
            Some(t) if super::totp::Totp::parse(&t).is_ok() => TotpEdit::Set { v: SecretString::new(t) },
            Some(t) => {
                // Not a code this Safe can make; kept, hidden, rather than lost.
                self.fields.push(("one-time code".into(), FieldKind::Concealed, t));
                warnings.push(format!("“{title}”: its one-time code setup was not recognised and was kept as a hidden field"));
                TotpEdit::Unchanged
            }
            None => TotpEdit::Unchanged,
        };
        let edit = ItemEdit {
            kind: self.kind,
            title: self.title,
            fields: self
                .fields
                .into_iter()
                .map(|(label, kind, v)| FieldEdit { id: None, label, kind, value: EditValue::Set { v: SecretString::new(v) } })
                .collect(),
            urls: self.urls.into_iter().filter(|u| !u.trim().is_empty()).map(|url| UrlRule { url, match_: UrlMatch::Domain }).collect(),
            tags: self.tags,
            favorite: self.favorite,
            notes: self.notes,
            totp,
            expires_at: None,
        };
        match ItemBody::new_from(edit, now) {
            Ok(body) => Some(body),
            Err(e) => {
                warnings.push(format!("“{title}” was skipped: {e}"));
                None
            }
        }
    }
}

fn s<'a>(v: &'a Value, key: &str) -> &'a str {
    v.get(key).and_then(Value::as_str).unwrap_or("")
}

// ─── Bitwarden ───────────────────────────────────────────

fn bitwarden(bytes: &[u8], now: i64) -> Result<Parsed, ExchangeError> {
    let root: Value = serde_json::from_slice(bytes).map_err(|_| ExchangeError::Unknown)?;
    if root.get("encrypted").and_then(Value::as_bool) == Some(true) {
        return Err(ExchangeError::EncryptedBitwarden);
    }
    let items = root.get("items").and_then(Value::as_array).ok_or(ExchangeError::Unknown)?;
    let folders: BTreeMap<&str, &str> = root
        .get("folders")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|f| Some((f.get("id")?.as_str()?, f.get("name")?.as_str()?)))
        .collect();

    let mut warnings = Vec::new();
    let mut out = Vec::new();
    for item in items {
        let mut d = Draft { title: s(item, "name").to_string(), notes: s(item, "notes").to_string(), ..Default::default() };
        d.favorite = item.get("favorite").and_then(Value::as_bool).unwrap_or(false);
        if let Some(folder) = item.get("folderId").and_then(Value::as_str).and_then(|id| folders.get(id)) {
            d.tags.push(folder.to_string());
        }
        match item.get("type").and_then(Value::as_u64) {
            Some(1) => {
                d.kind = ItemKind::Login;
                let login = item.get("login").cloned().unwrap_or_default();
                d.field("username", FieldKind::Username, s(&login, "username"));
                d.field("password", FieldKind::Password, s(&login, "password"));
                d.totp = Some(s(&login, "totp").to_string()).filter(|t| !t.is_empty());
                d.urls = login.get("uris").and_then(Value::as_array).into_iter().flatten().map(|u| s(u, "uri").to_string()).collect();
            }
            Some(2) => d.kind = ItemKind::SecureNote,
            Some(3) => {
                d.kind = ItemKind::Card;
                let c = item.get("card").cloned().unwrap_or_default();
                d.field("cardholder", FieldKind::Text, s(&c, "cardholderName"));
                d.field("number", FieldKind::Concealed, s(&c, "number"));
                let expiry = format!("{}/{}", s(&c, "expMonth"), s(&c, "expYear"));
                d.field("expiry", FieldKind::Text, expiry.trim_matches('/'));
                d.field("cvv", FieldKind::Pin, s(&c, "code"));
                d.field("brand", FieldKind::Text, s(&c, "brand"));
            }
            Some(4) => {
                d.kind = ItemKind::Identity;
                let id = item.get("identity").cloned().unwrap_or_default();
                let name = [s(&id, "firstName"), s(&id, "middleName"), s(&id, "lastName")].iter().filter(|p| !p.is_empty()).cloned().collect::<Vec<_>>().join(" ");
                d.field("full name", FieldKind::Text, &name);
                d.field("email", FieldKind::Email, s(&id, "email"));
                d.field("phone", FieldKind::Phone, s(&id, "phone"));
                for (key, label) in [("ssn", "SSN"), ("passportNumber", "passport"), ("licenseNumber", "licence")] {
                    d.field(label, FieldKind::Concealed, s(&id, key));
                }
                let address = [s(&id, "address1"), s(&id, "address2"), s(&id, "city"), s(&id, "postalCode"), s(&id, "country")]
                    .iter().filter(|p| !p.is_empty()).cloned().collect::<Vec<_>>().join(", ");
                d.field("address", FieldKind::Multiline, &address);
            }
            Some(5) => {
                d.kind = ItemKind::SshKey;
                let k = item.get("sshKey").cloned().unwrap_or_default();
                d.field("private key", FieldKind::Concealed, s(&k, "privateKey"));
                d.field("public key", FieldKind::Multiline, s(&k, "publicKey"));
            }
            other => warnings.push(format!("“{}”: Bitwarden type {other:?} imported as Other", d.title)),
        }
        for f in item.get("fields").and_then(Value::as_array).into_iter().flatten() {
            let kind = match f.get("type").and_then(Value::as_u64) {
                Some(1) => FieldKind::Concealed,
                Some(3) => continue, // a link to another field, not a value
                _ => FieldKind::Text,
            };
            let value = match f.get("value") {
                Some(Value::String(v)) => v.clone(),
                Some(Value::Bool(b)) => b.to_string(),
                _ => String::new(),
            };
            d.field(s(f, "name"), kind, &value);
        }
        if let Some(b) = d.build(now, &mut warnings) {
            out.push(b);
        }
    }
    Ok(Parsed { items: out, warnings })
}

// ─── 1Password ───────────────────────────────────────────

fn onepux(bytes: &[u8], now: i64) -> Result<Parsed, ExchangeError> {
    use std::io::Read;
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(|_| ExchangeError::Unknown)?;
    let mut data = String::new();
    zip.by_name("export.data")
        .map_err(|_| ExchangeError::Damaged("no export.data inside".into()))?
        .take(256 * 1024 * 1024)
        .read_to_string(&mut data)
        .map_err(|e| ExchangeError::Damaged(e.to_string()))?;
    let data = zeroize::Zeroizing::new(data);
    let root: Value = serde_json::from_str(&data).map_err(|e| ExchangeError::Damaged(e.to_string()))?;

    let mut warnings = Vec::new();
    let mut out = Vec::new();
    for account in root.get("accounts").and_then(Value::as_array).into_iter().flatten() {
        for vault in account.get("vaults").and_then(Value::as_array).into_iter().flatten() {
            for raw in vault.get("items").and_then(Value::as_array).into_iter().flatten() {
                let item = raw.get("item").unwrap_or(raw);
                if item.get("trashed").and_then(Value::as_bool) == Some(true) || s(item, "state") == "deleted" {
                    continue;
                }
                let overview = item.get("overview").cloned().unwrap_or_default();
                let details = item.get("details").cloned().unwrap_or_default();
                let mut d = Draft { title: s(&overview, "title").to_string(), notes: s(&details, "notesPlain").to_string(), ..Default::default() };
                d.kind = match s(item, "categoryUuid") {
                    "001" | "005" => ItemKind::Login,
                    "002" => ItemKind::Card,
                    "003" => ItemKind::SecureNote,
                    "004" => ItemKind::Identity,
                    "109" => ItemKind::Wifi,
                    "112" => ItemKind::ApiKey,
                    "114" => ItemKind::SshKey,
                    _ => ItemKind::Other,
                };
                d.favorite = item.get("favIndex").and_then(Value::as_u64).unwrap_or(0) > 0;
                d.tags = overview.get("tags").and_then(Value::as_array).into_iter().flatten().filter_map(|t| t.as_str().map(str::to_string)).collect();
                if s(item, "state") == "archived" {
                    d.tags.push("archived".into());
                }
                let mut urls: Vec<String> = overview.get("urls").and_then(Value::as_array).into_iter().flatten().map(|u| s(u, "url").to_string()).collect();
                if urls.is_empty() && !s(&overview, "url").is_empty() {
                    urls.push(s(&overview, "url").to_string());
                }
                d.urls = urls;
                for f in details.get("loginFields").and_then(Value::as_array).into_iter().flatten() {
                    match s(f, "designation") {
                        "username" => d.field("username", FieldKind::Username, s(f, "value")),
                        "password" => d.field("password", FieldKind::Password, s(f, "value")),
                        _ if s(f, "fieldType") == "P" => d.field(s(f, "name"), FieldKind::Concealed, s(f, "value")),
                        _ => {}
                    }
                }
                if !s(&details, "password").is_empty() {
                    d.field("password", FieldKind::Password, s(&details, "password"));
                }
                for section in details.get("sections").and_then(Value::as_array).into_iter().flatten() {
                    for f in section.get("fields").and_then(Value::as_array).into_iter().flatten() {
                        let label = s(f, "title");
                        let Some(value) = f.get("value").and_then(Value::as_object) else { continue };
                        let Some((kind_name, v)) = value.iter().next() else { continue };
                        match (kind_name.as_str(), v) {
                            ("totp", Value::String(t)) if d.totp.is_none() => d.totp = Some(t.clone()),
                            ("concealed" | "creditCardNumber" | "totp", Value::String(t)) => d.field(label, FieldKind::Concealed, t),
                            ("string" | "creditCardType", Value::String(t)) => d.field(label, FieldKind::Text, t),
                            ("email", Value::Object(e)) => d.field(label, FieldKind::Email, e.get("email_address").and_then(Value::as_str).unwrap_or("")),
                            ("email", Value::String(t)) => d.field(label, FieldKind::Email, t),
                            ("url", Value::String(t)) => d.field(label, FieldKind::Url, t),
                            ("phone", Value::String(t)) => d.field(label, FieldKind::Phone, t),
                            ("monthYear", Value::Number(n)) => {
                                let n = n.as_u64().unwrap_or(0);
                                d.field(label, FieldKind::Text, &format!("{:02}/{}", n % 100, n / 100));
                            }
                            ("date", Value::Number(n)) => {
                                let date = chrono::DateTime::from_timestamp(n.as_i64().unwrap_or(0), 0).map(|t| t.format("%Y-%m-%d").to_string());
                                d.field(label, FieldKind::Date, &date.unwrap_or_default());
                            }
                            (_, Value::String(t)) => d.field(label, FieldKind::Text, t),
                            (other, _) => warnings.push(format!("“{}”: a “{other}” field was not imported", d.title)),
                        }
                    }
                }
                if let Some(b) = d.build(now, &mut warnings) {
                    out.push(b);
                }
            }
        }
    }
    Ok(Parsed { items: out, warnings })
}

// ─── KeePass XML ─────────────────────────────────────────

/// A KeePass entry's strings — key, value, whether KeePass protected it —
/// and the groups it sits in.
type KeePassEntry = (Vec<(String, String, bool)>, Vec<String>);

fn keepass(bytes: &[u8], now: i64) -> Result<Parsed, ExchangeError> {
    use quick_xml::events::Event;
    let text = std::str::from_utf8(bytes).map_err(|_| ExchangeError::Unknown)?;
    let mut reader = quick_xml::Reader::from_str(text);
    reader.config_mut().trim_text(true);

    let mut path: Vec<String> = Vec::new(); // element names, outermost first
    let mut groups: Vec<String> = Vec::new(); // group names, outermost first
    let mut group_named = Vec::new(); // whether each open group has its name yet
    let mut entry: Option<KeePassEntry> = None;
    let (mut key, mut value, mut protected) = (String::new(), String::new(), false);
    let mut out = Vec::new();
    let mut warnings = Vec::new();

    loop {
        let event = reader.read_event().map_err(|e| ExchangeError::Damaged(e.to_string()))?;
        match event {
            Event::Start(e) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).into_owned();
                let in_history = path.iter().any(|p| p == "History");
                match name.as_str() {
                    "Group" => {
                        groups.push(String::new());
                        group_named.push(false);
                    }
                    "Entry" if !in_history => entry = Some((Vec::new(), groups.clone())),
                    "Value" => {
                        protected = e.attributes().flatten().any(|a| a.key.as_ref() == b"ProtectInMemory" && a.value.as_ref() == b"True");
                    }
                    _ => {}
                }
                path.push(name);
            }
            Event::Text(t) => {
                let s = t.unescape().map(|c| c.into_owned()).unwrap_or_default();
                let here = path.last().map(String::as_str);
                let parent = path.len().checked_sub(2).and_then(|i| path.get(i)).map(String::as_str);
                match (parent, here) {
                    (Some("Group"), Some("Name")) => {
                        if let (Some(g), Some(named)) = (groups.last_mut(), group_named.last_mut()) {
                            if !*named {
                                *g = s;
                                *named = true;
                            }
                        }
                    }
                    (Some("String"), Some("Key")) => key = s,
                    (Some("String"), Some("Value")) => value = s,
                    _ => {}
                }
            }
            Event::End(e) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).into_owned();
                path.pop();
                let in_history = path.iter().any(|p| p == "History");
                match name.as_str() {
                    "String" if !in_history => {
                        if let Some((strings, _)) = entry.as_mut() {
                            strings.push((std::mem::take(&mut key), std::mem::take(&mut value), protected));
                        }
                        protected = false;
                    }
                    "Entry" if !in_history => {
                        if let Some((strings, entry_groups)) = entry.take() {
                            // The first group is the database itself, not a folder anyone chose.
                            let folders: Vec<String> = entry_groups.into_iter().skip(1).filter(|g| !g.is_empty()).collect();
                            if folders.iter().any(|g| g == "Recycle Bin" || g == "Thùng rác") {
                                continue;
                            }
                            if let Some(b) = keepass_entry(strings, folders, now, &mut warnings) {
                                out.push(b);
                            }
                        }
                    }
                    "Group" => {
                        groups.pop();
                        group_named.pop();
                    }
                    _ => {}
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    if out.is_empty() && !text.contains("<KeePassFile") {
        return Err(ExchangeError::Unknown);
    }
    Ok(Parsed { items: out, warnings })
}

fn keepass_entry(strings: Vec<(String, String, bool)>, folders: Vec<String>, now: i64, warnings: &mut Vec<String>) -> Option<ItemBody> {
    let mut d = Draft { kind: ItemKind::Login, tags: folders, ..Default::default() };
    for (key, value, protected) in strings {
        match key.as_str() {
            "Title" => d.title = value,
            "UserName" => d.field("username", FieldKind::Username, &value),
            "Password" => d.field("password", FieldKind::Password, &value),
            "URL" => d.urls.push(value),
            "Notes" => d.notes = value,
            "otp" | "TOTP Seed" | "TimeOtp-Secret-Base32" => d.totp = Some(value).filter(|v| !v.is_empty()),
            "TOTP Settings" | "KPEX_PASSKEY_CREDENTIAL_ID" => {}
            other => d.field(other, if protected { FieldKind::Concealed } else { FieldKind::Text }, &value),
        }
    }
    if d.fields.iter().all(|(_, k, _)| *k != FieldKind::Password) && d.urls.is_empty() {
        d.kind = if d.fields.is_empty() { ItemKind::SecureNote } else { ItemKind::Other };
    }
    d.build(now, warnings)
}

// ─── CSV ─────────────────────────────────────────────────

/// RFC 4180: quoted fields, doubled quotes, newlines inside quotes, CRLF.
fn csv_rows(text: &str) -> Vec<Vec<String>> {
    let mut rows = Vec::new();
    let (mut row, mut field, mut quoted) = (Vec::new(), String::new(), false);
    let mut chars = text.trim_start_matches('\u{feff}').chars().peekable();
    while let Some(c) = chars.next() {
        match (c, quoted) {
            ('"', true) if chars.peek() == Some(&'"') => {
                field.push('"');
                chars.next();
            }
            ('"', true) => quoted = false,
            ('"', false) if field.is_empty() => quoted = true,
            (',', false) => row.push(std::mem::take(&mut field)),
            ('\r', false) => {}
            ('\n', false) => {
                row.push(std::mem::take(&mut field));
                rows.push(std::mem::take(&mut row));
            }
            (c, _) => field.push(c),
        }
    }
    if !field.is_empty() || !row.is_empty() {
        row.push(field);
        rows.push(row);
    }
    rows.retain(|r| r.iter().any(|f| !f.trim().is_empty()));
    rows
}

fn csv(bytes: &[u8], now: i64) -> Result<Parsed, ExchangeError> {
    let text = zeroize::Zeroizing::new(String::from_utf8_lossy(bytes).into_owned());
    let rows = csv_rows(&text);
    let (header, body) = rows.split_first().ok_or(ExchangeError::Unknown)?;
    let column = |names: &[&str]| header.iter().position(|h| names.contains(&h.trim().to_ascii_lowercase().as_str()));
    let title = column(&["name", "title", "account"]);
    let url = column(&["url", "login_uri", "website", "web site", "login url"]);
    let user = column(&["username", "login_username", "user name", "login", "email", "user"]);
    let pass = column(&["password", "login_password", "pass"]);
    let notes = column(&["note", "notes", "extra", "comments", "comment"]);
    let otp = column(&["otpauth", "totp", "login_totp", "otp", "one-time password"]);
    let kind = column(&["type"]);
    if pass.is_none() && user.is_none() && notes.is_none() {
        return Err(ExchangeError::Unknown);
    }
    let known: Vec<usize> = [title, url, user, pass, notes, otp, kind].into_iter().flatten().collect();
    let mut warnings = Vec::new();
    let ignored: Vec<&str> = header.iter().enumerate().filter(|(i, h)| !known.contains(i) && !h.trim().is_empty()).map(|(_, h)| h.as_str()).collect();
    if !ignored.is_empty() {
        warnings.push(format!("these columns were not imported: {}", ignored.join(", ")));
    }
    let cell = |row: &Vec<String>, i: Option<usize>| i.and_then(|i| row.get(i)).map(|v| v.trim().to_string()).unwrap_or_default();

    let mut out = Vec::new();
    for row in body {
        let mut d = Draft { title: cell(row, title), notes: cell(row, notes), ..Default::default() };
        d.kind = if cell(row, kind).eq_ignore_ascii_case("note") { ItemKind::SecureNote } else { ItemKind::Login };
        d.field("username", FieldKind::Username, &cell(row, user));
        d.field("password", FieldKind::Password, &cell(row, pass));
        let u = cell(row, url);
        if !u.is_empty() {
            d.urls.push(u);
        }
        d.totp = Some(cell(row, otp)).filter(|t| !t.is_empty());
        if let Some(b) = d.build(now, &mut warnings) {
            out.push(b);
        }
    }
    Ok(Parsed { items: out, warnings })
}

/// Items as CSV, in the column order Chrome, Edge, Firefox and Bitwarden
/// read, with the one-time code as an `otpauth://` link last.
pub fn to_csv(items: &[ItemBody]) -> zeroize::Zeroizing<String> {
    fn quote(v: &str) -> String {
        if v.contains([',', '"', '\n', '\r']) {
            format!("\"{}\"", v.replace('"', "\"\""))
        } else {
            v.to_string()
        }
    }
    let mut out = zeroize::Zeroizing::new(String::from("name,url,username,password,note,totp\n"));
    for item in items.iter().filter(|i| i.trashed_at.is_none()) {
        let find = |kinds: &[FieldKind]| item.fields.iter().find(|f| kinds.contains(&f.kind)).map(|f| f.value.expose().to_string()).unwrap_or_default();
        let username = find(&[FieldKind::Username, FieldKind::Email]);
        let password = find(&[FieldKind::Password, FieldKind::Concealed, FieldKind::Pin]);
        let mut note = item.notes.expose().to_string();
        // Fields a CSV has no column for go into the note rather than nowhere.
        for f in &item.fields {
            let used = (f.kind == FieldKind::Username || f.kind == FieldKind::Email) && f.value.expose() == username
                || f.value.expose() == password;
            if !used && !f.value.is_empty() {
                note.push_str(&format!("\n{}: {}", f.label, f.value.expose()));
            }
        }
        let totp = item.totp.as_ref().map(|t| {
            format!(
                "otpauth://totp/{}?secret={}&algorithm={}&digits={}&period={}",
                urlencoding::encode(&item.title),
                t.secret.expose(),
                format!("{:?}", t.algorithm).to_uppercase(),
                t.digits,
                t.period
            )
        });
        let url = item.urls.first().map(|u| u.url.clone()).unwrap_or_default();
        out.push_str(&[&item.title, &url, &username, &password, note.trim(), totp.as_deref().unwrap_or("")].map(quote).join(","));
        out.push('\n');
    }
    out
}

// ─── .safe-export ────────────────────────────────────────

const EXPORT_MAGIC: &[u8; 4] = b"SFX1";
/// magic, version, m, t, p, salt, then the nonce.
const EXPORT_HEADER_LEN: usize = 4 + 2 + 4 + 4 + 1 + SALT_LEN;

fn export_key(password: &str, salt: &[u8; SALT_LEN], kdf: KdfParams) -> Result<Key, CryptoError> {
    // Argon2id, then the export's own context. No Secret Key: the file must
    // open on a machine that has never seen this Safe.
    let stretched = crypto::derive_auk(password.as_bytes(), salt, kdf, &crypto::SecretKey::from_bytes([0; 16]))?;
    Ok(crypto::subkey(ctx::EXPORT, &stretched))
}

/// Seal every item under a password chosen for this export.
pub fn seal_export(items: &[ItemBody], password: &str, kdf: KdfParams) -> Result<Vec<u8>, CryptoError> {
    let json = zeroize::Zeroizing::new(serde_json::to_vec(items).expect("items serialise"));
    let salt: [u8; SALT_LEN] = crypto::random_bytes()?;
    let nonce: [u8; NONCE_LEN] = crypto::random_bytes()?;
    let mut out = Vec::with_capacity(EXPORT_HEADER_LEN + NONCE_LEN + json.len() + 16);
    out.extend_from_slice(EXPORT_MAGIC);
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&kdf.m_kib.to_le_bytes());
    out.extend_from_slice(&kdf.t.to_le_bytes());
    out.push(kdf.p);
    out.extend_from_slice(&salt);
    let key = export_key(password, &salt, kdf)?;
    let sealed = crypto::seal(&key, &nonce, &out, &json)?;
    out.extend_from_slice(&nonce);
    out.extend_from_slice(&sealed);
    Ok(out)
}

pub fn open_export(bytes: &[u8], password: &str) -> Result<Vec<ItemBody>, ExchangeError> {
    if bytes.len() < EXPORT_HEADER_LEN + NONCE_LEN + 16 || !bytes.starts_with(EXPORT_MAGIC) {
        return Err(ExchangeError::Damaged("not a Safe export".into()));
    }
    let header = &bytes[..EXPORT_HEADER_LEN];
    if u16::from_le_bytes([header[4], header[5]]) != 1 {
        return Err(ExchangeError::Damaged("written by a newer Synabit".into()));
    }
    let kdf = KdfParams {
        m_kib: u32::from_le_bytes(header[6..10].try_into().expect("4 bytes")),
        t: u32::from_le_bytes(header[10..14].try_into().expect("4 bytes")),
        p: header[14],
    };
    // An export is a file anyone may hand you; one that asks for less work
    // than any Safe writes was not written by one.
    if !kdf.meets_floor() {
        return Err(ExchangeError::Damaged("asks for less protection than Synabit writes".into()));
    }
    let salt: [u8; SALT_LEN] = header[15..15 + SALT_LEN].try_into().expect("salt length");
    let nonce: [u8; NONCE_LEN] = bytes[EXPORT_HEADER_LEN..EXPORT_HEADER_LEN + NONCE_LEN].try_into().expect("nonce length");
    let key = export_key(password, &salt, kdf)?;
    let json = crypto::open(&key, &nonce, header, &bytes[EXPORT_HEADER_LEN + NONCE_LEN..]).map_err(|_| ExchangeError::WrongExportPassword)?;
    serde_json::from_slice(&json).map_err(|e| ExchangeError::Damaged(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn value(item: &ItemBody, kind: FieldKind) -> String {
        item.fields.iter().find(|f| f.kind == kind).map(|f| f.value.expose().to_string()).unwrap_or_default()
    }

    #[test]
    fn bitwarden_logins_cards_and_folders() {
        let json = br#"{"encrypted":false,
          "folders":[{"id":"f1","name":"Work"}],
          "items":[
            {"type":1,"name":"GitHub","folderId":"f1","favorite":true,"notes":"n",
             "login":{"username":"anh","password":"hunter2","totp":"JBSWY3DPEHPK3PXP","uris":[{"uri":"https://github.com"}]},
             "fields":[{"name":"PIN","value":"1234","type":1},{"name":"linked","value":null,"type":3}]},
            {"type":3,"name":"Visa","card":{"cardholderName":"LE ANH","number":"4111111111111111","expMonth":"9","expYear":"2029","code":"123"}},
            {"type":2,"name":"Empty note"}
          ]}"#;
        let p = parse(Format::BitwardenJson, json, None, 1).unwrap();
        assert_eq!(p.items.len(), 2, "the empty note is skipped: {:?}", p.warnings);
        let gh = &p.items[0];
        assert_eq!((gh.title.as_str(), gh.favorite, gh.tags.clone()), ("GitHub", true, vec!["Work".to_string()]));
        assert_eq!(value(gh, FieldKind::Password), "hunter2");
        assert_eq!(value(gh, FieldKind::Concealed), "1234");
        assert!(gh.totp.is_some());
        assert_eq!(gh.urls[0].url, "https://github.com");
        let visa = &p.items[1];
        assert_eq!(visa.kind, ItemKind::Card);
        assert_eq!(value(visa, FieldKind::Concealed), "4111111111111111");
        assert!(p.warnings.iter().all(|w| !w.contains("hunter2") && !w.contains("4111")), "a warning carried a value");
    }

    #[test]
    fn an_encrypted_bitwarden_export_is_refused_with_a_reason() {
        let err = parse(Format::BitwardenJson, br#"{"encrypted":true,"data":"..."}"#, None, 1).err().unwrap();
        assert!(matches!(err, ExchangeError::EncryptedBitwarden));
    }

    #[test]
    fn a_1pux_with_sections_totp_and_archived_items() {
        let data = r#"{"accounts":[{"vaults":[{"attrs":{"name":"Private"},"items":[
            {"uuid":"a","categoryUuid":"001","favIndex":1,"state":"active",
             "overview":{"title":"Bank","urls":[{"url":"https://bank.example"}],"tags":["money"]},
             "details":{"loginFields":[{"designation":"username","value":"anh"},{"designation":"password","value":"s3cret"}],
                        "notesPlain":"branch 12",
                        "sections":[{"title":"","fields":[
                          {"title":"one-time password","value":{"totp":"otpauth://totp/Bank?secret=JBSWY3DPEHPK3PXP"}},
                          {"title":"security answer","value":{"concealed":"blue"}},
                          {"title":"expires","value":{"monthYear":202912}}]}]}},
            {"uuid":"b","categoryUuid":"003","state":"archived","overview":{"title":"Old note"},"details":{"notesPlain":"hello"}},
            {"uuid":"c","categoryUuid":"001","trashed":true,"overview":{"title":"Gone"},"details":{}}
        ]}]}]}"#;
        let mut zip_bytes = Vec::new();
        {
            let mut w = zip::ZipWriter::new(std::io::Cursor::new(&mut zip_bytes));
            w.start_file("export.data", zip::write::SimpleFileOptions::default()).unwrap();
            std::io::Write::write_all(&mut w, data.as_bytes()).unwrap();
            w.finish().unwrap();
        }
        assert_eq!(detect("1PasswordExport.1pux", &zip_bytes), Some(Format::OnePux));
        let p = parse(Format::OnePux, &zip_bytes, None, 1).unwrap();
        assert_eq!(p.items.len(), 2, "the trashed item is not imported");
        let bank = &p.items[0];
        assert_eq!(value(bank, FieldKind::Password), "s3cret");
        assert_eq!(value(bank, FieldKind::Concealed), "blue");
        assert!(bank.totp.is_some());
        assert!(bank.favorite);
        assert!(bank.fields.iter().any(|f| f.value.expose() == "12/2029"));
        assert!(p.items[1].tags.contains(&"archived".to_string()));
    }

    #[test]
    fn keepass_groups_become_tags_and_history_and_recycle_bin_are_skipped() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<KeePassFile><Root><Group><Name>Database</Name>
  <Group><Name>Work</Name>
    <Entry>
      <String><Key>Title</Key><Value>Jira</Value></String>
      <String><Key>UserName</Key><Value>anh</Value></String>
      <String><Key>Password</Key><Value ProtectInMemory="True">new &amp; improved</Value></String>
      <String><Key>URL</Key><Value>https://jira.example</Value></String>
      <String><Key>Recovery</Key><Value ProtectInMemory="True">abc-def</Value></String>
      <String><Key>otp</Key><Value>otpauth://totp/Jira?secret=JBSWY3DPEHPK3PXP</Value></String>
      <History><Entry>
        <String><Key>Title</Key><Value>Jira</Value></String>
        <String><Key>Password</Key><Value>old-password</Value></String>
      </Entry></History>
    </Entry>
  </Group>
  <Group><Name>Recycle Bin</Name>
    <Entry><String><Key>Title</Key><Value>Deleted</Value></String><String><Key>Password</Key><Value>x</Value></String></Entry>
  </Group>
</Group></Root></KeePassFile>"#;
        let p = parse(Format::KeePassXml, xml.as_bytes(), None, 1).unwrap();
        assert_eq!(p.items.len(), 1, "{:?}", p.items.iter().map(|i| &i.title).collect::<Vec<_>>());
        let jira = &p.items[0];
        assert_eq!(jira.tags, vec!["Work".to_string()], "the database group is not a tag");
        assert_eq!(value(jira, FieldKind::Password), "new & improved");
        assert_eq!(value(jira, FieldKind::Concealed), "abc-def");
        assert!(jira.totp.is_some());
        assert!(!jira.fields.iter().any(|f| f.value.expose() == "old-password"), "an entry's history was imported as an item's field");
    }

    #[test]
    fn csv_from_chrome_firefox_and_safari() {
        let chrome = "name,url,username,password,note\nGitHub,https://github.com,anh,\"pa,ss\"\"word\",\"line1\nline2\"\n";
        let p = parse(Format::Csv, chrome.as_bytes(), None, 1).unwrap();
        assert_eq!(p.items.len(), 1);
        assert_eq!(value(&p.items[0], FieldKind::Password), "pa,ss\"word");
        assert_eq!(p.items[0].notes.expose(), "line1\nline2");

        // Firefox has no title column: the host stands in.
        let firefox = "\u{feff}\"url\",\"username\",\"password\",\"httpRealm\",\"formActionOrigin\",\"guid\"\r\n\"https://www.example.com\",\"anh\",\"x\",,\"\",\"{1}\"\r\n";
        let p = parse(Format::Csv, firefox.as_bytes(), None, 1).unwrap();
        assert_eq!(p.items[0].title, "example.com");
        assert!(p.warnings.iter().any(|w| w.contains("httpRealm")));

        let safari = "Title,URL,Username,Password,Notes,OTPAuth\nBank,bank.example,anh,pw,,otpauth://totp/Bank?secret=JBSWY3DPEHPK3PXP\n";
        let p = parse(Format::Csv, safari.as_bytes(), None, 1).unwrap();
        assert!(p.items[0].totp.is_some());
    }

    #[test]
    fn a_csv_without_password_columns_is_not_guessed_at() {
        assert!(parse(Format::Csv, b"a,b,c\n1,2,3\n", None, 1).is_err());
    }

    #[test]
    fn a_safe_export_round_trips_everything_and_needs_its_password() {
        let p = parse(Format::Csv, b"name,url,username,password\nA,a.example,u,p1\nB,,v,p2\n", None, 1).unwrap();
        let mut items = p.items;
        items[0].totp = Some(super::super::totp::Totp::parse("JBSWY3DPEHPK3PXP").unwrap());
        let bytes = seal_export(&items, "export password", KdfParams::FLOOR).unwrap();
        assert_eq!(detect("anything.bin", &bytes), Some(Format::SafeExport));
        assert!(!bytes.windows(2).any(|w| w == b"p1"), "plaintext in the export");

        let back = open_export(&bytes, "export password").unwrap();
        assert_eq!(back, items);
        assert!(matches!(open_export(&bytes, "wrong password!"), Err(ExchangeError::WrongExportPassword)));

        let mut altered = bytes.clone();
        altered[8] ^= 1; // the memory cost, in the header
        assert!(open_export(&altered, "export password").is_err());
    }

    #[test]
    fn csv_export_quotes_what_needs_quoting_and_reads_back() {
        let p = parse(Format::Csv, "name,url,username,password,note\n\"Co, Ltd\",x.example,anh,\"a\"\"b\",hi\n".as_bytes(), None, 1).unwrap();
        let out = to_csv(&p.items);
        let back = parse(Format::Csv, out.as_bytes(), None, 1).unwrap();
        assert_eq!(back.items[0].title, "Co, Ltd");
        assert_eq!(value(&back.items[0], FieldKind::Password), "a\"b");
    }

    #[test]
    fn detection_goes_by_name_then_contents() {
        assert_eq!(detect("export.csv", b"x"), Some(Format::Csv));
        assert_eq!(detect("bitwarden_export.json", b"{}"), Some(Format::BitwardenJson));
        assert_eq!(detect("db.xml", b"<x/>"), Some(Format::KeePassXml));
        assert_eq!(detect("mystery", b"<KeePassFile>"), Some(Format::KeePassXml));
        assert_eq!(detect("mystery.bin", b"nothing"), None);
    }

    #[test]
    fn junk_never_panics() {
        for format in [Format::OnePux, Format::BitwardenJson, Format::KeePassXml, Format::Csv, Format::SafeExport] {
            for bytes in [&b""[..], b"{", b"<KeePassFile><Root><Group>", b"PK\x03\x04garbage", b"\"unterminated", b"SFX1"] {
                let _ = parse(format, bytes, Some("pw"), 1);
            }
        }
    }
}
