//! Reading and writing vCard, so a contact list is not a place people go to die.
//!
//! Three versions arrive in practice and all three are accepted: 2.1 from
//! older phones and older Outlook, 3.0 from Apple and most of the web, 4.0
//! from anything current. The differences that matter are narrow — 2.1 writes
//! its types as bare parameters and may encode values as quoted-printable, 3.0
//! writes photos as `ENCODING=b`, 4.0 writes them as a `data:` URI — so rather
//! than three parsers there is one that is liberal about all three. What is
//! written back out is always 4.0.
//!
//! # Nothing is dropped in silence
//!
//! A property this module does not recognise does not vanish; it becomes a
//! detail on the person, labelled with the name it had in the file. That
//! covers the `X-` properties every vendor invents — `X-ABDATE`, `X-SKYPE`,
//! `X-GENDER` — and anything from a future version. A contact list that loses
//! a field on the way in is worse than one that shows an oddly-named row.

use base64::Engine;
use serde_json::{json, Map, Value};

use crate::utils::contentline::{
    escape, fold, param, params_all, prop, split_line, split_values, unescape, unfold,
};

/// A picture that came with a card, still in whatever format it was sent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Photo {
    pub bytes: Vec<u8>,
    /// A file extension without the dot, worked out from the bytes.
    pub extension: String,
}

/// One person, in the shape [`crate::commands::nodes::write_node_file`] wants.
#[derive(Debug, Clone, PartialEq)]
pub struct ImportedContact {
    pub title: String,
    pub properties: Map<String, Value>,
    /// The `NOTE`, which becomes the person's own notes.
    pub body: String,
    pub photo: Option<Photo>,
}

/// One person on the way out.
pub struct ExportContact<'a> {
    pub title: &'a str,
    pub properties: &'a Value,
    pub body: &'a str,
    pub photo: Option<&'a Photo>,
}

// ─── Quoted-printable ───────────────────────────────────────

