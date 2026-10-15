//! What each built-in tool may do, where, and when — in one table.
//!
//! # Why one table
//!
//! The answers used to live in a dozen lists, each beside the code that asked:
//! the capability `match` in `registry`, the undo sentences beside it, two
//! lists in `taint`, three in `surface`, a rule in `delegate`, one in `recipe`,
//! one per app in `locks`, the groups in `toolset`, and the names `tools.rs`
//! hands to the engine. Only two of them were checked against each other, and
//! the drift was real: `stage_transactions` was declared a read while it wrote
//! a file, which let it through plan mode, helpers and Telegram's read rule,
//! and it was missing from the Finance lock.
//!
//! Now a tool is one row. Adding a tool without a row fails a test; a row
//! without a tool fails another. The rows read top to bottom as the policy,
//! and the modules that used to keep a list ask here.
//!
//! # What is not here
//!
//! Connector tools (`connector__<slug>__<op>`). Their power comes from a
//! server or a manifest at run time, not from this file — see
//! `connector::provider::capability_for` and `connector::declared::capability`.
//! Every question below answers a connector tool the strict way: not after
//! reading, not by a helper unless it only reads, the app only.

use serde_json::Value;

use crate::syn::consent::Capability;
use crate::syn::registry::Reversal;
use crate::syn::surface::Surface;
use crate::syn::toolset::Group;

/// The power a tool needs, as far as this table can say without its arguments.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Power {
    /// Reads the vault, or touches nothing at all (a plan, a group of tools).
    Read,
    /// Writes to the vault; undone from inside the app.
    Write,
    /// Reshapes every note of a kind.
    Structural,
    /// Goes to the web.
    Browse,
    /// Writes a draft into the vault — or, given a spec's address, reads that
    /// host first. The one tool whose power depends on what it was given.
    DraftFromSpec,
}

/// Which group brings a tool into a run — `toolset::Group` without the
/// connector variant, so a row can be a constant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Home {
    Core,
    Finance,
    Feeds,
    Files,
    Boards,
    Timeline,
    History,
    Structure,
    Past,
    Safe,
    Tables,
    Compute,
    Writes,
    Authoring,
    Skills,
    Jobs,
}

impl Home {
    pub fn group(self) -> Group {
        match self {
            Home::Core => Group::Core,
            Home::Finance => Group::Finance,
            Home::Feeds => Group::Feeds,
            Home::Files => Group::Files,
            Home::Boards => Group::Boards,
            Home::Timeline => Group::Timeline,
            Home::History => Group::History,
            Home::Structure => Group::Structure,
            Home::Past => Group::Past,
            Home::Safe => Group::Safe,
            Home::Tables => Group::Tables,
            Home::Compute => Group::Compute,
            Home::Writes => Group::Writes,
            Home::Authoring => Group::Authoring,
            Home::Skills => Group::Skills,
            Home::Jobs => Group::Jobs,
        }
    }
}

/// What happens to a call once the run has read somebody else's words.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AfterReading {
    /// It goes, as before: reading, and making something new.
    Allow,
    /// It is held for the person to approve when the run is done — a change
    /// to what was already there, which something read may have asked for.
    /// In the app only; elsewhere there is no card to hold it on, and it is
    /// refused. See `syn::changes`.
    Stage,
    /// It is refused.
    Refuse,
}

/// One built-in tool's row.
#[derive(Debug, Clone, Copy)]
pub struct Policy {
    pub name: &'static str,
    /// Which group brings it into a run. See `toolset`.
    pub group: Home,
    pub power: Power,
    /// What undoing it looks like, where the sentence for its power would be
    /// untrue of it. See `registry::reversal_of`.
    pub undo: Option<&'static str>,
    /// What it returns is somebody else's words: reading it taints the run.
    pub outside_words: bool,
    /// What happens to it once the run has read somebody else's words.
    pub after_reading: AfterReading,
    pub app: bool,
    pub telegram: bool,
    pub routine: bool,
    /// A helper run (`delegate`) may use it.
    pub helper: bool,
    /// The app whose PIN lock hides it. See `locks`.
    pub lock: Option<&'static str>,
    /// Run by the engine rather than `tools::execute_tool`: it is async, or it
    /// stops the run, or it changes the run itself.
    pub engine: bool,
}

impl Policy {
    /// Reading: everywhere, after reading, in a helper and a recipe.
    const fn read(name: &'static str, group: Home) -> Policy {
        Policy {
            name,
            group,
            power: Power::Read,
            undo: None,
            outside_words: false,
            after_reading: AfterReading::Allow,
            app: true,
            telegram: true,
            routine: true,
            helper: true,
            lock: None,
            engine: false,
        }
    }

