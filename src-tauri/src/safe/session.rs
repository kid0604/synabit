//! A Safe that is open, and everything that can be done with it.
//!
//! While the Safe is locked, this holds nothing. While it is open, it holds the
//! Safe Key and the list of items — titles, usernames, hosts, tags — and **no
//! value from any concealed field**. A value is decrypted from its file when it
//! is asked for, handed on, and dropped.
//!
//! Every call names the vault it is about. A Safe opened for one vault is not
//! open for another: switching vaults with a Safe open finds it locked.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use super::crypto::Key;
use super::format::Keyset;
use super::generator::GenerateError;
use super::item::{ItemBody, ItemEdit, ItemError, ItemKind, ItemSummary, ItemView, SecretString};
use super::keyset::KeysetError;
use super::store::{self, ItemId, StoreError, Unreadable};

/// Why a Safe command did not do what it was asked, in a form the screen can
/// translate: [`SafeError::code`] becomes `SAFE:<code>` over IPC.
#[derive(Debug, thiserror::Error)]
pub enum SafeError {
    #[error("the Safe is locked")]
    Locked,
    #[error("this vault has no Safe yet")]
    NoSafe,
    #[error("this vault already has a Safe")]
    AlreadyExists,
    #[error("the master password or Secret Key is wrong")]
    WrongPassword,
    /// The keychain on this device has no Secret Key for this Safe — a new
    /// device, or a keychain that was reset. The user types it in.
    #[error("this device does not have the Secret Key for this Safe")]
    NeedsSecretKey,
    #[error("that is not a valid Secret Key")]
    BadSecretKey,
    #[error("a name for Syn is 2–40 lower-case letters, digits and dashes")]
    BadHandle,
    #[error("another item already has that name")]
    HandleTaken,
    #[error("an item Syn may use needs at least one place it may be sent")]
    NoDestination,
    #[error("that request is no longer open; ask Syn again")]
    RequestGone,
    #[error("checking for breaches is turned off in Safe's settings")]
    BreachCheckOff,
    #[error("this file is not an export Safe knows how to read")]
    ImportUnknown,
    #[error("this is a password-protected Bitwarden export; export it again without a password")]
    EncryptedBitwarden,
    #[error("a Safe export needs its export password")]
    NeedsExportPassword,
    #[error("the export password is wrong, or the file was altered")]
    WrongExportPassword,
    #[error("the KeePass database's password is wrong, or it also needs a key file")]
    WrongKdbxPassword,
    #[error("the master password needs at least {} characters", super::keyset::MIN_PASSWORD_CHARS)]
    PasswordTooShort,
    #[error("no such item")]
    NotFound,
    #[error(transparent)]
    Item(#[from] ItemError),
    #[error(transparent)]
    Generate(#[from] GenerateError),
    #[error("the keychain could not be reached: {0}")]
    Keychain(String),
    #[error("the clipboard could not be reached: {0}")]
    Clipboard(String),
    /// Anything else: a damaged file, a disk error. Never contains a secret —
    /// errors are logged when they cross to the screen.
    #[error("{0}")]
    Failed(String),
}

impl SafeError {
    pub fn code(&self) -> &'static str {
        match self {
            SafeError::Locked => "locked",
            SafeError::NoSafe => "no_safe",
            SafeError::AlreadyExists => "already_exists",
            SafeError::WrongPassword => "wrong_password",
            SafeError::NeedsSecretKey => "needs_secret_key",
            SafeError::BadSecretKey => "bad_secret_key",
            SafeError::BadHandle => "bad_handle",
            SafeError::HandleTaken => "handle_taken",
            SafeError::NoDestination => "no_destination",
            SafeError::RequestGone => "request_gone",
            SafeError::BreachCheckOff => "breach_check_off",
            SafeError::ImportUnknown => "import_unknown",
            SafeError::EncryptedBitwarden => "encrypted_bitwarden",
            SafeError::NeedsExportPassword => "needs_export_password",
            SafeError::WrongExportPassword => "wrong_export_password",
            SafeError::WrongKdbxPassword => "wrong_kdbx_password",
            SafeError::PasswordTooShort => "password_too_short",
            SafeError::NotFound => "not_found",
            SafeError::Item(ItemError::NoTitle) => "no_title",
            SafeError::Item(ItemError::UnknownField) => "unknown_field",
            SafeError::Item(ItemError::BadTotp) => "bad_totp",
            SafeError::Generate(_) => "bad_recipe",
            SafeError::Keychain(_) => "keychain",
            SafeError::Clipboard(_) => "clipboard",
            SafeError::Failed(_) => "failed",
        }
    }

    /// Whether this is something going wrong, as against the Safe working —
    /// a wrong password is the lock doing its job, and not logged as an error.
    pub fn is_failure(&self) -> bool {
        matches!(self, SafeError::Keychain(_) | SafeError::Clipboard(_) | SafeError::Failed(_))
    }
}

impl From<KeysetError> for SafeError {
    fn from(e: KeysetError) -> Self {
        use super::format::OpenError;
        match e {
            KeysetError::AlreadyExists => SafeError::AlreadyExists,
            KeysetError::Missing => SafeError::NoSafe,
            KeysetError::PasswordTooShort => SafeError::PasswordTooShort,
            KeysetError::Open(OpenError::WrongPassword) => SafeError::WrongPassword,
            other => SafeError::Failed(other.to_string()),
        }
    }
}

impl From<StoreError> for SafeError {
    fn from(e: StoreError) -> Self {
        SafeError::Failed(e.to_string())
    }
}

/// Per-device settings, in `.synabit/safe/settings.json`. Not secret, not
/// synced: how long this machine waits before locking is a fact about this
/// machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Settings {
    /// Lock after this long without a Safe command. 0 is never, which the
    /// screen does not offer — the cap is eight hours.
    #[serde(default = "default_auto_lock")]
    pub auto_lock_secs: u64,
    /// Clear a copied value from the clipboard after this long, if it is still
    /// there. 0 is never.
    #[serde(default = "default_clipboard_clear")]
    pub clipboard_clear_secs: u64,
    /// Whether passwords may be checked against Have I Been Pwned. Off until
    /// the user turns it on: Synabit does not reach the network unasked, even
    /// with five characters of a hash.
    #[serde(default)]
    pub breach_check: bool,
    /// Whether the SSH agent runs. Off until turned on.
    #[serde(default)]
    pub ssh_agent: bool,
    /// Whether each signature waits for a yes in the app. On by default.
    #[serde(default = "yes")]
    pub ssh_confirm: bool,
    /// Whether `synabit-safe run` may ask. Off until turned on.
    #[serde(default)]
    pub cli: bool,
}

