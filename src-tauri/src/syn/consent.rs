//! What Syn is allowed to do, and what it has to ask about first.
//!
//! # Why this exists before anything reaches outside
//!
//! Everything Syn can touch today is in the vault, and everything in the vault
//! comes back: trash, version history, a CRDT log. That is the whole of the
//! current safety model, and it is why `VaultWrite` never asks. The moment a
//! tool can send an email, spend money, or run code, that argument stops
//! holding — and the roadmap puts the door before the opening for exactly that
//! reason. A lock fitted after the door is open is a lock fitted to a room
//! somebody has already walked through.
//!
//! # Where consent is kept, and why it does not travel
//!
//! `{vault}/.synabit/consent.json`. A dotfile, which `sync/utils.rs` skips and
//! `is_in_unscanned_dir` skips, and both of those are load-bearing rather than
//! incidental: **allowing something on a laptop is not allowing it on a
//! phone.** A grant is a judgement made in one place, about one device, with
//! one set of things in reach. Syncing it would silently extend a decision the
//! person made while looking at one screen to a device they were not holding.
//!
//! That is the opposite of `Syn/declined.json`, which *does* sync — a refusal
//! about oneself travels, an authorisation does not. The asymmetry is the
//! point: erring towards asking again is cheap, and erring towards not asking
//! is the failure this module exists to prevent.

use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};

/// How long an `Always` lasts before it is asked about again.
///
/// Ninety days. Not forever, because a permission granted once and never
/// revisited is indistinguishable from one nobody chose — and the person who
/// said yes in September may not recognise what they agreed to in March.
pub const ALWAYS_LASTS_DAYS: i64 = 90;

/// The kind of power a tool has.
///
/// Coarse on purpose. This is not an access-control list; it is the answer to
/// "what sort of thing is this", which is the question a consent card has to
/// put into one sentence a person can read while doing something else.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Capability {
    /// Looks at the vault. Never asks.
    VaultRead,
    /// Changes one node. Never asks, because trash and version history put
    /// every one of these back.
    VaultWrite,
    /// Changes many files at once — a field renamed on every task, a whole kind
    /// removed. Asks in its own way already: called without `confirm_nodes`
    /// these report the count and change nothing.
    VaultStructural,
    /// Reads from somewhere outside. Asked once per host, and remembered.
    NetRead { domain: String },
    /// Sends something outside. Asked per host *and* per tool, because "may
    /// read from this server" and "may post to it as me" are not one decision.
    NetWrite { domain: String, tool: String },
    /// Spends money. Always asks, always shows the figure, never remembered.
    Spend { cents_estimate: u32 },
    /// Runs code. Always asks, and the code is shown as it will run.
    Execute,
}

/// What the user said.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Answer {
    /// Yes, this time. Nothing is written down.
    Once,
    /// Yes, and stop asking — for this exact scope, until it expires.
    Always,
    /// No, and stop asking. Kept until the user changes their mind.
    Never,
}

/// What to do with a capability, having consulted the ledger.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Decision {
    /// Go ahead without asking.
    Allow,
    /// Stop and put the question to the user.
    Ask,
    /// The user has already said no to this. Do not ask again.
    Refuse,
}

/// One decision the user made, written down.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Grant {
    /// What was decided about, as `scope_key` renders it.
    pub scope: String,
    /// Kept readable so the ledger can be audited by a person, not only parsed.
    pub about: String,
    pub answer: Answer,
    pub granted_at: String,
    /// When an `Always` stops counting. `None` for `Never`, which does not
    /// expire: a refusal that quietly lapses is a refusal nobody made.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
}

impl Capability {
    /// The string a grant is filed under.
    ///
    /// `Spend` and `Execute` deliberately have none. There is nothing to
    /// remember about them, because they are asked every time — see `decide`.
    pub fn scope_key(&self) -> Option<String> {
        match self {
            Capability::NetRead { domain } => Some(format!("net_read:{}", domain.to_lowercase())),
            Capability::NetWrite { domain, tool } => Some(format!(
                "net_write:{}:{}",
                domain.to_lowercase(),
                tool.to_lowercase()
            )),
            _ => None,
        }
    }

