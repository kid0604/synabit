//! Telling "this is the same person" from "this is somebody new".
//!
//! Importing the same file twice is the ordinary case, not the odd one:
//! somebody exports from their phone, imports, exports again a month later
//! after adding three contacts, and imports that. Without this, the second
//! import doubles the address book.
//!
//! # What counts as the same person
//!
//! An email address or a phone number, because those are chosen to be unique
//! and are how the rest of the world identifies somebody. A name is not
//! enough on its own — there are a great many people called Nguyễn Văn An —
//! so a name match is reported as a *possible* duplicate for someone to look
//! at, never merged on its own.

use serde_json::{json, Map, Value};

use super::vcard::ImportedContact;

/// Why two records look like the same person.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "on", content = "value", rename_all = "snake_case")]
pub enum Reason {
    Email(String),
    Phone(String),
    /// Same name, nothing else in common. Somebody has to decide.
    Name(String),
}

impl Reason {
    /// Whether this is strong enough to merge without being asked.
    pub fn is_certain(&self) -> bool {
        !matches!(self, Reason::Name(_))
    }
}

/// One incoming contact, and who it already looks like.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Duplicate {
    /// Position in the incoming list.
    pub incoming: usize,
    /// The vault path of the person already there, or `None` when the match
    /// is against another row of the same import.
    pub existing_id: Option<String>,
    /// Position in the incoming list, when two rows of one file are the same
    /// person.
    pub existing_incoming: Option<usize>,
    pub reason: Reason,
}

// ─── Keys ───────────────────────────────────────────────────

/// An email address, in the form two spellings of it agree on.
pub fn email_key(raw: &str) -> Option<String> {
    let value = raw.trim().to_ascii_lowercase();
    let value = value.strip_prefix("mailto:").unwrap_or(&value).trim();
    // The @ is the whole test: anything without one is not an address, and
    // matching on it would join unrelated people.
    let (user, domain) = value.split_once('@')?;
    if user.is_empty() || !domain.contains('.') {
        return None;
    }
    Some(format!("{}@{}", user, domain))
}

/// A phone number, in the form two spellings of it agree on.
///
/// Full E.164 needs to know which country a bare number belongs to, and a
/// contact list rarely says. What it does instead is compare the last nine
/// digits — the national number, without the trunk `0` or the country code —
/// which is what makes `+84 90 123 4567`, `0901234567` and `090 123 4567` the
/// same number. Two different countries can collide on nine digits; inside
/// one person's address book that has not been worth the weight of a full
/// phone-number library.
pub fn phone_key(raw: &str) -> Option<String> {
    let digits: String = raw.chars().filter(char::is_ascii_digit).collect();
    // Short enough to be an extension or a service code, which several people
    // can share without being the same person.
    if digits.len() < 8 {
        return None;
    }
    let tail = &digits[digits.len().saturating_sub(9)..];
    Some(tail.to_string())
}

/// A name, in the form two spellings of it agree on.
fn name_key(raw: &str) -> Option<String> {
    let key = raw
        .split_whitespace()
        .map(|w| w.to_lowercase())
        .collect::<Vec<_>>()
        .join(" ");
    (!key.is_empty()).then_some(key)
}

/// Every email and phone on a person, whichever shape they are stored in.
fn contact_keys(properties: &Value) -> Vec<(String, Reason)> {
    let mut keys = Vec::new();

    let mut take = |label: &str, value: &str| {
        let label = label.to_ascii_lowercase();
        if label.contains("email") || label.contains("mail") {
            if let Some(key) = email_key(value) {
                keys.push((format!("e:{}", key), Reason::Email(key)));
            }
        } else if label.contains("phone") || label.contains("tel") || label.contains("mobile") {
            if let Some(key) = phone_key(value) {
                keys.push((format!("p:{}", key), Reason::Phone(key)));
            }
        }
    };

    if let Some(details) = properties.get("details").and_then(Value::as_array) {
        for detail in details {
            let label = detail.get("label").and_then(Value::as_str).unwrap_or("");
            let value = detail.get("value").and_then(Value::as_str).unwrap_or("");
            take(label, value);
        }
    }
    // The flat copies, for people written before details existed.
    for (key, label) in [("email", "email"), ("phone", "phone")] {
        if let Some(value) = properties.get(key).and_then(Value::as_str) {
            take(label, value);
        }
    }

    keys
}

