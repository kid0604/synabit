//! What an item holds once it is decrypted, and the three shapes it takes on
//! its way to the screen.
//!
//! # JSON inside the envelope, not postcard
//!
//! The design said `postcard`. It is the wrong choice for exactly one reason,
//! and the reason is sync: postcard is not self-describing, so a field added in
//! a later build is either a decoding error on an older build or — worse —
//! silently dropped when the older build writes the item back. Two devices a
//! release apart would erase each other's fields.
//!
//! JSON with `#[serde(default)]` reads what an older build wrote, and
//! [`ItemBody::unknown`] carries what a newer build wrote straight back out.
//! The size difference disappears inside the padding.
//!
//! # Three shapes
//!
//! * [`ItemBody`] — the whole item. Lives in Rust only.
//! * [`ItemSummary`] — one row of the list. No secret in it at all.
//! * [`ItemView`] — the detail screen. Every field, but a concealed field's
//!   value is left out: the screen gets its label and a rough length, and asks
//!   for the value itself with `safe_reveal` when the user asks to see it.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use zeroize::Zeroize;

/// How many previous values of one field an item remembers.
pub const HISTORY_PER_FIELD: usize = 20;

/// A string that is wiped when dropped and never printed.
///
/// No `Display`, and `Debug` prints nothing of it. Reading it takes
/// [`SecretString::expose`], which is greppable.
#[derive(Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SecretString(String);

impl SecretString {
    pub fn new(value: String) -> Self {
        Self(value)
    }