fn yes() -> bool {
    true
}

fn default_auto_lock() -> u64 {
    600
}

fn default_clipboard_clear() -> u64 {
    30
}

pub const MAX_AUTO_LOCK_SECS: u64 = 8 * 60 * 60;

impl Default for Settings {
    fn default() -> Self {
        Settings {
            auto_lock_secs: default_auto_lock(),
            clipboard_clear_secs: default_clipboard_clear(),
            breach_check: false,
            ssh_agent: false,
            ssh_confirm: true,
            cli: false,
        }
    }
}

impl Settings {
    fn path(vault: &Path) -> PathBuf {
        vault.join(".synabit").join("safe").join("settings.json")
    }

    pub fn load(vault: &Path) -> Settings {
        std::fs::read(Self::path(vault))
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Settings>(&bytes).ok())
            .map(Settings::clamped)
            .unwrap_or_default()
    }

    pub fn save(self, vault: &Path) -> Result<(), SafeError> {
        let bytes = serde_json::to_vec_pretty(&self.clamped()).expect("settings serialise");
        store::write_atomic(&Self::path(vault), &bytes).map_err(|e| SafeError::Failed(e.to_string()))
    }

    fn clamped(self) -> Settings {
        Settings {
            auto_lock_secs: self.auto_lock_secs.clamp(60, MAX_AUTO_LOCK_SECS),
            clipboard_clear_secs: self.clipboard_clear_secs.min(600),
            breach_check: self.breach_check,
            ssh_agent: self.ssh_agent,
            ssh_confirm: self.ssh_confirm,
            cli: self.cli,
        }
    }
}

struct Entry {
    revision: u64,
    /// `None` for a tombstone: the id is remembered so its revision is, and
    /// nothing else is.
    summary: Option<ItemSummary>,
    /// What Syn may know of it. Kept beside the summary so that answering Syn
    /// — "is there a usable `github-token`?" — decrypts nothing it need not.
    ai: super::item::AiPolicy,
    handle: Option<String>,
}

impl Entry {
    fn of(revision: u64, id: &ItemId, body: Option<&ItemBody>) -> Entry {
        Entry {
            revision,
            summary: body.map(|b| b.summary(&hex::encode(id))),
            ai: body.map(|b| b.ai.clone()).unwrap_or_default(),
            handle: body.and_then(|b| b.handle.clone()),
        }
    }
}

/// What Syn is told about an item it may see: never a value.
#[derive(Debug, Clone, Serialize)]
pub struct AiItem {
    pub handle: String,
    pub title: String,
    pub kind: ItemKind,
    pub hosts: Vec<String>,
    /// Where it may be sent — only for an item Syn may use.
    pub destinations: Vec<String>,
    pub usable: bool,
    pub expires_at: Option<i64>,
}

/// An open Safe.
pub struct Unlocked {
    vault: PathBuf,
    keyset: Keyset,
    safe_key: Key,
    entries: HashMap<ItemId, Entry>,
    unreadable: Vec<Unreadable>,
    last_used: Instant,
    settings: Settings,
    /// Fingerprints of every concealed value, for the leak guard.
    prints: super::guard::Fingerprints,
    /// Each live item judged on its own; see `health::Assessed`.
    assessed: HashMap<ItemId, super::health::Assessed>,
    /// What the last breach check found, this session.
    breached: HashSet<ItemId>,
    breach_checked_at: Option<i64>,
    /// Every item's health flags, from the two above.
    health: HashMap<ItemId, Vec<super::health::Flag>>,
    /// For the reuse check's fingerprints; new every session.
    health_key: [u8; 32],
}

/// What the list can be narrowed to.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(tag = "by", rename_all = "snake_case")]
pub enum Filter {
    #[default]
    All,
    Favorites,
    Kind {
        kind: ItemKind,
    },
    Tag {
        tag: String,
    },
    Trash,
    /// Items the health check flagged; with `flag`, only that one.
    Health {
        #[serde(default)]
        flag: Option<super::health::Flag>,
    },
}

/// The counts beside each entry of the sidebar.
#[derive(Debug, Clone, Serialize, Default)]
pub struct Overview {
    pub all: usize,
    pub favorites: usize,
    pub trash: usize,
    pub kinds: Vec<(ItemKind, usize)>,
    pub tags: Vec<(String, usize)>,
    /// Files that did not open. Shown, never acted on.
    pub unreadable: Vec<Unreadable>,
    /// Items with any health flag, and how many carry each.
    pub unhealthy: usize,
    pub health: Vec<(super::health::Flag, usize)>,
    pub breach_checked_at: Option<i64>,
}

fn parse_id(id: &str) -> Result<ItemId, SafeError> {
    hex::decode(id).ok().and_then(|b| b.try_into().ok()).ok_or(SafeError::NotFound)
}