    /// Writing: the app only, not after reading, not in a helper.
    const fn write(name: &'static str, group: Home) -> Policy {
        Policy {
            name,
            group,
            power: Power::Write,
            undo: None,
            outside_words: false,
            after_reading: AfterReading::Refuse,
            app: true,
            telegram: false,
            routine: false,
            helper: false,
            lock: None,
            engine: false,
        }
    }

    /// Reshaping a kind: the app only. Never a recipe step either — see
    /// `recipe::problems`.
    const fn structural(name: &'static str) -> Policy {
        Policy { power: Power::Structural, ..Policy::write(name, Home::Structure) }
    }

    /// The web: not from Telegram, which has no browser.
    const fn browse(name: &'static str) -> Policy {
        Policy { power: Power::Browse, telegram: false, ..Policy::read(name, Home::Core) }
    }

    const fn outside(self) -> Policy {
        Policy { outside_words: true, ..self }
    }
    const fn after_reading(self) -> Policy {
        Policy { after_reading: AfterReading::Allow, ..self }
    }
    /// Held for the person after reading, rather than refused.
    const fn staged(self) -> Policy {
        Policy { after_reading: AfterReading::Stage, ..self }
    }
    const fn telegram(self) -> Policy {
        Policy { telegram: true, ..self }
    }
    const fn routine(self) -> Policy {
        Policy { routine: true, ..self }
    }
    /// Reached only in the app, where its card can be shown and its
    /// connectors live.
    const fn app_only(self) -> Policy {
        Policy { telegram: false, routine: false, ..self }
    }
    const fn not_in_app(self) -> Policy {
        Policy { app: false, ..self }
    }
    const fn no_helper(self) -> Policy {
        Policy { helper: false, ..self }
    }
    const fn locked_with(self, app: &'static str) -> Policy {
        Policy { lock: Some(app), ..self }
    }
    const fn undone_by(self, how: &'static str) -> Policy {
        Policy { undo: Some(how), ..self }
    }
    const fn engine(self) -> Policy {
        Policy { engine: true, ..self }
    }
}

use crate::syn::connector::authoring::{DRAFT_TOOL, FIND_TOOL as CONNECTOR_FIND, PROPOSE_TOOL as CONNECTOR_CONNECT, TRY_TOOL};
use crate::syn::tools::{BROWSE_TOOL, CAPTURE_TOOL, LOOK_BACK_TOOL, PLAN_TOOL};