/// Undo `=C3=A9` back into `é`.
///
/// Only vCard 2.1 writes this, and only for values with non-ASCII in them —
/// which for a Vietnamese address book is nearly every line. Decoded as bytes
/// and then read as UTF-8 at the end, because one character is several escapes
/// and decoding them one at a time would produce fragments.
fn decode_quoted_printable(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'=' && i + 2 < bytes.len() {
            let hex = &value[i + 1..i + 3];
            if let Ok(byte) = u8::from_str_radix(hex, 16) {
                out.push(byte);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    // A file that lied about its charset is still worth importing; the
    // replacement character is a visible problem, a dropped contact is not.
    String::from_utf8_lossy(&out).into_owned()
}

fn is_quoted_printable(params: &[(String, String)]) -> bool {
    params_all(params, "ENCODING")
        .iter()
        .any(|e| e.eq_ignore_ascii_case("QUOTED-PRINTABLE"))
        || params_all(params, "TYPE")
            .iter()
            .any(|e| e.eq_ignore_ascii_case("QUOTED-PRINTABLE"))
}

/// Join the lines of a card, honouring both continuation rules.
///
/// The ordinary one is a leading space, which [`unfold`] handles. Quoted-
/// printable brings a second: a value ending in `=` continues on the next
/// line with no leading space at all. A card using it — and 2.1 cards from
/// older phones nearly always do — comes apart under the first rule alone.
fn unfold_card(text: &str) -> Vec<String> {
    let lines = unfold(text);
    let mut out: Vec<String> = Vec::with_capacity(lines.len());
    let mut pending: Option<String> = None;

    for line in lines {
        match pending.take() {
            Some(mut open) => {
                open.push_str(line.trim_start());
                if open.ends_with('=') {
                    open.pop();
                    pending = Some(open);
                } else {
                    out.push(open);
                }
            }
            None => {
                let qp = line.to_ascii_uppercase().contains("QUOTED-PRINTABLE");
                if qp && line.ends_with('=') {
                    let mut open = line;
                    open.pop();
                    pending = Some(open);
                } else {
                    out.push(line);
                }
            }
        }
    }
    if let Some(open) = pending {
        out.push(open);
    }
    out
}

// ─── Labels ─────────────────────────────────────────────────

/// What to call a phone number, from the types the card gave it.
///
/// The word "Phone" is kept in every label on purpose. The People screen
/// copies the first detail whose label contains "phone" into a flat field the
/// sidebar reads, so a number labelled plain "Mobile" would import fine and
/// then not show up beside the person's name.
fn phone_label(types: &[&str]) -> String {
    let has = |t: &str| types.iter().any(|x| x.eq_ignore_ascii_case(t));
    if has("CELL") || has("MOBILE") {
        "Mobile Phone"
    } else if has("FAX") {
        "Fax"
    } else if has("WORK") {
        "Work Phone"
    } else if has("HOME") {
        "Home Phone"
    } else {
        "Phone"
    }
    .to_string()
}

fn email_label(types: &[&str]) -> String {
    let has = |t: &str| types.iter().any(|x| x.eq_ignore_ascii_case(t));
    if has("WORK") {
        "Work Email"
    } else if has("HOME") || has("PERSONAL") {
        "Personal Email"
    } else {
        "Email"
    }
    .to_string()
}

/// Name a link by where it points.
///
/// The People screen colours LinkedIn, Twitter, GitHub and Website by label,
/// so recognising the host here is what makes an imported link look like one
/// typed in by hand.
fn url_label(value: &str, types: &[&str]) -> String {
    let v = value.to_ascii_lowercase();
    for (host, label) in [
        ("linkedin.", "LinkedIn"),
        ("github.", "GitHub"),
        ("twitter.", "Twitter"),
        ("x.com", "Twitter"),
        ("facebook.", "Facebook"),
        ("instagram.", "Instagram"),
    ] {
        if v.contains(host) {
            return label.to_string();
        }
    }
    if types.iter().any(|t| t.eq_ignore_ascii_case("WORK")) {
        return "Work Website".to_string();
    }
    "Website".to_string()
}

/// `X-ABC-DEF` → `Abc Def`, for a property nobody here has heard of.
fn humanize(name: &str) -> String {
    name.trim_start_matches("X-")
        .split(['-', '_'])
        .filter(|s| !s.is_empty())
        .map(|word| {
            let lower = word.to_ascii_lowercase();
            let mut chars = lower.chars();
            match chars.next() {
                Some(first) => first.to_ascii_uppercase().to_string() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// The same rules the contact form applies when somebody types a label.
fn infer_type(label: &str, value: &str) -> &'static str {
    let l = label.to_ascii_lowercase();
    if l.contains("email") || l.contains("mail") {
        return "email";
    }
    if l.contains("phone") || l.contains("tel") || l.contains("mobile") || l.contains("fax") {
        return "phone";
    }
    if l.contains("linkedin")
        || l.contains("twitter")
        || l.contains("github")
        || l.contains("website")
        || l.contains("url")
        || value.starts_with("http")
    {
        return "url";
    }
    "text"
}

// ─── Dates ──────────────────────────────────────────────────

/// A vCard date in any of its shapes, as the vault writes them.
///
/// `19940302`, `1994-03-02`, `1994-03-02T00:00:00Z` all mean the same day.
/// `--0302` is vCard 4's way of saying "this day, year unknown", which the
/// vault writes as `MM-DD` and the reminder engine reads.
fn parse_date(raw: &str) -> Option<String> {
    let raw = raw.trim();
    let raw = raw.split(['T', ' ']).next()?.trim_end_matches('Z');

    if let Some(rest) = raw.strip_prefix("--") {
        let digits: String = rest.chars().filter(|c| c.is_ascii_digit()).collect();
        if digits.len() == 4 {
            return Some(format!("{}-{}", &digits[..2], &digits[2..]));
        }
        return None;
    }

    let digits: String = raw.chars().filter(|c| c.is_ascii_digit()).collect();
    match digits.len() {
        8 => Some(format!(
            "{}-{}-{}",
            &digits[..4],
            &digits[4..6],
            &digits[6..8]
        )),
        // A year on its own says nothing about the day, and an anniversary
        // with no day cannot be announced.
        _ => None,
    }
}

// ─── Reading ────────────────────────────────────────────────

/// Every card in the file.
///
/// Anything that is not a card is skipped rather than refused: files arrive
/// concatenated, with mail headers stuck to the front, or truncated halfway.
/// What can be read is read.
pub fn import(text: &str) -> Vec<ImportedContact> {
    let mut out = Vec::new();
    let mut current: Option<CardBuilder> = None;

    for line in unfold_card(text) {
        let trimmed = line.trim();
        if trimmed.eq_ignore_ascii_case("BEGIN:VCARD") {
            current = Some(CardBuilder::default());
            continue;
        }
        if trimmed.eq_ignore_ascii_case("END:VCARD") {
            if let Some(card) = current.take() {
                if let Some(contact) = card.finish() {
                    out.push(contact);
                }
            }
            continue;
        }
        if let (Some(card), Some(parsed)) = (current.as_mut(), split_line(trimmed)) {
            card.take_line(parsed);
        }
    }

    // A file whose last card was never closed is still worth what it holds.
    if let Some(card) = current {
        if let Some(contact) = card.finish() {
            out.push(contact);
        }
    }

    out
}

#[derive(Default)]
struct CardBuilder {
    formatted_name: String,
    structured_name: Option<Vec<String>>,
    nickname: String,
    birthday: Option<String>,
    important_dates: Vec<Value>,
    tags: Vec<String>,
    details: Vec<Value>,
    org: Vec<String>,
    title_role: String,
    relationship: String,
    cadence: String,
    body: String,
    photo: Option<Photo>,
}

impl CardBuilder {
    fn detail(&mut self, label: String, value: String) {
        if value.trim().is_empty() {
            return;
        }
        let kind = infer_type(&label, &value);
        self.details.push(json!({
            "label": label,
            "value": value,
            "type": kind,
        }));
    }

    fn take_line(&mut self, (name, params, raw): (String, Vec<(String, String)>, String)) {
        // Decoded before anything else looks at it, or a quoted-printable
        // address is read as the literal `=C4=90...` it was written as.
        let raw = if is_quoted_printable(&params) {
            decode_quoted_printable(&raw)
        } else {
            raw
        };
        let types = params_all(&params, "TYPE");
        let text = || unescape(&raw);

        match name.as_str() {
            "VERSION" | "PRODID" | "UID" | "REV" | "SOURCE" | "KIND" | "PROFILE" | "CLASS"
            | "MAILER" | "SORT-STRING" | "X-ABUID" => {}

            "FN" => self.formatted_name = text(),
            "N" => self.structured_name = Some(split_values(&raw, ';')),
            "NICKNAME" => self.nickname = split_values(&raw, ',').join(", "),

            "TEL" => {
                let label = phone_label(&types);
                self.detail(label, text());
            }
            "EMAIL" => {
                let label = email_label(&types);
                self.detail(label, text());
            }
            "URL" | "X-SOCIALPROFILE" => {
                let value = text();
                let label = url_label(&value, &types);
                self.detail(label, value);
            }
            "IMPP" => {
                let value = text();
                // `skype:someone` — the scheme is the better label.
                let label = value
                    .split_once(':')
                    .map(|(scheme, _)| humanize(scheme))
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| "Messaging".to_string());
                self.detail(label, value);
            }

            "ADR" => {
                let parts = split_values(&raw, ';');
                // po box; extended; street; locality; region; post code; country
                let joined = parts
                    .iter()
                    .map(|s| s.trim())
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<_>>()
                    .join(", ");
                let label = if types.iter().any(|t| t.eq_ignore_ascii_case("WORK")) {
                    "Work Address"
                } else if types.iter().any(|t| t.eq_ignore_ascii_case("HOME")) {
                    "Home Address"
                } else {
                    "Address"
                };
                self.detail(label.to_string(), joined);
            }

            "ORG" => {
                self.org = split_values(&raw, ';')
                    .into_iter()
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
            }
            "TITLE" | "ROLE" => {
                if self.title_role.is_empty() {
                    self.title_role = text();
                }
            }

            "BDAY" => self.birthday = parse_date(&raw),
            "ANNIVERSARY" | "X-ANNIVERSARY" => {
                if let Some(date) = parse_date(&raw) {
                    self.important_dates
                        .push(json!({ "label": "Anniversary", "date": date }));
                }
            }

            "CATEGORIES" => {
                self.tags.extend(
                    split_values(&raw, ',')
                        .into_iter()
                        .map(|t| t.trim().to_ascii_lowercase())
                        .filter(|t| !t.is_empty()),
                );
            }

            "NOTE" => self.body = text(),

            // What this app writes for the things no standard property
            // carries. Read back by name so a card that left here comes home
            // as the person who left, not as a row called "Synabit Detail".
            "X-SYNABIT-RELATIONSHIP" => self.relationship = text(),
            "X-SYNABIT-CONTACT-FREQUENCY" => self.cadence = text(),
            "X-SYNABIT-DETAIL" => {
                let label = param(&params, "LABEL")
                    .map(unescape)
                    .unwrap_or_else(|| "Detail".to_string());
                self.detail(label, text());
            }
            "X-SYNABIT-DATE" => {
                if let (Some(label), Some(date)) = (param(&params, "LABEL"), parse_date(&raw)) {
                    self.important_dates
                        .push(json!({ "label": unescape(label), "date": date }));
                }
            }

            "PHOTO" | "LOGO" => self.photo = decode_photo(&raw, &params),

            // Everything else keeps its name and becomes a row on the person.
            other => {
                let label = humanize(other);
                if !label.is_empty() {
                    self.detail(label, text());
                }
            }
        }
    }

    fn finish(self) -> Option<ImportedContact> {
        let title = self.display_title()?;

        let mut properties = Map::new();
        properties.insert("display_name".into(), json!("fullname"));

        if !self.nickname.is_empty() {
            properties.insert("nickname".into(), json!(self.nickname));
        }
        if let Some(bday) = &self.birthday {
            properties.insert("birthday".into(), json!(bday));
        }
        if !self.important_dates.is_empty() {
            properties.insert("important_dates".into(), json!(self.important_dates));
        }
        if !self.tags.is_empty() {
            let mut tags = self.tags.clone();
            tags.dedup();
            properties.insert("tags".into(), json!(tags));
        }

        // A card describes where somebody works now, so the job it names is
        // the current one. Anything earlier is history the card does not hold.
        if !self.org.is_empty() || !self.title_role.is_empty() {
            properties.insert(
                "experiences".into(),
                json!([{
                    "company": self.org.join(" — "),
                    "role": self.title_role,
                    "start": "",
                    "end": "",
                    "current": true,
                }]),
            );
        }

        if !self.relationship.is_empty() {
            // A list, so a relationship whose name contains a comma stays one
            // relationship. The old shape joined them into a string and split
            // it again to read, which lost exactly those.
            let relationships: Vec<&str> = self
                .relationship
                .split(',')
                .map(str::trim)
                .filter(|r| !r.is_empty())
                .collect();
            properties.insert("relationship_type".into(), json!(relationships));
        }
        if !self.cadence.is_empty() {
            properties.insert("contact_frequency".into(), json!(self.cadence));
        }
        if !self.details.is_empty() {
            properties.insert("details".into(), json!(self.details.clone()));
        }

        // The flat copies the sidebar and the search index read.
        let first_of = |needle: &str| -> Option<String> {
            self.details.iter().find_map(|d| {
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
        if !self.org.is_empty() {
            properties.insert("company".into(), json!(self.org[0]));
        }

        Some(ImportedContact {
            title,
            properties,
            body: self.body,
            photo: self.photo,
        })
    }

    /// What to call this person.
    ///
    /// `FN` is the display name and wins. Without it the parts of `N` are put
    /// back together in reading order — a card with only `N:Nguyễn;An` is
    /// common from older exporters, and refusing it would drop the contact.
    fn display_title(&self) -> Option<String> {
        let fn_trimmed = self.formatted_name.trim();
        if !fn_trimmed.is_empty() {
            return Some(fn_trimmed.to_string());
        }

        let parts = self.structured_name.as_ref()?;
        let get = |i: usize| parts.get(i).map(String::as_str).unwrap_or("").trim();
        // family; given; additional; prefix; suffix
        let assembled = [get(3), get(1), get(2), get(0), get(4)]
            .iter()
            .filter(|s| !s.is_empty())
            .cloned()
            .collect::<Vec<_>>()
            .join(" ");

        (!assembled.is_empty()).then_some(assembled)
    }
}

/// Turn a `PHOTO` value into bytes, whichever way it was written.
fn decode_photo(raw: &str, params: &[(String, String)]) -> Option<Photo> {
    let raw = raw.trim();

    // vCard 4.0: a data URI. Anything else is a link to somewhere else, and
    // fetching it would mean this import reaching out to the network.
    let encoded = if let Some(rest) = raw.strip_prefix("data:") {
        rest.split_once(";base64,").map(|(_, data)| data)?
    } else if raw.starts_with("http://") || raw.starts_with("https://") {
        return None;
    } else {
        // 2.1 and 3.0: `PHOTO;ENCODING=b` or `ENCODING=BASE64`, value inline.
        let encoding = param(params, "ENCODING").unwrap_or("");
        if !(encoding.eq_ignore_ascii_case("b")
            || encoding.eq_ignore_ascii_case("base64")
            || params_all(params, "TYPE")
                .iter()
                .any(|t| t.eq_ignore_ascii_case("BASE64")))
        {
            return None;
        }
        raw
    };

    let cleaned: String = encoded.chars().filter(|c| !c.is_whitespace()).collect();
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(&cleaned)
        .ok()?;
    if bytes.is_empty() {
        return None;
    }

    // Read from the bytes rather than believing the `TYPE` parameter, which
    // is routinely wrong — plenty of exporters label every photo JPEG.
    let extension = infer::get(&bytes)
        .filter(|kind| kind.mime_type().starts_with("image/"))
        .map(|kind| kind.extension().to_string())?;

    Some(Photo { bytes, extension })
}

// ─── Writing ────────────────────────────────────────────────

/// Every contact as one vCard 4.0 file.
pub fn export(contacts: &[ExportContact]) -> String {
    let mut out = String::new();
    for contact in contacts {
        write_card(contact, &mut out);
    }
    out
}

fn write_card(contact: &ExportContact, out: &mut String) {
    out.push_str("BEGIN:VCARD\r\n");
    out.push_str("VERSION:4.0\r\n");
    out.push_str("PRODID:-//Synabit//People//EN\r\n");

    prop("FN", contact.title, out);
    // `N` is required by every version, and a reader that finds none may drop
    // the card. The whole name goes in the family slot: splitting a name on
    // spaces guesses wrong in most of the world.
    fold(&format!("N:{};;;;", escape(contact.title)), out);

    let props = contact.properties;
    let str_of = |key: &str| props.get(key).and_then(Value::as_str).unwrap_or("").trim();

    if !str_of("nickname").is_empty() {
        prop("NICKNAME", str_of("nickname"), out);
    }

    for detail in props
        .get("details")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[])
    {
        write_detail(detail, out);
    }

    if let Some(job) = props
        .get("experiences")
        .and_then(Value::as_array)
        .and_then(|jobs| jobs.iter().find(|j| j.get("current") == Some(&json!(true))).or(jobs.first()))
    {
        let company = job.get("company").and_then(Value::as_str).unwrap_or("");
        let role = job.get("role").and_then(Value::as_str).unwrap_or("");
        prop("ORG", company, out);
        prop("TITLE", role, out);
    }

    if let Some(bday) = props.get("birthday").and_then(Value::as_str) {
        if let Some(written) = write_date(bday) {
            fold(&format!("BDAY:{}", written), out);
        }
    }

    for date in props
        .get("important_dates")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[])
    {
        let label = date.get("label").and_then(Value::as_str).unwrap_or("");
        let value = date.get("date").and_then(Value::as_str).unwrap_or("");
        let Some(written) = write_date(value) else { continue };
        if label.eq_ignore_ascii_case("anniversary") {
            fold(&format!("ANNIVERSARY:{}", written), out);
        } else {
            // No standard property fits, so it keeps its own name and comes
            // back as the same labelled date it went out as.
            fold(
                &format!("X-SYNABIT-DATE;LABEL={}:{}", escape(label), written),
                out,
            );
        }
    }

    let tags: Vec<&str> = props
        .get("tags")
        .and_then(Value::as_array)
        .map(|t| t.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    if !tags.is_empty() {
        fold(
            &format!(
                "CATEGORIES:{}",
                tags.iter().map(|t| escape(t)).collect::<Vec<_>>().join(",")
            ),
            out,
        );
    }

    // The relationship is what makes this a person in somebody's life rather
    // than a row in a directory, and no standard property carries it.
    // Read from either shape: a vault written before relationships became a
    // list still holds a string, and an export must not silently drop it.
    let relationship = match props.get("relationship_type") {
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(Value::as_str)
            .map(str::trim)
            .filter(|r| !r.is_empty())
            .collect::<Vec<_>>()
            .join(", "),
        Some(Value::String(raw)) => raw.trim().to_string(),
        _ => String::new(),
    };
    if !relationship.is_empty() {
        prop("X-SYNABIT-RELATIONSHIP", &relationship, out);
    }
    let cadence = str_of("contact_frequency");
    if !cadence.is_empty() {
        prop("X-SYNABIT-CONTACT-FREQUENCY", cadence, out);
    }

    if !contact.body.trim().is_empty() {
        prop("NOTE", contact.body.trim(), out);
    }

    if let Some(photo) = contact.photo {
        let encoded = base64::engine::general_purpose::STANDARD.encode(&photo.bytes);
        let mime = match photo.extension.as_str() {
            "png" => "image/png",
            "gif" => "image/gif",
            "webp" => "image/webp",
            _ => "image/jpeg",
        };
        fold(&format!("PHOTO:data:{};base64,{}", mime, encoded), out);
    }

    out.push_str("END:VCARD\r\n");
}

fn write_detail(detail: &Value, out: &mut String) {
    let label = detail.get("label").and_then(Value::as_str).unwrap_or("");
    let value = detail.get("value").and_then(Value::as_str).unwrap_or("");
    if value.trim().is_empty() {
        return;
    }
    let kind = detail.get("type").and_then(Value::as_str).unwrap_or("text");
    let lower = label.to_ascii_lowercase();

    let line = match kind {
        "email" => {
            let t = if lower.contains("work") { "work" } else { "home" };
            format!("EMAIL;TYPE={}:{}", t, escape(value))
        }
        "phone" => {
            let t = if lower.contains("mobile") || lower.contains("cell") {
                "cell"
            } else if lower.contains("fax") {
                "fax"
            } else if lower.contains("work") {
                "work"
            } else {
                "home"
            };
            format!("TEL;TYPE={}:{}", t, escape(value))
        }
        "url" => format!("URL:{}", escape(value)),
        _ if lower.contains("address") => {
            let t = if lower.contains("work") { "work" } else { "home" };
            // Everything in the street slot: the vault holds an address as one
            // line, and inventing a split into city and postcode would put
            // words in fields they do not belong to.
            format!("ADR;TYPE={}:;;{};;;;", t, escape(value))
        }
        _ => format!(
            "X-SYNABIT-DETAIL;LABEL={}:{}",
            escape(label),
            escape(value)
        ),
    };
    fold(&line, out);
}

/// `1994-03-02` → `19940302`, `03-02` → `--0302`.
fn write_date(value: &str) -> Option<String> {
    let digits: String = value.chars().filter(|c| c.is_ascii_digit()).collect();
    match digits.len() {
        8 => Some(digits),
        4 => Some(format!("--{}", digits)),
        _ => None,
    }
}

#[cfg(test)]
mod tests;