impl Unlocked {
    /// Read every item of the Safe in `vault` and hold it open.
    pub fn open(vault: &Path, keyset: Keyset, safe_key: Key) -> Result<Self, SafeError> {
        super::memory::harden();
        let mut unlocked = Unlocked {
            vault: vault.to_path_buf(),
            keyset,
            safe_key,
            entries: HashMap::new(),
            unreadable: Vec::new(),
            last_used: Instant::now(),
            settings: Settings::load(vault),
            prints: super::guard::Fingerprints::default(),
            assessed: HashMap::new(),
            breached: HashSet::new(),
            breach_checked_at: None,
            health: HashMap::new(),
            health_key: super::crypto::random_bytes().map_err(|e| SafeError::Failed(e.to_string()))?,
        };
        unlocked.reload()?;
        Ok(unlocked)
    }

    /// Read every item again — after sync brought some — and fold in any
    /// version sync set aside.
    pub fn reload(&mut self) -> Result<(), SafeError> {
        if let Ok(on_disk) = super::keyset::read(&self.vault) {
            if on_disk.header.safe_id == self.keyset.header.safe_id {
                super::sync::saw_keyset(&self.vault, on_disk.header.keyset_revision);
                self.keyset = on_disk;
            }
        }
        let (loaded, unreadable) = store::load_all(&self.vault, &self.keyset, &self.safe_key)?;
        let mut prints = super::guard::Fingerprints::new(&self.safe_key, std::iter::empty());
        let now = chrono::Utc::now().timestamp();
        let mut assessed = HashMap::new();
        self.entries = loaded
            .into_iter()
            .map(|item| {
                super::sync::saw_item(&self.vault, &item.id, item.revision);
                if let Some(b) = item.body.as_ref() {
                    fingerprint(&mut prints, b);
                    if let Some(a) = super::health::assess_one(b, now, &self.health_key) {
                        assessed.insert(item.id, a);
                    }
                }
                (item.id, Entry::of(item.revision, &item.id, item.body.as_ref()))
            })
            .collect();
        self.prints = prints;
        self.assessed = assessed;
        self.rejudge();
        self.unreadable = unreadable;
        self.resolve_conflicts();
        Ok(())
    }

    /// Fold each version sync set aside into the item it belongs to.
    ///
    /// The fold is deterministic (`ItemBody::absorb`), so two devices holding
    /// the same pair write the same item; if they do it at the same moment the
    /// two new revisions tie again, and that second fold changes nothing and
    /// ends it. A version that does not open stays where it is and is
    /// reported, never deleted.
    fn resolve_conflicts(&mut self) {
        for (id, path) in super::sync::conflicts_for(&self.vault) {
            let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            let other = std::fs::read(&path)
                .map_err(|e| e.to_string())
                .and_then(|b| super::format::ItemFile::decode(&b).map_err(|e| e.to_string()))
                .and_then(|f| f.open(&self.safe_key, &self.keyset.header.safe_id, &id).map_err(|e| e.to_string()))
                .and_then(|content| match content {
                    super::format::ItemContent::Tombstone => Ok(None),
                    super::format::ItemContent::Body(json) => {
                        ItemBody::from_json(&json).map(Some).map_err(|e| e.to_string())
                    }
                });
            let other = match other {
                Ok(other) => other,
                Err(reason) => {
                    self.unreadable.push(Unreadable { file: format!("conflicts/{name}"), reason });
                    continue;
                }
            };
            let current = self.entries.get(&id).and_then(|e| e.summary.as_ref()).map(|_| self.body(&id));
            let outcome = match (current, other) {
                // A live item and a live version beside it: fold.
                (Some(Ok(mut body)), Some(other)) => {
                    if body.absorb(other) {
                        self.write(id, &body)
                    } else {
                        Ok(())
                    }
                }
                // One side was deleted for good. A deletion is not undone by an
                // edit made at the same moment — nor an edit lost to a
                // deletion's tombstone being the one kept: the live side stays.
                (Some(Ok(_)), None) | (None, _) => Ok(()),
                (Some(Err(e)), _) => Err(e),
            };
            match outcome {
                Ok(()) => {
                    if let Err(e) = std::fs::remove_file(&path) {
                        log::warn!("[Safe] could not remove the merged version {name}: {e}");
                    }
                }
                Err(e) => self.unreadable.push(Unreadable { file: format!("conflicts/{name}"), reason: e.to_string() }),
            }
        }
    }

    pub fn settings(&self) -> Settings {
        self.settings
    }

    pub fn set_settings(&mut self, settings: Settings) -> Result<Settings, SafeError> {
        settings.save(&self.vault)?;
        self.settings = settings.clamped();
        Ok(self.settings)
    }

    pub fn keyset(&self) -> &Keyset {
        &self.keyset
    }

    pub fn safe_key(&self) -> &Key {
        &self.safe_key
    }

    pub fn replace_keyset(&mut self, keyset: Keyset) {
        self.keyset = keyset;
    }

    fn live(&self) -> impl Iterator<Item = &ItemSummary> {
        self.entries.values().filter_map(|e| e.summary.as_ref())
    }

    fn flags_of(&self, s: &ItemSummary) -> Vec<super::health::Flag> {
        parse_id(&s.id).ok().and_then(|id| self.health.get(&id).cloned()).unwrap_or_default()
    }

    pub fn list(&self, filter: &Filter, query: &str) -> Vec<ItemSummary> {
        let mut out: Vec<ItemSummary> = self
            .live()
            .map(|s| ItemSummary { health: self.flags_of(s), ..s.clone() })
            .filter(|s| match filter {
                Filter::Trash => s.trashed,
                _ if s.trashed => false,
                Filter::All => true,
                Filter::Favorites => s.favorite,
                Filter::Kind { kind } => s.kind == *kind,
                Filter::Tag { tag } => s.tags.iter().any(|t| t.eq_ignore_ascii_case(tag)),
                Filter::Health { flag: None } => !s.health.is_empty(),
                Filter::Health { flag: Some(f) } => s.health.contains(f),
            })
            .filter(|s| query.trim().is_empty() || s.matches(query))
            .collect();
        out.sort_by(|a, b| a.title.to_lowercase().cmp(&b.title.to_lowercase()).then_with(|| a.id.cmp(&b.id)));
        out
    }