    /// One sentence, for the card and for the ledger.
    pub fn describe(&self) -> String {
        match self {
            Capability::VaultRead => "read your vault".to_string(),
            Capability::VaultWrite => "change a note in your vault".to_string(),
            Capability::VaultStructural => "change many files at once".to_string(),
            Capability::NetRead { domain } => format!("read from {domain}"),
            Capability::NetWrite { domain, tool } => format!("send something to {domain} ({tool})"),
            Capability::Spend { cents_estimate } => {
                format!("spend about {:.2} USD", *cents_estimate as f64 / 100.0)
            }
            Capability::Execute => "run code on this computer".to_string(),
        }
    }

    /// Whether an `Always` means anything here.
    ///
    /// It does not for money or for running code, and refusing to record one is
    /// better than recording one and ignoring it: a ledger that shows a
    /// permission which does not apply is a ledger that lies to the person
    /// reading it to decide what they have agreed to.
    pub fn can_be_remembered(&self) -> bool {
        self.scope_key().is_some()
    }
}

/// A question the run stopped to ask.
///
/// Kept on the run rather than in a queue somewhere, because the answer only
/// means anything in the context of what was being attempted. A consent prompt
/// detached from the work it was for is a dialog box asking about a stranger.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ask {
    pub tool: String,
    pub capability: Capability,
    /// The capability in one English sentence, for the audit log and for a
    /// fallback.
    ///
    /// The card does *not* show this. The app is bilingual and the sentence a
    /// person reads has to come from i18n, keyed on the capability — a
    /// Vietnamese sentence composed in Rust around an English fragment is the
    /// bug this field exists to not be.
    pub about: String,
    /// Whether "always" is on offer. False for money and for running code.
    pub can_be_remembered: bool,
    pub asked_at: String,
}

impl Ask {
    pub fn about(tool: &str, capability: &Capability, now: &str) -> Self {
        Ask {
            tool: tool.to_string(),
            about: capability.describe(),
            can_be_remembered: capability.can_be_remembered(),
            capability: capability.clone(),
            asked_at: now.to_string(),
        }
    }
}

/// Every decision the user has made on this device.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Ledger {
    #[serde(default)]
    pub grants: Vec<Grant>,
}

fn ledger_path(vault_path: &str) -> AppResult<std::path::PathBuf> {
    let dir = std::path::Path::new(vault_path).join(".synabit");
    std::fs::create_dir_all(&dir)
        .map_err(|e| AppError::General(format!("Failed to create .synabit: {e}")))?;
    Ok(dir.join("consent.json"))
}

/// Read what has been decided. An unreadable ledger is an empty one.
///
/// Empty, and therefore asking again — which is the safe direction. A ledger
/// that failed to parse and was treated as "everything allowed" would turn a
/// corrupt file into a silent grant.
pub fn load(vault_path: &str) -> Ledger {
    let Ok(path) = ledger_path(vault_path) else {
        return Ledger::default();
    };
    let Ok(content) = std::fs::read_to_string(&path) else {
        return Ledger::default();
    };
    serde_json::from_str(&content).unwrap_or_else(|e| {
        log::warn!("[Syn] Consent ledger is unreadable, asking again: {e}");
        Ledger::default()
    })
}

fn save(vault_path: &str, ledger: &Ledger) -> AppResult<()> {
    let path = ledger_path(vault_path)?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_string_pretty(ledger)?)?;
    std::fs::rename(&tmp, &path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        AppError::General(format!("Failed to write the consent ledger: {e}"))
    })
}