/// Every built-in tool.
pub static POLICIES: &[Policy] = &[
    // ── Reading the vault ──────────────────────────────────────────
    Policy::read("query_nodes", Home::Core),
    Policy::read("get_node", Home::Core),
    Policy::read("list_schemas", Home::Core),
    Policy::read("get_linked_nodes", Home::Core),
    Policy::read(crate::syn::skill::LOAD_TOOL, Home::Core),
    Policy::read("timeline", Home::Timeline),
    Policy::read("recall", Home::Past),
    Policy::read(LOOK_BACK_TOOL, Home::Past),
    Policy::read("list_trash", Home::History),
    Policy::read("list_versions", Home::History),
    Policy::read("read_board", Home::Boards).locked_with("whiteboard"),
    Policy::read("get_finance_summary", Home::Finance).locked_with("finance"),
    Policy::read("search_finance", Home::Finance).locked_with("finance"),
    Policy::read("get_transactions", Home::Finance).locked_with("finance"),
    // Names only — never a value — and what Syn may know of is the user's
    // choice per item. Reading a page cannot make a name mean more.
    Policy::read("safe_list", Home::Safe).locked_with("safe"),
    Policy::read("safe_health", Home::Safe).locked_with("safe"),
    // Arithmetic over a table this conversation already brought back. Reaches
    // nothing and writes nothing — and a table read from outside is exactly
    // what it is for, so refusing it after reading would refuse its point.
    Policy::read(crate::syn::crunch::TOOL, Home::Compute),
    // Code with no edge to anything: in are the tables named and the data
    // passed, out is a value. Reading from outside cannot make it reach
    // further, so it is everywhere a read is. See `syn::script`.
    Policy::read(crate::syn::script::TOOL, Home::Compute),

    // ── Reading somebody else's words ──────────────────────────────
    // A summary is as much the feed's words as the article is.
    Policy::read("search_feed_articles", Home::Feeds).outside().locked_with("feeds"),
    Policy::read("read_feed_article", Home::Feeds).outside().locked_with("feeds"),
    // Its snippets come out of the same PDFs and Office files `read_file_text`
    // reads whole; a sentence planted in one is as much there in an excerpt.
    Policy::read("search_files", Home::Files).outside().locked_with("file"),
    Policy::read("read_file_text", Home::Files).outside().locked_with("file"),
    // A workbook comes from a bank, a colleague, a download, and a cell can
    // hold a sentence addressed to the model as easily as a number.
    Policy::read("read_spreadsheet", Home::Files).outside().locked_with("file"),
    // The engine sets the taint for this one: it is not in `execute_tool`.
    Policy::browse(BROWSE_TOOL).engine(),

    // ── The run itself ─────────────────────────────────────────────
    // Touches nothing but the run's own list.
    Policy::read(PLAN_TOOL, Home::Core).no_helper().engine(),
    // Hands work to a sub-run that may only read; what that reaches for is
    // weighed again inside it. A helper does not hand on.
    Policy::read(crate::syn::delegate::TOOL, Home::Core).no_helper().engine(),
    // Loads a group of tools; each is weighed again when it is called.
    Policy::read(crate::syn::toolset::FIND_TOOL, Home::Core).engine(),

    // ── Making something new ───────────────────────────────────────
    // `create_node` refuses Syn's own kinds whatever the taint — see
    // `taint::reserved_type`.
    Policy::write("create_node", Home::Core).after_reading().telegram().routine(),
    // Telegram's way of keeping what was sent with no home named. The app has
    // the capture window for that.
    Policy::write(CAPTURE_TOOL, Home::Core).after_reading().telegram().not_in_app(),
    Policy::write("draw_board", Home::Boards).after_reading().routine().locked_with("whiteboard"),
    // All it can do is mark an article read or starred, and "read these and
    // mark them" is most of why the feed tools exist.
    Policy::write("update_feed_article", Home::Feeds).after_reading().telegram().locked_with("feeds"),
    // Only ever a new file: an existing name is refused, and a string that
    // looks like a formula is written as text — see `spreadsheet::write_xlsx`.
    Policy::write("write_spreadsheet", Home::Files)
        .after_reading()
        .locked_with("file")
        .undone_by("a new file, nothing overwritten; deleting it in the Files app moves it to the vault's trash"),
    // A statement is read to be staged: rows for Finance's import preview,
    // and nothing reaches the ledger until the person confirms it there.
    Policy::write(crate::syn::staging::TOOL, Home::Finance)
        .after_reading()
        .locked_with("finance")
        .undone_by("nothing reaches the ledger until the person confirms Finance's import preview; dismissing it there drops the staged rows"),
    // A card the user fills in, or does not. Nothing is sent and nothing is
    // written by the call itself.
    Policy::write("safe_request", Home::Safe).after_reading().locked_with("safe"),

    // ── Changing what was there ────────────────────────────────────
    // After reading somebody else's words, held for the person (`staged`):
    // the change is what something read may have asked for, so it waits on a
    // card at the end of the run with what was read named beside it.
    Policy::write("update_node", Home::Core).telegram().staged(),
    Policy::write("trash_node", Home::Core).telegram().staged(),
    Policy::write("remember", Home::Core).telegram().staged(),
    // The union of whatever its steps do, which `recipe::problems` keeps to
    // writes by refusing the structural tools.
    Policy::write(crate::syn::recipe::RUN_TOOL, Home::Core),
    Policy::write("restore_node", Home::History).telegram().staged(),
    Policy::write("restore_version", Home::History).telegram().staged(),
    Policy::write("edit_board", Home::Boards).staged().locked_with("whiteboard"),
    // Rows inside a note: a write to that note, undone by its earlier version.
    Policy::write("table_rows", Home::Tables).staged(),
    // A page saying *you spent 5,000,000* is exactly the forged record taint
    // is for — held, so the person sees the amount and where it came from.
    Policy::write("create_transaction", Home::Finance).telegram().staged().locked_with("finance"),
    Policy::write("update_transaction", Home::Finance)
        .staged()
        .locked_with("finance")
        .undone_by("update_transaction again, with the values it returned as `before`"),
    Policy::write("delete_transaction", Home::Finance)
        .staged()
        .locked_with("finance")
        .undone_by("the row is kept aside in its month; update_transaction with restore: true puts it back"),

    // ── Reshaping a kind ───────────────────────────────────────────
    Policy::structural("rename_field"),
    Policy::structural("delete_field"),
    Policy::structural("rename_kind"),
    Policy::structural("delete_kind"),

    // ── Connectors: sends and new services ─────────────────────────
    // Lays a plan of sends out for the user; sends nothing itself. Approving
    // the plan is the user deciding where what was read may go. The sends are
    // to connectors, which are the app's alone.
    Policy::read(crate::syn::writes::TOOL, Home::Writes).app_only().engine(),
    // A draft is inert until the user approves it on the card, which shows the
    // host whatever the draft says; drafting from documentation just read is
    // the point. `authoring::check` refuses a host the user did not name.
    Policy { power: Power::DraftFromSpec, ..Policy::write(DRAFT_TOOL, Home::Authoring).after_reading().engine() },
    Policy::read(CONNECTOR_CONNECT, Home::Authoring).app_only().engine(),
    Policy::read(CONNECTOR_FIND, Home::Authoring).app_only(),
    // Reaches the draft's host, asked like any read of it — the engine weighs
    // it from the draft — and held to values the person said once anything
    // was read.
    Policy::read(TRY_TOOL, Home::Authoring).app_only().engine(),

    // ── Skills Syn writes for itself ───────────────────────────────
    // Written switched off; only the person's answer on the card switches it
    // on. Inert until then, so it may be written after reading — and the card
    // says what had been read. The app's alone: that is where the card is.
    // See `syn::drafting`.
    Policy::write(crate::syn::drafting::TOOL, Home::Skills).after_reading().engine(),
    // A routine Syn writes is switched off and agreed to on no computer; the
    // card is where it is turned on. See `routine::draft`.
    Policy::write(crate::syn::routine::DRAFT_TOOL, Home::Jobs).after_reading().engine(),
];

