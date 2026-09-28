//! "This device": the secrets Synabit keeps in this machine's keychain, beside
//! the Safe rather than in it.
//!
//! They live there because they must work while the Safe is locked — the
//! Telegram bot answers while its owner is away, connectors connect at start
//! (section 6.8 of the design). What this adds is that they are *seen*: until
//! now a provider key or a connector's token, once pasted into a settings
//! field, was invisible for the life of the machine.
//!
//! Names only, never values. Forgetting is offered where nothing else manages
//! the secret on its own screen; the sync key and the app-lock PIN have their
//! own, and forgetting them from here would bypass what those screens say
//! first.

use serde::Serialize;

use crate::secrets::AppSecrets;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    SyncKey,
    AppLockPin,
    Provider,
    Telegram,
    Connector,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DeviceSecret {
    /// The keychain slot, for forgetting it. Not a secret.
    pub slot: String,
    pub kind: Kind,
    /// What to call it: a provider's name, a connector's name and the header
    /// or variable the secret fills.
    pub name: String,
    pub forgettable: bool,
}

/// What `secrets` holds, described. `connector_name` turns a connector's id
/// into what the person called it, when the vault still has it.
pub fn describe(secrets: &AppSecrets, connector_name: impl Fn(&str) -> Option<String>) -> Vec<DeviceSecret> {
    let mut out = Vec::new();
    if secrets.e2ee_key.is_some() || secrets.e2ee_password.is_some() {
        out.push(DeviceSecret { slot: String::new(), kind: Kind::SyncKey, name: "sync".into(), forgettable: false });
    }
    if secrets.app_lock_hash.is_some() {
        out.push(DeviceSecret { slot: String::new(), kind: Kind::AppLockPin, name: "app lock".into(), forgettable: false });
    }
    let mut slots: Vec<&String> = secrets.syn_api_keys.iter().filter(|(_, v)| !v.trim().is_empty()).map(|(k, _)| k).collect();
    slots.sort();
    for slot in slots {
        let entry = if slot == crate::syn::telegram::TOKEN_SLOT {
            DeviceSecret { slot: slot.clone(), kind: Kind::Telegram, name: "Telegram".into(), forgettable: false }
        } else if let Some(rest) = slot
            .strip_prefix(crate::syn::connector::config::SLOT_PREFIX)
            .or_else(|| slot.strip_prefix(crate::syn::connector::config::LEGACY_SLOT_PREFIX))
        {
            let mut parts = rest.splitn(3, ':');
            let (id, _kind, key) = (parts.next().unwrap_or(""), parts.next().unwrap_or(""), parts.next().unwrap_or(""));
            let server = connector_name(id).unwrap_or_else(|| id.to_string());
            DeviceSecret { slot: slot.clone(), kind: Kind::Connector, name: format!("{server} · {key}"), forgettable: true }
        } else {
            let name = match slot.as_str() {
                "anthropic" => "Anthropic",
                "gemini" => "Gemini",
                "openai_compat" => "OpenAI-compatible",
                "ollama" => "Ollama",
                other => other,
            };
            DeviceSecret { slot: slot.clone(), kind: Kind::Provider, name: name.into(), forgettable: true }
        };
        out.push(entry);
    }
    out
}

/// Whether `slot` may be forgotten from this screen.
pub fn may_forget(secrets: &AppSecrets, slot: &str) -> bool {
    describe(secrets, |_| None).iter().any(|s| s.slot == slot && s.forgettable)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn secrets() -> AppSecrets {
        let mut s = AppSecrets { e2ee_key: Some("k".into()), app_lock_hash: Some("h".into()), ..Default::default() };
        s.syn_api_keys.insert("anthropic".into(), "sk-ant-canary".into());
        s.syn_api_keys.insert("telegram_bot".into(), "123:canary".into());
        s.syn_api_keys.insert("connector:abc:header:Authorization".into(), "Bearer canary".into());
        s.syn_api_keys.insert("gemini".into(), "  ".into());
        s
    }

    #[test]
    fn names_everything_and_no_value() {
        let described = describe(&secrets(), |id| (id == "abc").then(|| "Linear".to_string()));
        let json = serde_json::to_string(&described).unwrap();
        assert!(!json.contains("canary"), "{json}");
        let names: Vec<&str> = described.iter().map(|d| d.name.as_str()).collect();
        assert_eq!(names, ["sync", "app lock", "Anthropic", "Linear · Authorization", "Telegram"]);
        assert!(!names.contains(&"Gemini"), "a blank key is no key");
    }

    #[test]
    fn only_what_no_other_screen_manages_can_be_forgotten() {
        let s = secrets();
        assert!(may_forget(&s, "anthropic"));
        assert!(may_forget(&s, "connector:abc:header:Authorization"));
        assert!(!may_forget(&s, "telegram_bot"), "Telegram has its own screen, which also stops the bot");
        assert!(!may_forget(&s, ""), "the sync key and PIN are never forgotten from here");
        assert!(!may_forget(&s, "nonexistent"));
    }
}