/// What to do about a capability, right now.
///
/// The rules, in the order they apply:
///
/// - Reading and writing the vault never ask. That is today's behaviour, stated
///   rather than changed, and it rests on everything in the vault being
///   recoverable.
/// - A recorded `Never` refuses, for anything. A person who has said no is not
///   asked again by a different route.
/// - Money and running code always ask, whatever the ledger says. There is no
///   scope under which "you already agreed once" should spend again.
/// - A live `Always` allows. An expired one does not, and asking again is the
///   whole reason it expires.
pub fn decide(capability: &Capability, ledger: &Ledger, now: &str) -> Decision {
    if matches!(
        capability,
        Capability::VaultRead | Capability::VaultWrite | Capability::VaultStructural
    ) {
        return Decision::Allow;
    }

    let scope = capability.scope_key();

    // A refusal is honoured even for the capabilities that otherwise always
    // ask. Saying "never run code" and then being asked to run code every time
    // is not a setting, it is a nag.
    let refused = ledger.grants.iter().any(|g| {
        g.answer == Answer::Never
            && match (&scope, capability) {
                (Some(key), _) => &g.scope == key,
                (None, Capability::Execute) => g.scope == "execute",
                (None, Capability::Spend { .. }) => g.scope == "spend",
                _ => false,
            }
    });
    if refused {
        return Decision::Refuse;
    }

    if !capability.can_be_remembered() {
        return Decision::Ask;
    }

    let Some(key) = scope else { return Decision::Ask };
    let allowed = ledger.grants.iter().any(|g| {
        g.scope == key
            && g.answer == Answer::Always
            && g.expires_at.as_deref().is_none_or(|when| when > now)
    });

    if allowed {
        Decision::Allow
    } else {
        Decision::Ask
    }
}

/// Write down an answer, when there is anything to write down.
///
/// `Once` records nothing: it was an answer about this moment, and a ledger
/// full of them would be a log pretending to be a set of permissions.
pub fn record(
    vault_path: &str,
    capability: &Capability,
    answer: Answer,
    now: chrono::DateTime<chrono::Utc>,
) -> AppResult<()> {
    if answer == Answer::Once {
        return Ok(());
    }

    // `Always` on something that cannot be remembered is refused rather than
    // stored and ignored. A card that offers it should not have been drawn, and
    // storing it would leave the ledger claiming a permission that never
    // applies.
    if answer == Answer::Always && !capability.can_be_remembered() {
        return Err(AppError::General(format!(
            "`{}` is asked every time and cannot be granted permanently",
            capability.describe()
        )));
    }

    let scope = capability.scope_key().unwrap_or_else(|| {
        match capability {
            Capability::Execute => "execute",
            Capability::Spend { .. } => "spend",
            _ => "unscoped",
        }
        .to_string()
    });

    let mut ledger = load(vault_path);
    // One answer per scope. The newest replaces the old, so changing one's mind
    // is a decision rather than an argument between two rows.
    ledger.grants.retain(|g| g.scope != scope);
    ledger.grants.push(Grant {
        scope,
        about: capability.describe(),
        answer,
        granted_at: now.to_rfc3339(),
        expires_at: (answer == Answer::Always)
            .then(|| (now + chrono::Duration::days(ALWAYS_LASTS_DAYS)).to_rfc3339()),
    });
    save(vault_path, &ledger)
}