/// The row for a tool, if it is a built-in one.
pub fn of(tool: &str) -> Option<&'static Policy> {
    POLICIES.iter().find(|p| p.name == tool)
}

/// What power a call needs. `None` for a name this table does not know.
pub fn capability(tool: &str, args: &Value) -> Option<Capability> {
    Some(match of(tool)?.power {
        Power::Read => Capability::VaultRead,
        Power::Write => Capability::VaultWrite,
        Power::Structural => Capability::VaultStructural,
        Power::Browse => Capability::Browse,
        Power::DraftFromSpec => match args
            .get("openapi_url")
            .and_then(Value::as_str)
            .and_then(|u| url::Url::parse(u).ok())
            .and_then(|u| u.host_str().map(str::to_string))
        {
            Some(host) => Capability::NetRead { domain: host },
            None => Capability::VaultWrite,
        },
    })
}

/// What undoing one call looks like.
pub fn reversal(tool: &str, capability: &Capability) -> Reversal {
    match of(tool).and_then(|p| p.undo) {
        Some(how) => Reversal::Automatic { how: how.into() },
        None => crate::syn::registry::reversal_of(capability),
    }
}

/// Whether what this tool returns taints the run.
pub fn outside_words(tool: &str) -> bool {
    of(tool).is_some_and(|p| p.outside_words)
}

/// Whether a run that has read somebody else's words may still call it.
pub fn after_reading(tool: &str) -> bool {
    of(tool).is_some_and(|p| p.after_reading == AfterReading::Allow)
}

/// Whether, after reading, a call to it is held for the person instead.
pub fn staged_after_reading(tool: &str) -> bool {
    of(tool).is_some_and(|p| p.after_reading == AfterReading::Stage)
}

/// Whether a surface offers it.
pub fn on(tool: &str, surface: Surface) -> Option<bool> {
    of(tool).map(|p| match surface {
        Surface::App => p.app,
        // Telegram is the person at the computer, from their phone
        // (2026-10-15): what the app is offered, and `capture` besides. Its
        // questions come to the phone as cards with buttons.
        Surface::Telegram => p.app || p.telegram,
        Surface::Routine => p.routine,
    })
}

/// Whether a helper may use it, as far as the row says.
pub fn helper(tool: &str) -> Option<bool> {
    of(tool).map(|p| p.helper)
}

/// The app whose lock hides it.
pub fn lock(tool: &str) -> Option<&'static str> {
    of(tool).and_then(|p| p.lock)
}