    pub fn overview(&self) -> Overview {
        let mut o = Overview {
            unreadable: self.unreadable.clone(),
            breach_checked_at: self.breach_checked_at,
            ..Default::default()
        };
        let mut by_flag: std::collections::BTreeMap<super::health::Flag, usize> = Default::default();
        for flags in self.health.values().filter(|f| !f.is_empty()) {
            o.unhealthy += 1;
            for f in flags {
                *by_flag.entry(*f).or_default() += 1;
            }
        }
        o.health = by_flag.into_iter().collect();
        let mut kinds: HashMap<ItemKind, usize> = HashMap::new();
        let mut tags: HashMap<String, (String, usize)> = HashMap::new();
        for s in self.live() {
            if s.trashed {
                o.trash += 1;
                continue;
            }
            o.all += 1;
            o.favorites += usize::from(s.favorite);
            *kinds.entry(s.kind).or_default() += 1;
            for t in &s.tags {
                tags.entry(t.to_lowercase()).or_insert_with(|| (t.clone(), 0)).1 += 1;
            }
        }
        o.kinds = kinds.into_iter().collect();
        o.kinds.sort_by_key(|(k, _)| format!("{k:?}"));
        o.tags = tags.into_values().collect();
        o.tags.sort_by_key(|(t, _)| t.to_lowercase());
        o
    }

    fn body(&self, id: &ItemId) -> Result<ItemBody, SafeError> {
        let entry = self.entries.get(id).ok_or(SafeError::NotFound)?;
        if entry.summary.is_none() {
            return Err(SafeError::NotFound);
        }
        store::load(&self.vault, &self.keyset, &self.safe_key, id)?.body.ok_or(SafeError::NotFound)
    }

    fn write(&mut self, id: ItemId, body: &ItemBody) -> Result<(), SafeError> {
        let revision = self.entries.get(&id).map_or(1, |e| e.revision + 1);
        store::save(&self.vault, &self.keyset, &self.safe_key, &id, revision, body)?;
        super::sync::saw_item(&self.vault, &id, revision);
        fingerprint(&mut self.prints, body);
        self.entries.insert(id, Entry::of(revision, &id, Some(body)));
        match super::health::assess_one(body, chrono::Utc::now().timestamp(), &self.health_key) {
            Some(a) => self.assessed.insert(id, a),
            None => self.assessed.remove(&id),
        };
        self.rejudge();
        Ok(())
    }

    fn rejudge(&mut self) {
        self.health = super::health::combine(&self.assessed, &self.breached);
    }

    /// Every live item's passwords as the breach check sends them — five hex
    /// characters of SHA-1 — and the rest it matches against, by item.
    pub fn breach_queries(&self) -> Result<Vec<(ItemId, String, String)>, SafeError> {
        let mut out = Vec::new();
        for id in self.assessed.keys() {
            let body = self.body(id)?;
            for f in body.fields.iter().filter(|f| f.kind == super::item::FieldKind::Password && !f.value.is_empty()) {
                let (prefix, suffix) = super::health::breach::split(f.value.expose());
                out.push((*id, prefix, suffix));
            }
        }
        Ok(out)
    }

    /// Every OpenSSH private key in a live item's hidden fields, with the
    /// item's title. For the SSH agent, in this process only.
    pub fn ssh_keys(&self) -> Vec<(String, SecretString)> {
        let mut out = Vec::new();
        for (id, entry) in &self.entries {
            if entry.summary.as_ref().is_none_or(|s| s.trashed) {
                continue;
            }
            let Ok(body) = self.body(id) else { continue };
            for f in body.fields.iter().filter(|f| f.kind.is_concealed() && f.value.expose().contains("-----BEGIN OPENSSH PRIVATE KEY-----")) {
                out.push((body.title.clone(), f.value.clone()));
            }
        }
        out.sort_by(|a, b| a.0.cmp(&b.0));
        out
    }

    /// The value `synabit-safe run` asked for: an item by its Syn name or its
    /// title, and its first hidden field — or, after a dot, the field of that
    /// label. Only a live item; a title two items share is refused by name.
    pub fn cli_value(&self, reference: &str) -> Result<SecretString, String> {
        let live = || self.entries.iter().filter(|(_, e)| e.summary.as_ref().is_some_and(|s| !s.trashed));
        let find = |name: &str| -> Result<Option<ItemId>, String> {
            if let Some((id, _)) = live().find(|(_, e)| e.handle.as_deref() == Some(name)) {
                return Ok(Some(*id));
            }
            let titled: Vec<ItemId> = live()
                .filter(|(_, e)| e.summary.as_ref().is_some_and(|s| s.title.eq_ignore_ascii_case(name)))
                .map(|(id, _)| *id)
                .collect();
            match titled.len() {
                0 => Ok(None),
                1 => Ok(Some(titled[0])),
                _ => Err(format!("more than one item is called “{name}”; give one of them a name for Syn and use that")),
            }
        };
        let (id, field) = match find(reference)? {
            Some(id) => (id, None),
            None => match reference.rsplit_once('.') {
                Some((item, field)) => (find(item)?.ok_or_else(|| format!("no item called “{item}”"))?, Some(field)),
                None => return Err(format!("no item called “{reference}”")),
            },
        };
        let body = self.body(&id).map_err(|e| e.to_string())?;
        let chosen = match field {
            Some(label) => body.fields.iter().find(|f| f.label.eq_ignore_ascii_case(label)),
            None => body.fields.iter().find(|f| f.kind.is_concealed() && !f.value.is_empty()),
        };
        chosen.map(|f| f.value.clone()).ok_or_else(|| format!("“{reference}” has no such field"))
    }

    pub fn set_breached(&mut self, breached: HashSet<ItemId>, at: i64) {
        self.breached = breached;
        self.breach_checked_at = Some(at);
        self.rejudge();
    }

    pub fn breach_checked_at(&self) -> Option<i64> {
        self.breach_checked_at
    }

