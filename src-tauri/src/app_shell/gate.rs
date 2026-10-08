//! The one place every call from a webview to this app's commands passes
//! through, and what it refuses.
//!
//! # Why here and not the capability files
//!
//! It was believed the capability files closed this, and they do not. They
//! govern the **ACL**, and `webview/mod.rs` consults the ACL only for plugin
//! commands or for an app that ships its own ACL manifest:
//!
//! ```text
//! if (plugin_command.is_some() || has_app_acl_manifest) && invoke.acl.is_none()
//! ```
//!
//! The app's own commands are neither. `gen/schemas/acl-manifests.json` has no
//! `__app-acl__` key, so the check is skipped and the command runs — from any
//! webview, since `__TAURI_INTERNALS__.invoke` and its key are injected into
//! every one, remote pages included. The proper repair is an app ACL manifest:
//! a permission and a grant for every command. Until somebody writes that,
//! this is the lock, wrapped around the generated handler in `lib.rs` so that
//! a command is covered the day it is added rather than the day somebody
//! remembers. An ACL arriving later makes it redundant rather than wrong.
//!
//! # What it asks, in order
//!
//! 1. **May this webview call this command at all?** An allowlist per webview
//!    label ([`may_call`]): the app's window everything, each panel what its
//!    screen calls, the browsing pane one command, anything else nothing.
//! 2. **Is the vault it names the open one?** Every `vault_path` argument must
//!    name [`super::vault`]'s vault. One rule here instead of a check in the
//!    two hundred commands that take one.
//! 3. **Was the path it writes to picked in a dialog?** For the commands in
//!    [`CHOSEN_ARGS`], the argument must be a path [`super::dialogs`] put up a
//!    dialog for, and it is spent by the call.
//!
//! The first refusal wins, and nothing is spent on a call that is refused.

use serde_json::{Map, Value};

use super::vault::Open;
use super::MAIN_WINDOW;
use crate::error::AppError;
use crate::syn::browser::{THE_ONE_DOOR, WINDOW as BROWSING_PANE};

/// The capture panel the global hotkey opens (`build_quick_entry_window`).
pub const QUICK_ENTRY: &str = "quick-entry";
/// Safe's Quick Access panel (`build_safe_quick_window`).
pub const SAFE_QUICK: &str = "safe-quick";

/// What `QuickEntry.vue` calls, and `useSynEnabled` for it. A box for one
/// sentence: it queues a capture or hands a question to the main window.
/// `quick_entry_calls_only_what_it_may` keeps this in step with the screen.
pub const QUICK_ENTRY_MAY: &[&str] = &["queue_capture", "ask_syn_from_quick_entry", "syn_get_settings"];

/// What `SafeQuick.vue` calls: find an item and copy from it. Not the rest of
/// Safe — no reveal, no edit, no export — which `commands::safe::gate` would
/// otherwise let this window reach.
pub const SAFE_QUICK_MAY: &[&str] = &["safe_status", "safe_list", "safe_unlock", "safe_copy_primary"];

/// Whether this webview may call this command.
///
/// Keyed on the **webview** label rather than the window's. The browsing view
/// is its own webview whether it sits in its own window or docked inside the
/// main one, and a rule written against the window label would quietly stop
/// applying the day it moved.
///
/// An unknown label is refused everything. A webview added later starts with
/// nothing and is given what it needs, rather than starting with everything.
/// That includes the `node_*` windows `commands::safe` still names: nothing
/// opens one today, and the day something does, it is added here on purpose.
pub fn may_call(webview_label: &str, command: &str) -> bool {
    match webview_label {
        MAIN_WINDOW => true,
        QUICK_ENTRY => QUICK_ENTRY_MAY.contains(&command),
        SAFE_QUICK => SAFE_QUICK_MAY.contains(&command),
        BROWSING_PANE => command == THE_ONE_DOOR,
        _ => false,
    }
}

/// Commands whose `vault_path` is checked by the command itself, because the
/// argument is how a vault comes to be open at all.
///
/// Only `start_vault_watcher`: with a vault open it must name that vault, and
/// with none it may open a folder that is already one (`ActiveVault::claim`).
/// Every other way to open a vault takes no path from the page — the picker
/// opens its own dialog, the phone decides for itself, startup reads Rust's
/// own record. Restoring a backup restores *into* the open vault, so it is not
/// here; there is no flow that creates a vault at a path the page names.
pub const VAULT_CHECKED_INSIDE: &[&str] = &["start_vault_watcher"];