// ─── Matching ───────────────────────────────────────────────

/// One person already in the vault, as far as matching is concerned.
pub struct Existing<'a> {
    pub id: &'a str,
    pub title: &'a str,
    pub properties: &'a Value,
}

/// Which incoming contacts already exist, and why.
///
/// Rows of the incoming file are matched against each other as well as
/// against the vault: a file that lists somebody twice should not create them
/// twice, and plenty do.
pub fn find_duplicates(incoming: &[ImportedContact], existing: &[Existing]) -> Vec<Duplicate> {
    let mut by_key: Vec<(String, &str)> = Vec::new();
    let mut names: Vec<(String, &str)> = Vec::new();
    for person in existing {
        for (key, _) in contact_keys(person.properties) {
            by_key.push((key, person.id));
        }
        if let Some(key) = name_key(person.title) {
            names.push((key, person.id));
        }
    }

    // Rows already seen in this file, so the second mention of somebody is
    // matched against the first rather than added beside it.
    let mut seen_keys: Vec<(String, usize)> = Vec::new();
    let mut seen_names: Vec<(String, usize)> = Vec::new();

    let mut out = Vec::new();
    for (i, contact) in incoming.iter().enumerate() {
        let properties = Value::Object(contact.properties.clone());
        let keys = contact_keys(&properties);

        let found = keys.iter().find_map(|(key, reason)| {
            if let Some((_, id)) = by_key.iter().find(|(k, _)| k == key) {
                return Some(Duplicate {
                    incoming: i,
                    existing_id: Some((*id).to_string()),
                    existing_incoming: None,
                    reason: reason.clone(),
                });
            }
            seen_keys
                .iter()
                .find(|(k, _)| k == key)
                .map(|(_, earlier)| Duplicate {
                    incoming: i,
                    existing_id: None,
                    existing_incoming: Some(*earlier),
                    reason: reason.clone(),
                })
        });

        // Only when nothing stronger was found: a name is a question, not an
        // answer, and asking about somebody already matched on their address
        // would be noise.
        let found = found.or_else(|| {
            let key = name_key(&contact.title)?;
            let reason = Reason::Name(contact.title.clone());
            if let Some((_, id)) = names.iter().find(|(k, _)| *k == key) {
                return Some(Duplicate {
                    incoming: i,
                    existing_id: Some((*id).to_string()),
                    existing_incoming: None,
                    reason,
                });
            }
            seen_names
                .iter()
                .find(|(k, _)| *k == key)
                .map(|(_, earlier)| Duplicate {
                    incoming: i,
                    existing_id: None,
                    existing_incoming: Some(*earlier),
                    reason,
                })
        });

        if let Some(duplicate) = found {
            out.push(duplicate);
        }

        for (key, _) in keys {
            seen_keys.push((key, i));
        }
        if let Some(key) = name_key(&contact.title) {
            seen_names.push((key, i));
        }
    }

    out
}

// ─── Merging ────────────────────────────────────────────────