    /// Counts per flag, for Syn: numbers only, and the names of items Syn may
    /// already know of.
    pub fn health_for_syn(&self) -> serde_json::Value {
        let mut counts: std::collections::BTreeMap<super::health::Flag, usize> = Default::default();
        for flags in self.health.values() {
            for f in flags {
                *counts.entry(*f).or_default() += 1;
            }
        }
        let named: Vec<serde_json::Value> = self
            .entries
            .iter()
            .filter(|(_, e)| e.handle.is_some() && e.ai.level != super::item::AiLevel::Hidden && e.summary.as_ref().is_some_and(|s| !s.trashed))
            .filter_map(|(id, e)| {
                let flags = self.health.get(id).filter(|f| !f.is_empty())?;
                Some(serde_json::json!({ "handle": e.handle, "flags": flags }))
            })
            .collect();
        serde_json::json!({ "counts": counts, "items_you_may_know": named, "breach_checked": self.breach_checked_at.is_some() })
    }

    /// The leak guard's view of this Safe.
    pub fn fingerprints(&self) -> &super::guard::Fingerprints {
        &self.prints
    }

    /// The items Syn may know of, by handle.
    pub fn ai_items(&self) -> Vec<AiItem> {
        let mut out: Vec<AiItem> = self
            .entries
            .values()
            .filter_map(|e| {
                let s = e.summary.as_ref().filter(|s| !s.trashed)?;
                let handle = e.handle.clone()?;
                let usable = e.ai.level == super::item::AiLevel::Usable;
                (usable || e.ai.level == super::item::AiLevel::Listed).then(|| AiItem {
                    handle,
                    title: s.title.clone(),
                    kind: s.kind,
                    hosts: s.hosts.clone(),
                    destinations: if usable { e.ai.destinations.clone() } else { Vec::new() },
                    usable,
                    expires_at: None,
                })
            })
            .collect();
        out.sort_by(|a, b| a.handle.cmp(&b.handle));
        out
    }

    /// Set what Syn may do with an item: its level, the handle it is called
    /// by, and where it may be sent.
    pub fn set_ai(
        &mut self,
        id: &str,
        level: super::item::AiLevel,
        handle: Option<String>,
        destinations: Vec<String>,
        now: i64,
    ) -> Result<ItemView, SafeError> {
        use super::item::AiLevel;
        let id = parse_id(id)?;
        let mut body = self.body(&id)?;
        let handle = handle.map(|h| h.trim().to_string()).filter(|h| !h.is_empty());
        if level != AiLevel::Hidden {
            let h = handle.as_deref().ok_or(SafeError::BadHandle)?;
            if !is_handle(h) {
                return Err(SafeError::BadHandle);
            }
            if self.entries.iter().any(|(other, e)| *other != id && e.summary.is_some() && e.handle.as_deref() == Some(h)) {
                return Err(SafeError::HandleTaken);
            }
        }
        let destinations: Vec<String> = destinations.into_iter().map(|d| d.trim().to_string()).filter(|d| valid_destination(d)).collect();
        if level == AiLevel::Usable && destinations.is_empty() {
            return Err(SafeError::NoDestination);
        }
        body.ai.level = level;
        body.ai.destinations = if level == AiLevel::Usable { destinations } else { Vec::new() };
        body.handle = handle;
        body.updated_at = now;
        self.write(id, &body)?;
        self.view(&hex::encode(id))
    }

    pub fn view(&self, id: &str) -> Result<ItemView, SafeError> {
        let id = parse_id(id)?;
        let mut view = self.body(&id)?.view(&hex::encode(id));
        view.health = self.health.get(&id).cloned().unwrap_or_default();
        Ok(view)
    }

    /// One field's value. The only way a concealed value leaves this module.
    pub fn reveal(&self, id: &str, field: &str) -> Result<SecretString, SafeError> {
        let id = parse_id(id)?;
        self.body(&id)?.field_value(field).cloned().ok_or(SafeError::NotFound)
    }

    /// The item's current one-time code, and how long it has left.
    pub fn totp(&self, id: &str, now: u64) -> Result<super::totp::Code, SafeError> {
        let id = parse_id(id)?;
        self.body(&id)?.totp.as_ref().map(|t| t.code_at(now)).ok_or(SafeError::NotFound)
    }

    /// Write items brought in from elsewhere, each under a new id.
    pub fn import(&mut self, items: Vec<ItemBody>) -> Result<usize, SafeError> {
        let mut written = 0;
        for body in items {
            let id = store::new_id()?;
            self.write(id, &body)?;
            written += 1;
        }
        Ok(written)
    }

    /// Every live and trashed item, decrypted, for an export. Tombstones are
    /// not items.
    pub fn all_items(&self) -> Result<Vec<ItemBody>, SafeError> {
        let mut ids: Vec<ItemId> = self.entries.iter().filter(|(_, e)| e.summary.is_some()).map(|(id, _)| *id).collect();
        ids.sort();
        ids.iter().map(|id| self.body(id)).collect()
    }

    pub fn create(&mut self, edit: ItemEdit, now: i64) -> Result<ItemView, SafeError> {
        let body = ItemBody::new_from(edit, now)?;
        let id = store::new_id()?;
        self.write(id, &body)?;
        self.view(&hex::encode(id))
    }

    pub fn update(&mut self, id: &str, edit: ItemEdit, now: i64) -> Result<ItemView, SafeError> {
        let id = parse_id(id)?;
        let mut body = self.body(&id)?;
        body.apply(edit, now)?;
        self.write(id, &body)?;
        self.view(&hex::encode(id))
    }

    pub fn set_favorite(&mut self, id: &str, favorite: bool, now: i64) -> Result<(), SafeError> {
        let id = parse_id(id)?;
        let mut body = self.body(&id)?;
        body.favorite = favorite;
        body.updated_at = now;
        self.write(id, &body)
    }