    pub fn expose(&self) -> &str {
        &self.0
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn chars(&self) -> usize {
        self.0.chars().count()
    }
}

impl Drop for SecretString {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

impl std::fmt::Debug for SecretString {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("‹redacted›")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ItemKind {
    #[default]
    Login,
    ApiKey,
    SshKey,
    Card,
    SecureNote,
    Identity,
    Wifi,
    /// Anything else, including a kind a newer build added. Only the icon and
    /// the template depend on the kind, so an unknown one loses nothing but
    /// its picture.
    #[serde(other)]
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum FieldKind {
    #[default]
    Text,
    Username,
    Email,
    Url,
    Phone,
    Date,
    Multiline,
    Password,
    /// Hidden like a password but not one: an API token, a card number, a
    /// recovery code.
    Concealed,
    Pin,
    /// A kind a newer build added. Shown as text but treated as concealed:
    /// if this build does not know what it is, it does not know it is safe to
    /// show.
    #[serde(other)]
    Unknown,
}

impl FieldKind {
    pub fn is_concealed(self) -> bool {
        matches!(self, Self::Password | Self::Concealed | Self::Pin | Self::Unknown)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Field {
    /// Stable across edits, so history and Syn's policy can name a field
    /// without depending on its label.
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub kind: FieldKind,
    #[serde(default)]
    pub value: SecretString,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum UrlMatch {
    /// The registrable domain and every subdomain of it.
    #[default]
    Domain,
    Host,
    Exact,
    Never,
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UrlRule {
    pub url: String,
    #[serde(default, rename = "match")]
    pub match_: UrlMatch,
}

/// What Syn may know about an item. Section 8.2 of the design; stored from the
/// start so that no item ever has to be migrated to gain it, and `Hidden` by
/// default because an item nobody thought about is one Syn should not know of.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum AiLevel {
    #[default]
    Hidden,
    Listed,
    Usable,
    /// A level a newer build added. Treated as `Hidden`: when in doubt, Syn
    /// knows less.
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct AiPolicy {
    #[serde(default)]
    pub level: AiLevel,
    /// `host:api.github.com` or `connector:linear`. Strings rather than an
    /// enum until P3 gives them meaning.
    #[serde(default)]
    pub destinations: Vec<String>,
    #[serde(default)]
    pub fields: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub field: String,
    pub value: SecretString,
    pub replaced_at: i64,
}

fn one() -> u16 {
    1
}

/// A whole item, decrypted. Never leaves Rust.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ItemBody {
    #[serde(default = "one")]
    pub schema: u16,
    #[serde(default)]
    pub kind: ItemKind,
    pub title: String,
    /// The short name Syn uses for the item. Unused until P3.
    #[serde(default)]
    pub handle: Option<String>,
    #[serde(default)]
    pub fields: Vec<Field>,
    #[serde(default)]
    pub urls: Vec<UrlRule>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub favorite: bool,
    #[serde(default)]
    pub notes: SecretString,
    /// Vault node ids this item is linked to.
    #[serde(default)]
    pub links: Vec<String>,
    /// One-time codes. Warned about when it sits beside the password of the
    /// same account — the Safe then holds both factors.
    #[serde(default)]
    pub totp: Option<super::totp::Totp>,
    #[serde(default)]
    pub ai: AiPolicy,
    #[serde(default)]
    pub expires_at: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
    #[serde(default)]
    pub history: Vec<HistoryEntry>,
    #[serde(default)]
    pub trashed_at: Option<i64>,
    /// Whatever a newer build wrote that this one does not know, written back
    /// untouched. See the module comment.
    #[serde(flatten)]
    pub unknown: Map<String, Value>,
}

impl ItemBody {
    pub fn to_json(&self) -> zeroize::Zeroizing<Vec<u8>> {
        zeroize::Zeroizing::new(serde_json::to_vec(self).expect("an item always serialises"))
    }

    pub fn from_json(bytes: &[u8]) -> Result<Self, serde_json::Error> {
        serde_json::from_slice(bytes)
    }
}

/// What went wrong reading JSON, without the text it was reading. serde's own
/// message quotes the offending value — `invalid type: string "hunter2"` —
/// and these messages reach the screen and the log.
pub fn json_problem(e: &serde_json::Error) -> String {
    let kind = match e.classify() {
        serde_json::error::Category::Io => "could not be read",
        serde_json::error::Category::Syntax => "is not valid JSON",
        serde_json::error::Category::Data => "does not have the expected shape",
        serde_json::error::Category::Eof => "ends early",
    };
    format!("{kind} (line {}, column {})", e.line(), e.column())
}

impl ItemBody {
    /// Drop a one-time code setup no code can be made from. Every way in
    /// through the editor checks this; a file brought in whole — a Safe export
    /// — is checked here. Returns whether one was dropped.
    pub fn drop_unusable_totp(&mut self) -> bool {
        let unusable = self.totp.as_ref().is_some_and(|t| !t.is_usable());
        if unusable {
            self.totp = None;
        }
        unusable
    }

    /// The field the list shows under the title: a username, else an email.
    fn account(&self) -> Option<&str> {
        self.fields
            .iter()
            .find(|f| f.kind == FieldKind::Username && !f.value.is_empty())
            .or_else(|| self.fields.iter().find(|f| f.kind == FieldKind::Email && !f.value.is_empty()))
            .map(|f| f.value.expose())
    }

    pub fn hosts(&self) -> Vec<String> {
        self.urls.iter().filter_map(|u| host_of(&u.url)).collect()
    }

    pub fn summary(&self, id: &str) -> ItemSummary {
        let hosts = self.hosts();
        ItemSummary {
            id: id.to_string(),
            kind: self.kind,
            title: self.title.clone(),
            subtitle: self.account().map(str::to_string).or_else(|| hosts.first().cloned()).unwrap_or_default(),
            hosts,
            tags: self.tags.clone(),
            favorite: self.favorite,
            trashed: self.trashed_at.is_some(),
            updated_at: self.updated_at,
            ai_level: self.ai.level,
            health: Vec::new(),
        }
    }

    pub fn view(&self, id: &str) -> ItemView {
        ItemView {
            id: id.to_string(),
            kind: self.kind,
            title: self.title.clone(),
            fields: self
                .fields
                .iter()
                .map(|f| {
                    let concealed = f.kind.is_concealed();
                    FieldView {
                        id: f.id.clone(),
                        label: f.label.clone(),
                        kind: f.kind,
                        concealed,
                        value: (!concealed).then(|| f.value.expose().to_string()),
                        length_bucket: concealed.then(|| length_bucket(f.value.chars())),
                        empty: f.value.is_empty(),
                    }
                })
                .collect(),
            urls: self.urls.clone(),
            tags: self.tags.clone(),
            favorite: self.favorite,
            notes: self.notes.expose().to_string(),
            links: self.links.clone(),
            totp: self.totp.as_ref().map(|t| t.view()),
            ai_level: self.ai.level,
            handle: self.handle.clone(),
            ai_destinations: self.ai.destinations.clone(),
            health: Vec::new(),
            expires_at: self.expires_at,
            created_at: self.created_at,
            updated_at: self.updated_at,
            history_count: self.history.len(),
            trashed_at: self.trashed_at,
        }
    }

    /// Apply an edit from the screen, keeping every concealed value the screen
    /// did not touch and remembering every one it replaced.
    pub fn apply(&mut self, edit: ItemEdit, now: i64) -> Result<(), ItemError> {
        let title = edit.title.trim();
        if title.is_empty() {
            return Err(ItemError::NoTitle);
        }

        let mut fields = Vec::with_capacity(edit.fields.len());
        for f in edit.fields {
            let previous = f.id.as_deref().and_then(|id| self.fields.iter().find(|p| p.id == id));
            let value = match f.value {
                EditValue::Unchanged => match previous {
                    Some(p) => p.value.clone(),
                    None => return Err(ItemError::UnknownField),
                },
                EditValue::Set { v } => v,
            };
            if let Some(p) = previous {
                if p.kind.is_concealed() && !p.value.is_empty() && p.value != value {
                    self.history.push(HistoryEntry { field: p.id.clone(), value: p.value.clone(), replaced_at: now });
                }
            }
            let id = match f.id {
                Some(id) if previous.is_some() => id,
                _ => new_field_id(),
            };
            fields.push(Field { id, label: f.label.trim().to_string(), kind: f.kind, value });
        }
        self.fields = fields;
        self.trim_history();

        self.kind = edit.kind;
        self.title = title.to_string();
        self.urls = edit.urls.into_iter().filter(|u| !u.url.trim().is_empty()).collect();
        self.tags = normalise_tags(edit.tags);
        self.favorite = edit.favorite;
        self.notes = SecretString::new(edit.notes);
        match edit.totp {
            TotpEdit::Unchanged => {}
            TotpEdit::Remove => self.totp = None,
            TotpEdit::Set { v } => self.totp = Some(super::totp::Totp::parse(v.expose()).map_err(|_| ItemError::BadTotp)?),
        }
        self.expires_at = edit.expires_at;
        self.updated_at = now;
        Ok(())
    }

    /// Keep the newest [`HISTORY_PER_FIELD`] entries of each field.
    fn trim_history(&mut self) {
        let mut seen: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
        let mut keep = vec![false; self.history.len()];
        for (i, entry) in self.history.iter().enumerate().rev() {
            let n = seen.entry(entry.field.clone()).or_default();
            if *n < HISTORY_PER_FIELD {
                keep[i] = true;
                *n += 1;
            }
        }
        let mut k = keep.into_iter();
        self.history.retain(|_| k.next().unwrap_or(false));
    }

    pub fn new_from(edit: ItemEdit, now: i64) -> Result<Self, ItemError> {
        let mut body = ItemBody {
            schema: 1,
            kind: edit.kind,
            title: String::new(),
            handle: None,
            fields: Vec::new(),
            urls: Vec::new(),
            tags: Vec::new(),
            favorite: false,
            notes: SecretString::default(),
            links: Vec::new(),
            totp: None,
            ai: AiPolicy::default(),
            expires_at: None,
            created_at: now,
            updated_at: now,
            history: Vec::new(),
            trashed_at: None,
            unknown: Map::new(),
        };
        body.apply(edit, now)?;
        Ok(body)
    }

    /// Fold in another version of this item, written at the same revision on
    /// another device.
    ///
    /// The newer edit (by `updated_at`, then device) wins, as the design
    /// says. Every concealed value the other version held that the winner does
    /// not is kept in the winner's history, so two people changing one
    /// password at once lose neither. Deterministic: both devices fold the
    /// same pair into the same body, so they stop disagreeing. Returns whether
    /// the result differs from `self`.
    pub fn absorb(&mut self, other: ItemBody) -> bool {
        let self_newer = (self.updated_at, self.fingerprint()) >= (other.updated_at, other.fingerprint());
        let (mut winner, loser) = if self_newer { (self.clone(), other) } else { (other, self.clone()) };
        for old in loser.fields.iter().filter(|f| f.kind.is_concealed() && !f.value.is_empty()) {
            let current = winner.fields.iter().find(|f| f.id == old.id).map(|f| f.value.clone());
            if current.as_ref() == Some(&old.value) {
                continue;
            }
            if winner.history.iter().any(|h| h.field == old.id && h.value == old.value) {
                continue;
            }
            winner.history.push(HistoryEntry { field: old.id.clone(), value: old.value.clone(), replaced_at: loser.updated_at });
        }
        for h in loser.history {
            if !winner.history.iter().any(|w| w.field == h.field && w.value == h.value) {
                winner.history.push(h);
            }
        }
        winner.history.sort_by(|a, b| a.replaced_at.cmp(&b.replaced_at).then_with(|| a.field.cmp(&b.field)));
        winner.trim_history();
        let changed = winner != *self;
        *self = winner;
        changed
    }

    /// A stable tie-break for two edits made in the same second.
    fn fingerprint(&self) -> [u8; 32] {
        *blake3::hash(&self.to_json()).as_bytes()
    }

    pub fn field_value(&self, field_id: &str) -> Option<&SecretString> {
        self.fields.iter().find(|f| f.id == field_id).map(|f| &f.value)
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ItemError {
    #[error("an item needs a title")]
    NoTitle,
    #[error("the edit keeps a field the item does not have")]
    UnknownField,
    #[error("that one-time code setup is neither an otpauth:// link nor a base32 secret")]
    BadTotp,
}

fn normalise_tags(tags: Vec<String>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for tag in tags {
        let tag = tag.trim().trim_start_matches('#').to_string();
        if !tag.is_empty() && !out.iter().any(|t| t.eq_ignore_ascii_case(&tag)) {
            out.push(tag);
        }
    }
    out
}

pub fn new_field_id() -> String {
    let bytes: [u8; 8] = super::crypto::random_bytes().expect("the system random number generator failed");
    hex::encode(bytes)
}

/// The host of a URL, accepting the bare `github.com` people actually type.
pub fn host_of(raw: &str) -> Option<String> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }
    let parsed = url::Url::parse(raw).or_else(|_| url::Url::parse(&format!("https://{raw}"))).ok()?;
    parsed.host_str().map(|h| h.trim_start_matches("www.").to_ascii_lowercase())
}

/// A concealed value's length, rounded so that the dots on screen do not give
/// away how long the password is.
pub fn length_bucket(chars: usize) -> u8 {
    match chars {
        0 => 0,
        1..=8 => 8,
        9..=12 => 12,
        13..=16 => 16,
        17..=24 => 24,
        _ => 32,
    }
}

// ─── the shapes the screen sees ──────────────────────────

/// One row of the list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ItemSummary {
    pub id: String,
    pub kind: ItemKind,
    pub title: String,
    pub subtitle: String,
    pub hosts: Vec<String>,
    pub tags: Vec<String>,
    pub favorite: bool,
    pub trashed: bool,
    pub updated_at: i64,
    pub ai_level: AiLevel,
    /// What the health check found. Filled in by the session, which sees
    /// every item; empty from `ItemBody::summary`, which sees one.
    #[serde(default)]
    pub health: Vec<super::health::Flag>,
}

impl ItemSummary {
    /// Every word of `query` appears somewhere in the title, subtitle, a host
    /// or a tag.
    pub fn matches(&self, query: &str) -> bool {
        let haystack = format!("{} {} {} {}", self.title, self.subtitle, self.hosts.join(" "), self.tags.join(" "))
            .to_lowercase();
        query.to_lowercase().split_whitespace().all(|word| haystack.contains(word))
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct FieldView {
    pub id: String,
    pub label: String,
    pub kind: FieldKind,
    pub concealed: bool,
    /// Present only when not concealed.
    pub value: Option<String>,
    /// Present only when concealed.
    pub length_bucket: Option<u8>,
    pub empty: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ItemView {
    pub id: String,
    pub kind: ItemKind,
    pub title: String,
    pub fields: Vec<FieldView>,
    pub urls: Vec<UrlRule>,
    pub tags: Vec<String>,
    pub favorite: bool,
    pub notes: String,
    pub links: Vec<String>,
    /// How the item's codes are made, never the secret behind them.
    pub totp: Option<super::totp::TotpView>,
    pub ai_level: AiLevel,
    /// The name Syn uses for it, when Syn may know of it.
    pub handle: Option<String>,
    /// Where Syn may send it — `connector:<id>` — when it may use it.
    pub ai_destinations: Vec<String>,
    /// Filled in by the session, which sees every item.
    pub health: Vec<super::health::Flag>,
    pub expires_at: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
    pub history_count: usize,
    pub trashed_at: Option<i64>,
}

/// A field as the editor sends it back.
#[derive(Debug, Clone, Deserialize)]
pub struct FieldEdit {
    /// `None` for a field added in this edit.
    pub id: Option<String>,
    pub label: String,
    pub kind: FieldKind,
    pub value: EditValue,
}

/// A concealed value the editor never received is sent back as `Unchanged`
/// rather than round-tripped through the WebView.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum EditValue {
    Unchanged,
    /// A `SecretString` straight from deserialisation, so a value is wiped
    /// even when the edit it came in is refused.
    Set { v: SecretString },
}

#[derive(Debug, Clone, Deserialize)]
pub struct ItemEdit {
    pub kind: ItemKind,
    pub title: String,
    #[serde(default)]
    pub fields: Vec<FieldEdit>,
    #[serde(default)]
    pub urls: Vec<UrlRule>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub favorite: bool,
    #[serde(default)]
    pub notes: String,
    #[serde(default)]
    pub totp: TotpEdit,
    #[serde(default)]
    pub expires_at: Option<i64>,
}

/// The one-time-code part of an edit. Like a concealed field, an existing
/// secret is never sent to the editor, so leaving it alone is `Unchanged`.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum TotpEdit {
    #[default]
    Unchanged,
    Remove,
    /// An `otpauth://` link or a base32 secret, as pasted.
    Set { v: SecretString },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(v: &str) -> EditValue {
        EditValue::Set { v: SecretString::new(v.to_string()) }
    }

    fn login(password: &str) -> ItemEdit {
        ItemEdit {
            kind: ItemKind::Login,
            title: "GitHub".into(),
            fields: vec![
                FieldEdit { id: None, label: "username".into(), kind: FieldKind::Username, value: set("anh") },
                FieldEdit { id: None, label: "password".into(), kind: FieldKind::Password, value: set(password) },
            ],
            urls: vec![UrlRule { url: "https://www.github.com/login".into(), match_: UrlMatch::Domain }],
            tags: vec![" #work".into(), "Work".into(), "dev".into()],
            favorite: false,
            notes: String::new(),
            totp: TotpEdit::Unchanged,
            expires_at: None,
        }
    }

    #[test]
    fn a_summary_carries_no_secret() {
        let body = ItemBody::new_from(login("hunter2"), 100).unwrap();
        let summary = serde_json::to_string(&body.summary("x")).unwrap();
        assert!(!summary.contains("hunter2"));
        assert!(summary.contains("anh") && summary.contains("github.com"));
        assert_eq!(body.tags, vec!["work", "dev"], "tags are trimmed, de-hashed and de-duplicated");
    }

    #[test]
    fn a_view_leaves_concealed_values_out() {
        let body = ItemBody::new_from(login("hunter2"), 100).unwrap();
        let view = body.view("x");
        let json = serde_json::to_string(&view).unwrap();
        assert!(!json.contains("hunter2"), "{json}");
        let password = view.fields.iter().find(|f| f.kind == FieldKind::Password).unwrap();
        assert!(password.concealed && password.value.is_none());
        assert_eq!(password.length_bucket, Some(8));
    }

    #[test]
    fn an_unchanged_concealed_value_survives_an_edit_and_a_changed_one_goes_to_history() {
        let mut body = ItemBody::new_from(login("hunter2"), 100).unwrap();
        let ids: Vec<String> = body.fields.iter().map(|f| f.id.clone()).collect();

        let mut edit = login("ignored");
        edit.fields[0].id = Some(ids[0].clone());
        edit.fields[1] = FieldEdit { id: Some(ids[1].clone()), label: "password".into(), kind: FieldKind::Password, value: EditValue::Unchanged };
        body.apply(edit, 200).unwrap();
        assert_eq!(body.fields[1].value.expose(), "hunter2");
        assert_eq!(body.fields[1].id, ids[1], "field ids are stable across edits");
        assert!(body.history.is_empty());

        let mut edit = login("correct horse");
        edit.fields[0].id = Some(ids[0].clone());
        edit.fields[1].id = Some(ids[1].clone());
        body.apply(edit, 300).unwrap();
        assert_eq!(body.fields[1].value.expose(), "correct horse");
        assert_eq!(body.history.len(), 1);
        assert_eq!(body.history[0].value.expose(), "hunter2");
        assert_eq!(body.history[0].replaced_at, 300);
    }

    #[test]
    fn unchanged_for_a_field_the_item_does_not_have_is_refused() {
        let mut edit = login("x");
        edit.fields[1].value = EditValue::Unchanged;
        assert_eq!(ItemBody::new_from(edit, 1).unwrap_err(), ItemError::UnknownField);
    }

    #[test]
    fn history_keeps_the_newest_entries_of_each_field() {
        let mut body = ItemBody::new_from(login("v0"), 0).unwrap();
        let ids: Vec<String> = body.fields.iter().map(|f| f.id.clone()).collect();
        for n in 1..=(HISTORY_PER_FIELD + 5) {
            let mut edit = login(&format!("v{n}"));
            edit.fields[0].id = Some(ids[0].clone());
            edit.fields[1].id = Some(ids[1].clone());
            body.apply(edit, n as i64).unwrap();
        }
        assert_eq!(body.history.len(), HISTORY_PER_FIELD);
        assert_eq!(body.history.last().unwrap().value.expose(), format!("v{}", HISTORY_PER_FIELD + 4));
    }

    /// A field a newer build added is carried back out by this one.
    #[test]
    fn what_a_newer_build_wrote_survives_this_one() {
        let newer = br#"{"schema":2,"kind":"passkey","title":"x","created_at":1,"updated_at":1,
            "fields":[{"id":"a","label":"l","kind":"hardware_key","value":"s"}],
            "totp":{"secret":"JBSWY3DPEHPK3PXP"}}"#;
        let body = ItemBody::from_json(newer).unwrap();
        assert_eq!(body.kind, ItemKind::Other);
        assert!(body.fields[0].kind.is_concealed(), "an unknown field kind is treated as concealed");
        let again: Value = serde_json::from_slice(&body.to_json()).unwrap();
        assert_eq!(again["totp"]["secret"], "JBSWY3DPEHPK3PXP");
    }

    #[test]
    fn a_totp_secret_is_kept_out_of_the_view_and_survives_an_edit() {
        let mut edit = login("x");
        edit.totp = TotpEdit::Set { v: SecretString::new("otpauth://totp/x?secret=JBSWY3DPEHPK3PXP&digits=8".into()) };
        let mut body = ItemBody::new_from(edit, 1).unwrap();
        let json = serde_json::to_string(&body.view("id")).unwrap();
        assert!(!json.contains("JBSWY3DPEHPK3PXP"), "{json}");
        assert_eq!(body.view("id").totp.unwrap().digits, 8);

        let ids: Vec<String> = body.fields.iter().map(|f| f.id.clone()).collect();
        let mut again = login("x");
        again.fields[0].id = Some(ids[0].clone());
        again.fields[1].id = Some(ids[1].clone());
        body.apply(again, 2).unwrap();
        assert!(body.totp.is_some(), "Unchanged kept the secret");

        let mut remove = login("x");
        remove.totp = TotpEdit::Remove;
        body.apply(remove, 3).unwrap();
        assert!(body.totp.is_none());

        let mut bad = login("x");
        bad.totp = TotpEdit::Set { v: SecretString::new("not a secret".into()) };
        assert_eq!(body.apply(bad, 4).unwrap_err(), ItemError::BadTotp);
    }

    /// Two devices changed the same password at once. Folding either version
    /// into the other gives the same item, with the loser's password in the
    /// history — and folding again changes nothing, which is what lets the
    /// two devices stop.
    #[test]
    fn absorbing_a_concurrent_edit_is_symmetric_and_settles() {
        let base = ItemBody::new_from(login("original"), 100).unwrap();
        let ids: Vec<String> = base.fields.iter().map(|f| f.id.clone()).collect();
        let edited = |password: &str, at: i64| {
            let mut b = base.clone();
            let mut e = login(password);
            e.fields[0].id = Some(ids[0].clone());
            e.fields[1].id = Some(ids[1].clone());
            b.apply(e, at).unwrap();
            b
        };
        let on_a = edited("from-a", 200);
        let on_b = edited("from-b", 201);

        let mut a_view = on_a.clone();
        assert!(a_view.absorb(on_b.clone()));
        let mut b_view = on_b.clone();
        b_view.absorb(on_a.clone());
        assert_eq!(a_view, b_view, "the two devices folded to different items");
        assert_eq!(a_view.fields[1].value.expose(), "from-b", "the newer edit wins");
        let kept: Vec<&str> = a_view.history.iter().map(|h| h.value.expose()).collect();
        assert!(kept.contains(&"from-a") && kept.contains(&"original"), "{kept:?}");

        let mut again = a_view.clone();
        assert!(!again.absorb(b_view.clone()), "folding a settled item changed it");
    }

    #[test]
    fn secrets_do_not_print() {
        let body = ItemBody::new_from(login("hunter2"), 1).unwrap();
        assert!(!format!("{body:?}").contains("hunter2"));
    }

    #[test]
    fn search_needs_every_word() {
        let s = ItemBody::new_from(login("x"), 1).unwrap().summary("id");
        assert!(s.matches("git anh"));
        assert!(s.matches("WORK"));
        assert!(!s.matches("git gitlab"));
    }

    #[test]
    fn hosts_accept_what_people_type() {
        assert_eq!(host_of("github.com").as_deref(), Some("github.com"));
        assert_eq!(host_of("https://www.GitHub.com/login").as_deref(), Some("github.com"));
        assert_eq!(host_of("  "), None);
    }
}