/// Arguments that must be a path the person just picked in a dialog this
/// process opened, as `(command, argument as the webview spells it)`.
///
/// Every write to a place outside the vault that a page could otherwise name.
/// The backup, its restore and the diagnostics export are not here because
/// they open their dialogs themselves and take no path.
pub const CHOSEN_ARGS: &[(&str, &str)] = &[
    ("export_table_xlsx", "destination"),
    ("export_calendar_ics", "destination"),
    ("export_contacts", "destination"),
    ("safe_save_emergency_kit", "path"),
    ("safe_export", "path"),
    ("safe_export_plain", "path"),
    ("add_file_source", "path"),
];

/// Why a call was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    /// This webview may not call this command.
    Window,
    /// The call names a vault other than the open one, or none is open.
    Vault,
    /// A path that has to come from a dialog did not.
    NotChosen(&'static str),
}

impl Refusal {
    pub fn into_error(self, command: &str) -> AppError {
        match self {
            Refusal::Window => AppError::General(format!("`{command}` cannot be called from this window")),
            Refusal::Vault => AppError::InvalidPath(super::vault::NOT_OPEN.into()),
            Refusal::NotChosen(arg) => AppError::InvalidPath(format!(
                "`{command}` writes only where a file dialog was answered, and `{arg}` was not"
            )),
        }
    }
}

/// The decision, from what the call carries. `args` is `None` for a call
/// whose body is bytes rather than JSON, which cannot carry a named argument
/// (Tauri refuses to deserialise one from a byte payload).
///
/// `take_chosen` is asked last and only once everything else has passed, so a
/// refused call does not spend somebody's pick.
pub fn judge(
    webview_label: &str,
    command: &str,
    args: Option<&Map<String, Value>>,
    open: Option<&Open>,
    take_chosen: impl FnOnce(&str) -> bool,
) -> Result<(), Refusal> {
    if !may_call(webview_label, command) {
        return Err(Refusal::Window);
    }
    let Some(args) = args else {
        return Ok(());
    };

    if !VAULT_CHECKED_INSIDE.contains(&command) {
        // Both spellings: Tauri asks for `vaultPath` unless a command says
        // `rename_all = "snake_case"`, and none does today — but a command
        // that did would otherwise slip past without a word.
        for key in ["vaultPath", "vault_path"] {
            match args.get(key) {
                None | Some(Value::Null) => {}
                Some(Value::String(claimed)) => {
                    if !open.is_some_and(|open| open.is(claimed)) {
                        return Err(Refusal::Vault);
                    }
                }
                Some(_) => return Err(Refusal::Vault),
            }
        }
    }

    if let Some((_, arg)) = CHOSEN_ARGS.iter().find(|(c, _)| *c == command) {
        let picked = match args.get(*arg) {
            Some(Value::String(path)) => take_chosen(path),
            _ => false,
        };
        if !picked {
            return Err(Refusal::NotChosen(arg));
        }
    }

    Ok(())
}