    /// Move to the trash, or back out of it. Trashed items keep everything and
    /// can be restored until they are purged.
    pub fn set_trashed(&mut self, id: &str, trashed: bool, now: i64) -> Result<(), SafeError> {
        let id = parse_id(id)?;
        let mut body = self.body(&id)?;
        body.trashed_at = trashed.then_some(now);
        body.updated_at = now;
        self.write(id, &body)
    }

    /// Delete for good: the file becomes a tombstone. Only from the trash, so
    /// that nothing goes in one step from the list to nowhere.
    pub fn purge(&mut self, id: &str) -> Result<(), SafeError> {
        let id = parse_id(id)?;
        let entry = self.entries.get(&id).ok_or(SafeError::NotFound)?;
        if !entry.summary.as_ref().is_some_and(|s| s.trashed) {
            return Err(SafeError::NotFound);
        }
        let revision = entry.revision + 1;
        store::bury(&self.vault, &self.keyset, &self.safe_key, &id, revision)?;
        super::sync::saw_item(&self.vault, &id, revision);
        self.entries.insert(id, Entry::of(revision, &id, None));
        Ok(())
    }
}

/// Whether the machine was asleep — or this process suspended — between two
/// ticks of a loop meant to run every `tick`.
///
/// The wall clock keeps running while a laptop sleeps; the loop does not. A
/// gap far longer than the tick means the Safe sat open through a sleep, and
/// a lid closed on an open Safe is a lid somebody else may open. Waking is
/// treated like being left alone: it locks. This needs no hook into each
/// operating system's power events, which Tauri does not offer. It does not
/// see the screen being locked without sleeping; the idle timeout covers that.
pub fn slept(previous: std::time::SystemTime, now: std::time::SystemTime, tick: Duration) -> bool {
    match now.duration_since(previous) {
        Ok(gap) => gap > tick * 6,
        // The clock went backwards — set by hand, or corrected. Not a sleep.
        Err(_) => false,
    }
}

fn fingerprint(prints: &mut super::guard::Fingerprints, body: &ItemBody) {
    for f in body.fields.iter().filter(|f| f.kind.is_concealed()) {
        prints.add(f.value.expose());
    }
    if let Some(t) = &body.totp {
        prints.add(t.secret.expose());
    }
}

/// `github-token`: lower-case letters, digits and single dashes, 2–40 long.
pub fn is_handle(h: &str) -> bool {
    (2..=40).contains(&h.len())
        && h.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        && !h.starts_with('-')
        && !h.ends_with('-')
        && !h.contains("--")
}

/// `connector:<id>` or `host:<name>`.
fn valid_destination(d: &str) -> bool {
    match d.split_once(':') {
        Some(("connector", id)) => !id.is_empty(),
        Some(("host", h)) => !h.is_empty() && !h.contains('/') && !h.contains(' '),
        _ => false,
    }
}

impl Unlocked {
    /// Whether the item Syn calls `handle` may be sent to `destination` —
    /// without decrypting anything.
    pub fn permits(&self, handle: &str, destination: &super::egress::Destination) -> Result<(), super::egress::EgressError> {
        use super::egress::EgressError;
        let entry = self
            .entries
            .values()
            .find(|e| e.handle.as_deref() == Some(handle) && e.summary.as_ref().is_some_and(|s| !s.trashed))
            .filter(|e| e.ai.level == super::item::AiLevel::Usable)
            // Hidden, only listed, trashed or absent: one answer, so Syn
            // cannot tell an item it may not use from one that does not exist.
            .ok_or_else(|| EgressError::Unknown(handle.to_string()))?;
        if entry.ai.destinations.iter().any(|d| d.eq_ignore_ascii_case(&destination.key())) {
            Ok(())
        } else {
            Err(EgressError::NotAllowed { handle: handle.to_string(), destination: destination.key() })
        }
    }
}

/// Egress asks the open Safe for values through this. Only an item Syn may
/// *use*, only to a destination it was given, only a concealed field.
impl super::egress::Lookup for Unlocked {
    fn value(
        &self,
        p: &super::egress::Placeholder,
        destination: &super::egress::Destination,
    ) -> Result<SecretString, super::egress::EgressError> {
        use super::egress::EgressError;
        self.permits(&p.handle, destination)?;
        let (id, _) = self
            .entries
            .iter()
            .find(|(_, e)| e.handle.as_deref() == Some(p.handle.as_str()) && e.summary.is_some())
            .ok_or_else(|| EgressError::Unknown(p.handle.clone()))?;
        let body = self.body(id).map_err(|_| EgressError::Unknown(p.handle.clone()))?;
        let field = match &p.field {
            Some(label) => body.fields.iter().find(|f| f.label.eq_ignore_ascii_case(label)),
            None => body
                .ai
                .fields
                .first()
                .and_then(|fid| body.fields.iter().find(|f| &f.id == fid))
                .or_else(|| body.fields.iter().find(|f| f.kind.is_concealed() && !f.value.is_empty())),
        };
        match field {
            Some(f) if f.kind.is_concealed() => Ok(f.value.clone()),
            // A username or a URL is not what a placeholder is for; and saying
            // so is better than sending it.
            _ => Err(EgressError::NoSuchField(p.handle.clone())),
        }
    }
}

/// The Safe of whichever vault is open, if it is unlocked.
///
/// One for the process — [`global`] — rather than Tauri-managed state, because
/// the places that need it are not all commands: Syn's gate asks whether an
/// item may go somewhere, and a connector call fills a placeholder, deep in
/// code that is handed a vault path and no `AppHandle`.
#[derive(Default)]
pub struct SafeSession {
    open: Mutex<Option<Unlocked>>,
}

static SESSION: SafeSession = SafeSession { open: Mutex::new(None) };

/// The process's Safe session.
pub fn global() -> &'static SafeSession {
    &SESSION
}