/// Take back one decision, by scope.
pub fn revoke(vault_path: &str, scope: &str) -> AppResult<()> {
    let mut ledger = load(vault_path);
    let before = ledger.grants.len();
    ledger.grants.retain(|g| g.scope != scope);
    if ledger.grants.len() == before {
        return Ok(());
    }
    save(vault_path, &ledger)
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: &str = "2026-09-05T00:00:00Z";

    fn at(text: &str) -> chrono::DateTime<chrono::Utc> {
        chrono::DateTime::parse_from_rfc3339(text)
            .expect("a time")
            .with_timezone(&chrono::Utc)
    }

    fn vault() -> (tempfile::TempDir, String) {
        let dir = tempfile::tempdir().expect("temp vault");
        let path = dir.path().to_str().expect("utf8").to_string();
        (dir, path)
    }

    /// A grant must not travel between devices.
    ///
    /// N4, and the reason is not tidiness: allowing something on a laptop is
    /// not allowing it on a phone. The mechanism is that the file is a dotfile,
    /// which both the sync walker and the vault scan skip — so this asserts the
    /// property those two rules give, rather than trusting that the path was
    /// chosen with them in mind.
    #[test]
    fn the_ledger_is_somewhere_that_does_not_sync() {
        let (_dir, vault) = vault();
        record(&vault, &Capability::NetRead { domain: "example.com".into() }, Answer::Always, at(NOW))
            .expect("recorded");

        let path = ledger_path(&vault).expect("a path");
        let relative = path
            .strip_prefix(&vault)
            .expect("inside the vault")
            .to_str()
            .expect("utf8");

        assert!(
            crate::commands::nodes::is_in_unscanned_dir(relative),
            "the vault scan must skip it, or a grant becomes a node"
        );
        assert!(
            relative.split(['/', '\\']).any(|part| part.starts_with('.')),
            "sync skips dotfiles, and this must be one: {relative}"
        );
    }

    /// The vault capabilities never ask, which is today's behaviour written down.
    #[test]
    fn touching_the_vault_does_not_ask() {
        let ledger = Ledger::default();
        for capability in [
            Capability::VaultRead,
            Capability::VaultWrite,
            Capability::VaultStructural,
        ] {
            assert_eq!(decide(&capability, &ledger, NOW), Decision::Allow);
        }
    }

    /// Asked once, then remembered.
    #[test]
    fn a_host_allowed_once_is_not_asked_about_again() {
        let (_dir, vault) = vault();
        let reading = Capability::NetRead { domain: "api.example.com".into() };

        assert_eq!(decide(&reading, &load(&vault), NOW), Decision::Ask);
        record(&vault, &reading, Answer::Always, at(NOW)).expect("recorded");
        assert_eq!(decide(&reading, &load(&vault), NOW), Decision::Allow);

        // A different host is a different question.
        let elsewhere = Capability::NetRead { domain: "other.example.com".into() };
        assert_eq!(decide(&elsewhere, &load(&vault), NOW), Decision::Ask);
    }

    /// Reading from a host is not permission to post to it.
    #[test]
    fn permission_to_read_is_not_permission_to_send() {
        let (_dir, vault) = vault();
        record(
            &vault,
            &Capability::NetRead { domain: "example.com".into() },
            Answer::Always,
            at(NOW),
        )
        .expect("recorded");

        let sending = Capability::NetWrite {
            domain: "example.com".into(),
            tool: "post_message".into(),
        };
        assert_eq!(
            decide(&sending, &load(&vault), NOW),
            Decision::Ask,
            "`may read from this server` and `may post to it as me` are not one decision"
        );
    }

    /// `Once` is an answer about this moment and is not written down.
    #[test]
    fn saying_yes_this_time_leaves_no_permission_behind() {
        let (_dir, vault) = vault();
        let reading = Capability::NetRead { domain: "example.com".into() };

        record(&vault, &reading, Answer::Once, at(NOW)).expect("recorded");
        assert!(load(&vault).grants.is_empty(), "nothing was stored");
        assert_eq!(decide(&reading, &load(&vault), NOW), Decision::Ask, "so it asks again");
    }

    /// Money and code always ask, whatever is in the ledger.
    ///
    /// There is no scope under which "you agreed once" should spend again, and
    /// an `Always` for one is refused rather than stored and ignored — a ledger
    /// showing a permission that never applies lies to the person reading it to
    /// find out what they have agreed to.
    #[test]
    fn spending_and_running_code_are_asked_every_time() {
        let (_dir, vault) = vault();
        for capability in [Capability::Spend { cents_estimate: 250 }, Capability::Execute] {
            assert!(!capability.can_be_remembered());
            assert!(
                record(&vault, &capability, Answer::Always, at(NOW)).is_err(),
                "an always for `{}` should be refused",
                capability.describe()
            );
            assert_eq!(decide(&capability, &load(&vault), NOW), Decision::Ask);
        }
    }

    /// A refusal sticks, including for the ones that otherwise always ask.
    ///
    /// Saying "never run code" and then being asked every time is not a
    /// setting, it is a nag.
    #[test]
    fn a_no_is_remembered_and_stops_the_asking() {
        let (_dir, vault) = vault();

        record(&vault, &Capability::Execute, Answer::Never, at(NOW)).expect("recorded");
        assert_eq!(decide(&Capability::Execute, &load(&vault), NOW), Decision::Refuse);

        let host = Capability::NetWrite { domain: "example.com".into(), tool: "post".into() };
        record(&vault, &host, Answer::Never, at(NOW)).expect("recorded");
        assert_eq!(decide(&host, &load(&vault), NOW), Decision::Refuse);
    }

    /// An `Always` expires, and a `Never` does not.
    ///
    /// A permission granted once and never revisited is indistinguishable from
    /// one nobody chose. A refusal that lapses is a refusal nobody made.
    #[test]
    fn permission_expires_and_refusal_does_not() {
        let (_dir, vault) = vault();
        let reading = Capability::NetRead { domain: "example.com".into() };

        record(&vault, &reading, Answer::Always, at(NOW)).expect("recorded");
        let ledger = load(&vault);
        assert_eq!(decide(&reading, &ledger, NOW), Decision::Allow);
        assert_eq!(
            decide(&reading, &ledger, "2027-01-01T00:00:00Z"),
            Decision::Ask,
            "ninety days later it is a question again"
        );

        record(&vault, &Capability::Execute, Answer::Never, at(NOW)).expect("recorded");
        let ledger = load(&vault);
        assert!(
            ledger.grants.iter().find(|g| g.scope == "execute").expect("it").expires_at.is_none()
        );
        assert_eq!(
            decide(&Capability::Execute, &ledger, "2099-01-01T00:00:00Z"),
            Decision::Refuse
        );
    }

    /// Changing one's mind replaces the answer rather than arguing with it.
    #[test]
    fn a_second_answer_replaces_the_first() {
        let (_dir, vault) = vault();
        let reading = Capability::NetRead { domain: "example.com".into() };

        record(&vault, &reading, Answer::Always, at(NOW)).expect("yes");
        record(&vault, &reading, Answer::Never, at(NOW)).expect("no");

        let ledger = load(&vault);
        assert_eq!(ledger.grants.len(), 1, "one answer per scope");
        assert_eq!(decide(&reading, &ledger, NOW), Decision::Refuse);
    }

    /// A host is a host whatever case it was written in.
    #[test]
    fn a_domain_is_matched_without_regard_to_case() {
        let (_dir, vault) = vault();
        record(
            &vault,
            &Capability::NetRead { domain: "API.Example.COM".into() },
            Answer::Always,
            at(NOW),
        )
        .expect("recorded");

        assert_eq!(
            decide(
                &Capability::NetRead { domain: "api.example.com".into() },
                &load(&vault),
                NOW
            ),
            Decision::Allow
        );
    }

    /// A ledger that will not parse asks again rather than allowing.
    #[test]
    fn an_unreadable_ledger_is_an_empty_one() {
        let (_dir, vault) = vault();
        let path = ledger_path(&vault).expect("a path");
        std::fs::write(&path, "{ this is not json").expect("written");

        assert!(load(&vault).grants.is_empty());
        assert_eq!(
            decide(&Capability::NetRead { domain: "x.com".into() }, &load(&vault), NOW),
            Decision::Ask,
            "a corrupt file must not read as a grant"
        );
    }

    /// Taking a permission back works, and is quiet about one that was not there.
    #[test]
    fn a_permission_can_be_taken_back() {
        let (_dir, vault) = vault();
        let reading = Capability::NetRead { domain: "example.com".into() };
        record(&vault, &reading, Answer::Always, at(NOW)).expect("recorded");

        revoke(&vault, &reading.scope_key().expect("a key")).expect("revoked");
        assert_eq!(decide(&reading, &load(&vault), NOW), Decision::Ask);

        revoke(&vault, "net_read:never-granted.com").expect("a no-op is not an error");
    }
}