/// [`judge`], for a call as Tauri delivers it.
pub fn check<R: tauri::Runtime>(invoke: &tauri::ipc::Invoke<R>) -> Result<(), AppError> {
    let message = &invoke.message;
    let command = message.command();
    let args = match message.payload() {
        tauri::ipc::InvokeBody::Json(Value::Object(map)) => Some(map),
        _ => None,
    };
    let open = super::vault::global().current();
    judge(message.webview_ref().label(), command, args, open.as_ref(), |path| {
        super::dialogs::chosen().take(path)
    })
    .map_err(|refusal| {
        log::warn!(
            "[gate] refused `{command}` from webview '{}': {refusal:?}",
            message.webview_ref().label()
        );
        refusal.into_error(command)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tempfile::TempDir;

    fn args(value: Value) -> Map<String, Value> {
        value.as_object().expect("an object").clone()
    }

    fn vault() -> (TempDir, Open) {
        let dir = TempDir::new().unwrap();
        std::fs::create_dir_all(dir.path().join(".synabit")).unwrap();
        let open = Open::resolve(&dir.path().to_string_lossy()).unwrap();
        (dir, open)
    }

    fn never(_: &str) -> bool {
        false
    }

    // ── which window ───────────────────────────────────────────────

    #[test]
    fn the_app_window_may_call_anything() {
        for command in ["trash_node", "syn_send_message", "safe_export_plain", THE_ONE_DOOR] {
            assert!(may_call(MAIN_WINDOW, command), "main lost {command}");
        }
    }

    #[test]
    fn the_panels_may_call_only_what_their_screens_call() {
        assert!(may_call(QUICK_ENTRY, "queue_capture"));
        assert!(may_call(SAFE_QUICK, "safe_copy_primary"));
        for command in ["trash_node", "read_local_file_content", "export_diagnostics", "safe_reveal"] {
            assert!(!may_call(QUICK_ENTRY, command), "quick-entry could call {command}");
        }
        for command in ["safe_reveal", "safe_export_plain", "safe_get", "trash_node", "queue_capture"] {
            assert!(!may_call(SAFE_QUICK, command), "safe-quick could call {command}");
        }
    }

    #[test]
    fn a_webview_nobody_named_may_call_nothing() {
        for label in ["", "ask-bar", "node_abc", "Main", "main ", "syn-browser-2"] {
            for command in ["queue_capture", "trash_node", THE_ONE_DOOR] {
                assert!(!may_call(label, command), "{label:?} could call {command}");
            }
        }
    }

    /// The panels' lists are what their screens actually invoke. A command
    /// added to `QuickEntry.vue` and not here fails in the panel at once,
    /// silently to anybody not reading the log — so it fails here first.
    #[test]
    fn quick_entry_calls_only_what_it_may() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../src");
        let mut seen = 0;
        for file in ["QuickEntry.vue", "shared/syn/useSynEnabled.ts"] {
            let source = std::fs::read_to_string(root.join(file)).expect("the screen is readable");
            for name in invoked(&source) {
                seen += 1;
                assert!(QUICK_ENTRY_MAY.contains(&name.as_str()), "{file} calls {name}, which quick-entry may not");
            }
        }
        assert!(seen >= 3, "read only {seen} calls; the parse broke");
    }

    #[test]
    fn safe_quick_calls_only_what_it_may() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../src");
        let screen = std::fs::read_to_string(root.join("SafeQuick.vue")).expect("readable");
        let api = std::fs::read_to_string(root.join("mini-apps/safe/api.ts")).expect("readable");

        let mut names = invoked(&screen);
        // Through `useSafeApi`: `api.status()` is the entry `status: () => invoke('safe_status', …)`,
        // which may wrap onto the next line.
        let lines: Vec<&str> = api.lines().collect();
        for method in screen.split("api.").skip(1).filter_map(|rest| rest.split('(').next()) {
            if method.is_empty() || !method.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                continue;
            }
            let at = lines
                .iter()
                .position(|l| l.trim_start().starts_with(&format!("{method}:")) && l.contains("=>"))
                .unwrap_or_else(|| panic!("api.{method} is not in api.ts"));
            let entry = if lines[at].contains("invoke") {
                lines[at].to_string()
            } else {
                format!("{} {}", lines[at], lines.get(at + 1).unwrap_or(&""))
            };
            names.extend(invoked(&entry));
        }
        assert!(names.len() >= 4, "read only {names:?}; the parse broke");
        for name in names {
            assert!(SAFE_QUICK_MAY.contains(&name.as_str()), "SafeQuick.vue calls {name}, which safe-quick may not");
        }
    }

    /// `invoke('name'` and `invoke<T>('name'`, the forms the front end uses.
    fn invoked(source: &str) -> Vec<String> {
        source
            .split("invoke")
            .skip(1)
            .filter_map(|rest| {
                let rest = rest.trim_start();
                let rest = if rest.starts_with('<') { &rest[rest.find(">(")? + 1..] } else { rest };
                let rest = rest.strip_prefix('(')?.trim_start();
                let quote = rest.chars().next().filter(|c| *c == '\'' || *c == '"')?;
                Some(rest[1..].split(quote).next()?.to_string())
            })
            .collect()
    }

    // ── which vault ────────────────────────────────────────────────

    #[test]
    fn a_call_naming_the_open_vault_passes() {
        let (dir, open) = vault();
        let a = args(json!({ "vaultPath": dir.path().to_string_lossy(), "relPath": "a.md" }));
        assert_eq!(judge(MAIN_WINDOW, "read_file", Some(&a), Some(&open), never), Ok(()));
    }

    /// The hole this closes: `vaultPath: "/"` read any file under 5 MB.
    #[test]
    fn a_call_naming_another_folder_is_refused() {
        let (_dir, open) = vault();
        let (other, _) = vault();
        for claimed in ["/", "", "relative/path", &other.path().to_string_lossy()] {
            let a = args(json!({ "vaultPath": claimed, "path": "/etc/passwd" }));
            assert_eq!(
                judge(MAIN_WINDOW, "read_local_file_content", Some(&a), Some(&open), never),
                Err(Refusal::Vault),
                "{claimed:?} passed"
            );
        }
        let snake = args(json!({ "vault_path": "/" }));
        assert_eq!(judge(MAIN_WINDOW, "read_file", Some(&snake), Some(&open), never), Err(Refusal::Vault));
        let not_a_string = args(json!({ "vaultPath": ["/"] }));
        assert_eq!(judge(MAIN_WINDOW, "read_file", Some(&not_a_string), Some(&open), never), Err(Refusal::Vault));
    }

    #[test]
    fn with_no_vault_open_no_vault_is_named() {
        let (dir, _) = vault();
        let a = args(json!({ "vaultPath": dir.path().to_string_lossy() }));
        assert_eq!(judge(MAIN_WINDOW, "scan_all_nodes", Some(&a), None, never), Err(Refusal::Vault));
    }

    /// Commands without a vault, or with an optional one left out, are not
    /// this rule's business.
    #[test]
    fn a_call_naming_no_vault_is_left_alone() {
        let none = args(json!({ "text": "hello" }));
        let null = args(json!({ "vaultPath": null }));
        assert_eq!(judge(MAIN_WINDOW, "queue_capture", Some(&none), None, never), Ok(()));
        assert_eq!(judge(MAIN_WINDOW, "get_family_safe", Some(&null), None, never), Ok(()));
        assert_eq!(judge(MAIN_WINDOW, "anything", None, None, never), Ok(()));
    }

    #[test]
    fn only_the_watcher_checks_its_own_vault() {
        assert_eq!(VAULT_CHECKED_INSIDE, &["start_vault_watcher"]);
        let a = args(json!({ "vaultPath": "/somewhere/new" }));
        assert_eq!(judge(MAIN_WINDOW, "start_vault_watcher", Some(&a), None, never), Ok(()));
    }

    /// The panels are held to the vault too: a panel is a webview a page could
    /// end up in as easily as the main one.
    #[test]
    fn the_panels_are_held_to_the_open_vault() {
        let (dir, open) = vault();
        let good = args(json!({ "vaultPath": dir.path().to_string_lossy(), "id": "x", "what": "password" }));
        let bad = args(json!({ "vaultPath": "/", "id": "x", "what": "password" }));
        assert_eq!(judge(SAFE_QUICK, "safe_copy_primary", Some(&good), Some(&open), never), Ok(()));
        assert_eq!(judge(SAFE_QUICK, "safe_copy_primary", Some(&bad), Some(&open), never), Err(Refusal::Vault));
    }

    // ── which file ─────────────────────────────────────────────────

    #[test]
    fn an_export_writes_only_where_a_dialog_was_answered() {
        let chosen = super::super::dialogs::Chosen::new();
        chosen.remember("/Users/a/calendar.ics");

        let typed = args(json!({ "destination": "/Users/a/.zshrc" }));
        assert_eq!(
            judge(MAIN_WINDOW, "export_calendar_ics", Some(&typed), None, |p| chosen.take(p)),
            Err(Refusal::NotChosen("destination"))
        );
        let missing = args(json!({}));
        assert_eq!(
            judge(MAIN_WINDOW, "export_calendar_ics", Some(&missing), None, |p| chosen.take(p)),
            Err(Refusal::NotChosen("destination"))
        );

        let picked = args(json!({ "destination": "/Users/a/calendar.ics" }));
        assert_eq!(judge(MAIN_WINDOW, "export_calendar_ics", Some(&picked), None, |p| chosen.take(p)), Ok(()));
        assert_eq!(
            judge(MAIN_WINDOW, "export_calendar_ics", Some(&picked), None, |p| chosen.take(p)),
            Err(Refusal::NotChosen("destination")),
            "one pick, one write"
        );
    }

    /// A pick is not spent by a call that was going to be refused anyway.
    #[test]
    fn a_refused_call_does_not_spend_the_pick() {
        let (_dir, open) = vault();
        let chosen = super::super::dialogs::Chosen::new();
        chosen.remember("/Users/a/people.vcf");
        let wrong_vault = args(json!({ "vaultPath": "/", "destination": "/Users/a/people.vcf" }));
        assert_eq!(
            judge(MAIN_WINDOW, "export_contacts", Some(&wrong_vault), Some(&open), |p| chosen.take(p)),
            Err(Refusal::Vault)
        );
        assert!(chosen.take("/Users/a/people.vcf"));
    }

    #[test]
    fn a_folder_source_is_one_the_person_picked() {
        let (dir, open) = vault();
        let a = args(json!({ "vaultPath": dir.path().to_string_lossy(), "path": "/", "name": "root" }));
        assert_eq!(
            judge(MAIN_WINDOW, "add_file_source", Some(&a), Some(&open), never),
            Err(Refusal::NotChosen("path"))
        );
    }

    /// Every command named in `CHOSEN_ARGS` exists, so a rename cannot leave
    /// the rule guarding a name nothing calls.
    #[test]
    fn every_guarded_command_is_registered() {
        let lib = include_str!("../lib.rs");
        let guarded = CHOSEN_ARGS.iter().map(|(c, _)| c);
        for command in guarded.chain(VAULT_CHECKED_INSIDE).chain(QUICK_ENTRY_MAY).chain(SAFE_QUICK_MAY) {
            let registered = lib.contains(&format!("::{command},")) || lib.contains(&format!(" {command},\n"));
            assert!(registered, "{command} is not registered in lib.rs");
        }
    }
}