impl SafeSession {
    /// Like [`Self::with`], without counting as use. For Syn: a routine that
    /// reaches for a secret every few minutes must not keep an unattended Safe
    /// open for ever.
    pub fn peek<R>(&self, vault: &Path, f: impl FnOnce(&Unlocked) -> Result<R, SafeError>) -> Result<R, SafeError> {
        match self.guard().as_ref() {
            Some(unlocked) if unlocked.vault == vault => f(unlocked),
            _ => Err(SafeError::Locked),
        }
    }

    /// Whatever Safe is open, whichever vault — for the leak guard, which
    /// sits where no vault is named. Every Safe open in this process holds
    /// secrets this process must not send.
    pub fn peek_any<R>(&self, f: impl FnOnce(Option<&Unlocked>) -> R) -> R {
        f(self.guard().as_ref())
    }

    pub fn install(&self, unlocked: Unlocked) {
        *self.guard() = Some(unlocked);
    }

    /// Lock. Dropping the `Unlocked` wipes the Safe Key.
    pub fn lock(&self) -> bool {
        self.guard().take().is_some()
    }

    /// The vault whose Safe is open, if one is.
    pub fn open_vault(&self) -> Option<PathBuf> {
        self.guard().as_ref().map(|u| u.vault.clone())
    }