/// Whether the engine runs it rather than `execute_tool`.
pub fn engine(tool: &str) -> bool {
    of(tool).is_some_and(|p| p.engine)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    /// Every tool offered to the model has a row, and every row a tool. A tool
    /// added without one would inherit whatever the dispatcher allowed.
    #[test]
    fn every_tool_has_exactly_one_row() {
        let offered: HashSet<String> =
            crate::syn::tools::get_tool_definitions().into_iter().map(|d| d.function.name).collect();
        let mut rows = HashSet::new();
        for p in POLICIES {
            assert!(rows.insert(p.name), "`{}` has two rows", p.name);
            assert!(offered.contains(p.name), "`{}` has a row but is not offered any more", p.name);
        }
        let missing: Vec<&String> = offered.iter().filter(|t| !rows.contains(t.as_str())).collect();
        assert!(missing.is_empty(), "offered with no row: {missing:?}");
    }

    /// A row in the group the toolset puts it in — there is one place that
    /// says which group, and it is here.
    #[test]
    fn the_toolset_groups_by_the_row() {
        for p in POLICIES {
            assert_eq!(crate::syn::toolset::group_of(p.name), p.group.group(), "{}", p.name);
        }
    }

    /// What the rows may never say, whatever a later edit wants.
    #[test]
    fn the_rows_keep_the_rules() {
        for p in POLICIES {
            let n = p.name;
            // A helper reads and looks things up; it changes nothing.
            if p.helper {
                assert!(matches!(p.power, Power::Read | Power::Browse), "{n}: a helper that writes");
            }
            // Telegram has no browser and cannot reshape a vault.
            if p.telegram {
                assert!(matches!(p.power, Power::Read | Power::Write), "{n} on Telegram");
            }
            // Nobody watches a routine, so it may only make new things.
            if p.routine && p.power == Power::Write {
                assert_eq!(p.after_reading, AfterReading::Allow, "{n}: a routine that changes what was there");
            }
            // Reshaping a kind is never done after reading, nor by a helper.
            if p.power == Power::Structural {
                assert!(p.after_reading == AfterReading::Refuse && !p.helper, "{n}");
            }
            // Only a change to the vault can be held: a read has nothing to
            // hold, and what the engine runs is not a change.
            if p.after_reading == AfterReading::Stage {
                assert!(p.power == Power::Write && !p.engine, "{n}");
            }
            // Whatever reads outside words is a read.
            if p.outside_words {
                assert!(matches!(p.power, Power::Read | Power::Browse), "{n}");
            }
        }
    }

    /// Changing or removing the user's work is refused after reading — the
    /// whole point of taint. These are the tools that do that.
    #[test]
    fn edits_are_refused_after_reading() {
        for tool in [
            "update_node", "trash_node", "remember", "restore_node", "restore_version", "edit_board",
            "table_rows", "create_transaction", "update_transaction", "delete_transaction", "run_recipe",
            "rename_field", "delete_field", "rename_kind", "delete_kind",
        ] {
            assert!(!after_reading(tool), "{tool}");
        }
        assert!(!after_reading("connector__jira__create_issue"), "a connector tool is never in the table");
    }

    /// Of those, the ones a person can be shown and asked about are held;
    /// a recipe (the union of its steps) and reshaping a kind are not.
    #[test]
    fn edits_after_reading_are_held_except_the_unshowable() {
        for tool in [
            "update_node", "trash_node", "remember", "restore_node", "restore_version", "edit_board",
            "table_rows", "create_transaction", "update_transaction", "delete_transaction",
        ] {
            assert!(staged_after_reading(tool), "{tool}");
        }
        for tool in ["run_recipe", "rename_field", "delete_field", "rename_kind", "delete_kind", "query_nodes", "create_node"] {
            assert!(!staged_after_reading(tool), "{tool}");
        }
        assert!(!staged_after_reading("connector__jira__create_issue"));
    }

    /// The tools an app's PIN hides.
    #[test]
    fn a_locked_app_hides_its_tools() {
        assert_eq!(lock(crate::syn::staging::TOOL), Some("finance"));
        assert_eq!(lock("create_transaction"), Some("finance"));
        assert_eq!(lock("write_spreadsheet"), Some("file"));
        assert_eq!(lock("draw_board"), Some("whiteboard"));
        assert_eq!(lock("safe_request"), Some("safe"));
        assert_eq!(lock("query_nodes"), None);
    }

    #[test]
    fn a_draft_from_a_spec_reads_its_host() {
        assert_eq!(
            capability(DRAFT_TOOL, &serde_json::json!({ "openapi_url": "https://api.example/openapi.json" })),
            Some(Capability::NetRead { domain: "api.example".into() })
        );
        assert_eq!(capability(DRAFT_TOOL, &serde_json::json!({ "yaml": "id: x" })), Some(Capability::VaultWrite));
    }
}