/// Fold an incoming contact into one already in the vault.
///
/// Nothing already recorded is overwritten. A field the vault has and the
/// import also has keeps the vault's — somebody typed that, and an export
/// from a phone is not a better authority on it than they are. What the
/// import adds is what the vault was missing: new details, new tags, a
/// birthday nobody had filled in.
///
/// The result is a patch, so a key it does not name is left alone.
pub fn merge(existing: &Value, incoming: &Map<String, Value>) -> Map<String, Value> {
    let mut patch = Map::new();

    // Scalars: fill a gap, never replace an answer.
    for key in ["nickname", "birthday", "company", "contact_frequency"] {
        let have = existing
            .get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .unwrap_or("");
        if have.is_empty() {
            if let Some(value) = incoming.get(key).and_then(Value::as_str) {
                if !value.trim().is_empty() {
                    patch.insert(key.into(), json!(value));
                }
            }
        }
    }

    // Details: union, keyed on what the value means rather than how it is
    // written, so `+84 90 123 4567` does not join `0901234567`.
    let detail_key = |detail: &Value| -> String {
        let label = detail.get("label").and_then(Value::as_str).unwrap_or("");
        let value = detail.get("value").and_then(Value::as_str).unwrap_or("");
        email_key(value)
            .or_else(|| phone_key(value))
            .unwrap_or_else(|| format!("{}|{}", label.to_lowercase(), value.trim().to_lowercase()))
    };

    let mut details: Vec<Value> = existing
        .get("details")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut have: Vec<String> = details.iter().map(detail_key).collect();
    let mut added = false;
    for detail in incoming.get("details").and_then(Value::as_array).map(Vec::as_slice).unwrap_or(&[]) {
        let key = detail_key(detail);
        if !have.contains(&key) {
            have.push(key);
            details.push(detail.clone());
            added = true;
        }
    }
    if added {
        patch.insert("details".into(), json!(details));
    }

    // Relationships are a list, and two lists pool rather than one winning.
    if let Some(merged) = union_array(existing, incoming, "relationship_type", |v| {
        v.as_str().unwrap_or("").to_ascii_lowercase()
    }) {
        patch.insert("relationship_type".into(), merged);
    }

    // Tags and key dates: union.
    if let Some(merged) = union_array(existing, incoming, "tags", |v| {
        v.as_str().unwrap_or("").to_ascii_lowercase()
    }) {
        patch.insert("tags".into(), merged);
    }
    if let Some(merged) = union_array(existing, incoming, "important_dates", |v| {
        format!(
            "{}|{}",
            v.get("label").and_then(Value::as_str).unwrap_or("").to_lowercase(),
            v.get("date").and_then(Value::as_str).unwrap_or("")
        )
    }) {
        patch.insert("important_dates".into(), merged);
    }

    // A job history the vault already has is a history; an import knows only
    // about now, and appending it would claim two current jobs.
    let has_jobs = existing
        .get("experiences")
        .and_then(Value::as_array)
        .is_some_and(|j| !j.is_empty());
    if !has_jobs {
        if let Some(jobs) = incoming.get("experiences") {
            patch.insert("experiences".into(), jobs.clone());
        }
    }

    // The flat copies follow whatever the details ended up as.
    if let Some(details) = patch.get("details").and_then(Value::as_array).cloned() {
        for (key, needle) in [("email", "email"), ("phone", "phone")] {
            let first = details.iter().find_map(|d| {
                let label = d.get("label")?.as_str()?.to_ascii_lowercase();
                label
                    .contains(needle)
                    .then(|| d.get("value")?.as_str().map(str::to_string))
                    .flatten()
            });
            if let Some(value) = first {
                patch.insert(key.into(), json!(value));
            }
        }
    }

    patch
}

fn union_array(
    existing: &Value,
    incoming: &Map<String, Value>,
    key: &str,
    identity: impl Fn(&Value) -> String,
) -> Option<Value> {
    let mut merged: Vec<Value> = existing
        .get(key)
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut seen: Vec<String> = merged.iter().map(&identity).collect();

    let mut added = false;
    for item in incoming.get(key).and_then(Value::as_array)?.iter() {
        let id = identity(item);
        if !seen.contains(&id) {
            seen.push(id);
            merged.push(item.clone());
            added = true;
        }
    }
    added.then(|| json!(merged))
}

#[cfg(test)]
mod tests;