    fn guard(&self) -> std::sync::MutexGuard<'_, Option<Unlocked>> {
        self.open.lock().unwrap_or_else(|p| p.into_inner())
    }

    pub fn is_open_for(&self, vault: &Path) -> bool {
        self.guard().as_ref().is_some_and(|u| u.vault == vault)
    }

    /// Run `f` against the open Safe of `vault`, counting it as use.
    pub fn with<R>(&self, vault: &Path, f: impl FnOnce(&mut Unlocked) -> Result<R, SafeError>) -> Result<R, SafeError> {
        let mut guard = self.guard();
        match guard.as_mut() {
            Some(unlocked) if unlocked.vault == vault => {
                unlocked.last_used = Instant::now();
                f(unlocked)
            }
            _ => Err(SafeError::Locked),
        }
    }

    /// Lock if the Safe has been left alone longer than its setting allows.
    /// Returns whether it locked.
    pub fn lock_if_idle(&self, now: Instant) -> bool {
        let mut guard = self.guard();
        let idle = guard.as_ref().is_some_and(|u| {
            now.saturating_duration_since(u.last_used) >= Duration::from_secs(u.settings.auto_lock_secs)
        });
        if idle {
            *guard = None;
        }
        idle
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::safe::crypto::{self, KdfParams, SecretKey};
    use crate::safe::format::KeysetHeader;
    use crate::safe::item::{EditValue, FieldEdit, FieldKind};

    fn open_safe(vault: &Path) -> Unlocked {
        let kdf = KdfParams { m_kib: 64, t: 1, p: 1 };
        let auk = crypto::derive_auk(b"pw", &[0; 32], kdf, &SecretKey::from_bytes([1; 16])).unwrap();
        let header = KeysetHeader { safe_id: [2; 16], key_epoch: 1, keyset_revision: 1, kdf, kdf_salt: [0; 32] };
        let keyset = Keyset::seal(header, &auk, &Key::from_bytes([3; 32]), [0; 24]).unwrap();
        Unlocked::open(vault, keyset, Key::from_bytes([3; 32])).unwrap()
    }

    fn edit(title: &str, password: &str, tags: &[&str]) -> ItemEdit {
        ItemEdit {
            kind: ItemKind::Login,
            title: title.into(),
            fields: vec![
                FieldEdit { id: None, label: "username".into(), kind: FieldKind::Username, value: EditValue::Set { v: SecretString::new("anh".into()) } },
                FieldEdit { id: None, label: "password".into(), kind: FieldKind::Password, value: EditValue::Set { v: SecretString::new(password.into()) } },
            ],
            urls: vec![],
            tags: tags.iter().map(|t| t.to_string()).collect(),
            favorite: false,
            notes: String::new(),
                totp: Default::default(),
            expires_at: None,
        }
    }

    #[test]
    fn create_list_reveal_and_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let mut safe = open_safe(dir.path());
        let github = safe.create(edit("GitHub", "canary-1", &["work"]), 1).unwrap();
        safe.create(edit("bank", "canary-2", &[]), 1).unwrap();

        let all = safe.list(&Filter::All, "");
        assert_eq!(all.iter().map(|s| s.title.as_str()).collect::<Vec<_>>(), ["bank", "GitHub"]);
        assert_eq!(safe.list(&Filter::Tag { tag: "WORK".into() }, "").len(), 1);
        assert_eq!(safe.list(&Filter::All, "git").len(), 1);

        let password = github.fields.iter().find(|f| f.kind == FieldKind::Password).unwrap();
        assert!(password.value.is_none());
        assert_eq!(safe.reveal(&github.id, &password.id).unwrap().expose(), "canary-1");

        // A second session over the same folder sees the same Safe.
        let again = open_safe(dir.path());
        assert_eq!(again.list(&Filter::All, "").len(), 2);
    }

    #[test]
    fn trash_restore_and_purge() {
        let dir = tempfile::tempdir().unwrap();
        let mut safe = open_safe(dir.path());
        let id = safe.create(edit("GitHub", "x", &[]), 1).unwrap().id;

        assert!(safe.purge(&id).is_err(), "purging skips the trash");
        safe.set_trashed(&id, true, 2).unwrap();
        assert!(safe.list(&Filter::All, "").is_empty());
        assert_eq!(safe.list(&Filter::Trash, "").len(), 1);
        assert_eq!(safe.overview().trash, 1);

        safe.set_trashed(&id, false, 3).unwrap();
        assert_eq!(safe.list(&Filter::All, "").len(), 1);

        safe.set_trashed(&id, true, 4).unwrap();
        safe.purge(&id).unwrap();
        assert!(safe.list(&Filter::Trash, "").is_empty());
        assert!(matches!(safe.view(&id), Err(SafeError::NotFound)));
        // The tombstone is still on disk with a higher revision.
        let reopened = open_safe(dir.path());
        assert!(reopened.list(&Filter::Trash, "").is_empty());
        assert_eq!(reopened.entries.len(), 1);
    }

    #[test]
    fn revisions_rise_with_every_write() {
        let dir = tempfile::tempdir().unwrap();
        let mut safe = open_safe(dir.path());
        let id = safe.create(edit("GitHub", "x", &[]), 1).unwrap().id;
        safe.set_favorite(&id, true, 2).unwrap();
        safe.set_favorite(&id, false, 3).unwrap();
        let reopened = open_safe(dir.path());
        assert_eq!(reopened.entries.values().next().unwrap().revision, 3);
    }

    #[test]
    fn a_session_is_bound_to_its_vault_and_locks_when_idle() {
        let a = tempfile::tempdir().unwrap();
        let b = tempfile::tempdir().unwrap();
        let session = SafeSession::default();
        session.install(open_safe(a.path()));

        assert!(session.with(a.path(), |_| Ok(())).is_ok());
        assert!(matches!(session.with(b.path(), |_| Ok(())), Err(SafeError::Locked)));

        assert!(!session.lock_if_idle(Instant::now()));
        assert!(session.lock_if_idle(Instant::now() + Duration::from_secs(601)));
        assert!(matches!(session.with(a.path(), |_| Ok(())), Err(SafeError::Locked)));
    }

    #[test]
    fn health_is_kept_current_as_items_change() {
        use crate::safe::health::Flag;
        let dir = tempfile::tempdir().unwrap();
        let mut safe = open_safe(dir.path());
        let t = chrono::Utc::now().timestamp();
        let a = safe.create(edit("A", "Vx7#qL9!mR2@tP5$wZ8&", &[]), t).unwrap();
        let b = safe.create(edit("B", "Vx7#qL9!mR2@tP5$wZ8&", &[]), t).unwrap();
        let reused = safe.list(&Filter::Health { flag: Some(Flag::Reused) }, "");
        assert_eq!(reused.len(), 2);
        assert_eq!(safe.overview().health.iter().find(|(f, _)| *f == Flag::Reused).map(|(_, n)| *n), Some(2));

        // Change one: neither is reused any more.
        let view = safe.view(&b.id).unwrap();
        let mut e = edit("B", "Different-Str0ng#Pass!word", &[]);
        e.fields[0].id = Some(view.fields[0].id.clone());
        e.fields[1].id = Some(view.fields[1].id.clone());
        safe.update(&b.id, e, t + 1).unwrap();
        assert!(safe.list(&Filter::Health { flag: Some(Flag::Reused) }, "").is_empty());

        // A weak one shows up, and trashing it takes it off the list.
        let weak = safe.create(edit("C", "password1", &[]), t + 2).unwrap();
        assert!(safe.list(&Filter::Health { flag: None }, "").iter().any(|s| s.id == weak.id && s.health.contains(&Flag::Weak)));
        safe.set_trashed(&weak.id, true, t + 3).unwrap();
        assert!(safe.list(&Filter::Health { flag: None }, "").is_empty(), "{:?}", safe.list(&Filter::Health { flag: None }, ""));
        let _ = a;
    }

    #[test]
    fn a_long_gap_between_ticks_is_a_sleep() {
        use std::time::SystemTime;
        let tick = Duration::from_secs(10);
        let t0 = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000);
        assert!(!slept(t0, t0 + Duration::from_secs(10), tick));
        assert!(!slept(t0, t0 + Duration::from_secs(45), tick), "a slow tick is not a sleep");
        assert!(slept(t0, t0 + Duration::from_secs(61), tick));
        assert!(slept(t0, t0 + Duration::from_secs(8 * 3600), tick));
        assert!(!slept(t0, t0 - Duration::from_secs(3600), tick), "a clock set back is not a sleep");
    }

    #[test]
    fn the_command_line_finds_items_by_name_or_title_and_field() {
        use crate::safe::item::AiLevel;
        let dir = tempfile::tempdir().unwrap();
        let mut safe = open_safe(dir.path());
        let gh = safe.create(edit("GitHub token", "gh-canary", &[]), 1).unwrap();
        safe.set_ai(&gh.id, AiLevel::Listed, Some("github-token".into()), vec![], 2).unwrap();
        safe.create(edit("Twin", "a", &[]), 1).unwrap();
        safe.create(edit("twin", "b", &[]), 1).unwrap();
        let gone = safe.create(edit("Old", "old-canary", &[]), 1).unwrap();
        safe.set_trashed(&gone.id, true, 2).unwrap();

        assert_eq!(safe.cli_value("github-token").unwrap().expose(), "gh-canary");
        assert_eq!(safe.cli_value("github token").unwrap().expose(), "gh-canary", "by title, any case");
        assert_eq!(safe.cli_value("github-token.username").unwrap().expose(), "anh");
        assert_eq!(safe.cli_value("GitHub token.USERNAME").unwrap().expose(), "anh");
        assert!(safe.cli_value("twin").unwrap_err().contains("more than one"));
        assert!(safe.cli_value("Old").is_err(), "a trashed item is not handed out");
        assert!(safe.cli_value("github-token.nope").is_err());
        assert!(safe.cli_value("missing").is_err());
    }

    #[test]
    fn settings_are_clamped_and_kept_per_device() {
        let dir = tempfile::tempdir().unwrap();
        let wild = Settings { auto_lock_secs: 1, clipboard_clear_secs: 99_999, breach_check: true, ssh_agent: true, ssh_confirm: false, cli: true };
        wild.save(dir.path()).unwrap();
        let loaded = Settings::load(dir.path());
        assert_eq!(loaded, Settings { auto_lock_secs: 60, clipboard_clear_secs: 600, ..wild });
        assert!(Settings::path(dir.path()).starts_with(dir.path().join(".synabit")), "a dotdir, so it does not sync");
    }
}
