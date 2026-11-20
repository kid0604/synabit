//! Which tools a turn is sent.
//!
//! # Why not all of them
//!
//! Every declaration is paid for on every turn. At 37 tools it was about five
//! thousand tokens before the person had said a word — most of an 8,192-token
//! local window, and a steady cost on a hosted one — and `PAYLOAD_BUDGET_CHARS`
//! had been raised four times, each time by one more tool that looked free.
//! Tools from connectors would add tens more. Past a point a model choosing
//! among seventy also chooses worse.
//!
//! So a turn is sent the **core** — the tools most questions use — and the
//! rest arrive in **groups**, when they are wanted.
//!
//! # Who decides a group is wanted
//!
//! The model. `find_tools` is in the core, and its description names every
//! group with what it is for (`description`), so the model sees the whole
//! menu on every turn and loads what the question needs. A tool the model
//! calls by name — the prompt still names several — runs whether or not it
//! was sent, and brings its group for the rest of the run.
//!
//! Until 2026-11-18 Rust read the question first, against lists of words per
//! group, because `recall` had been called in none of fifteen runs and skills
//! loaded in none of seventeen when the model had to think of reaching for
//! something it was not shown. Then `find_tools` named eight groups out of
//! seventeen, and the rest were invisible unless the words gave them away.
//! The lists were the wrong fix: they missed what was said another way ("làm
//! video nhẹ đi", "ghép mấy tấm ảnh") and loaded groups for words that meant
//! something else, and every new group needed another list. Naming every
//! group costs about two thousand characters, which the budget was raised to
//! hold.
//!
//! One thing is still read off the question: a connected server it names
//! (`named_in`). That is the server's own name, not a list anyone keeps.
//!
//! A group once loaded stays loaded for the run. A run that has used the
//! finance tools will likely use them again, and taking them away between
//! rounds would make the second use depend on the model asking a second time.

use std::collections::BTreeSet;

use crate::search_fold::fold;

/// The tool that loads a group.
pub const FIND_TOOL: &str = "find_tools";

/// A set of tools sent together, or not at all.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Group {
    /// Always sent.
    Core,
    Finance,
    Feeds,
    Files,
    Boards,
    Timeline,
    /// The trash and earlier versions: putting things back.
    History,
    /// Renaming or removing a field or a kind across the vault.
    Structure,
    /// Syn's own past: earlier runs, and remembered things looked up by hand.
    Past,
    /// The user's Safe: the names of what Syn may use, and asking for more.
    Safe,
    /// Rich Tables inside notes: their rows, by column.
    Tables,
    /// Working figures out over a table a tool brought back (`crunch`). Sent
    /// once a result in the run carries a table's handle, which is the only
    /// time it can do anything — see [`brought_by`].
    Compute,
    /// Planning sends to connectors (`propose_writes`). Sent whenever a
    /// connector tool that sends is — see the engine's `select`.
    Writes,
    /// Syn writing a connector: `connector_draft`, `connector_connect`.
    Authoring,
    /// Syn writing something to do on its own — at a time, or when something
    /// new comes in — for the person to turn on.
    Jobs,
    /// Drawing an interactive visual: `visual_guide`, `show_visual`. Only in
    /// runs offered them. See `syn::visual::tool::offered`.
    Visuals,
    /// This machine: where a file is, and how the machine is doing. macOS only.
    Machine,
    /// A sealed room on this machine, for real programs. macOS only.
    Room,
    /// The Utilities app's tools, on files of the vault: `run_utility`.
    Utilities,
    /// One connected connector, by its slug.
    Connector(String),
}

impl Group {
    /// Its name, as `find_tools` takes and says it.
    pub fn name(&self) -> String {
        match self {
            Group::Core => "core".into(),
            Group::Finance => "finance".into(),
            Group::Feeds => "feeds".into(),
            Group::Files => "files".into(),
            Group::Boards => "boards".into(),
            Group::Timeline => "timeline".into(),
            Group::History => "history".into(),
            Group::Structure => "structure".into(),
            Group::Past => "past".into(),
            Group::Safe => "safe".into(),
            Group::Tables => "tables".into(),
            Group::Compute => "compute".into(),
            Group::Writes => "writes".into(),
            Group::Authoring => "authoring".into(),
            Group::Jobs => "jobs".into(),
            Group::Visuals => "visuals".into(),
            Group::Machine => "machine".into(),
            Group::Room => "room".into(),
            Group::Utilities => "utilities".into(),
            Group::Connector(server) => format!("connector:{server}"),
        }
    }

    pub fn from_name(name: &str) -> Option<Group> {
        let name = name.trim().to_lowercase();
        Some(match name.as_str() {
            "finance" => Group::Finance,
            "feeds" => Group::Feeds,
            "files" => Group::Files,
            "boards" => Group::Boards,
            "timeline" => Group::Timeline,
            "history" => Group::History,
            "structure" => Group::Structure,
            "past" => Group::Past,
            "safe" => Group::Safe,
            "tables" => Group::Tables,
            "compute" => Group::Compute,
            "writes" => Group::Writes,
            "authoring" => Group::Authoring,
            "jobs" => Group::Jobs,
            "visuals" => Group::Visuals,
            "machine" => Group::Machine,
            "room" => Group::Room,
            "utilities" => Group::Utilities,
            // `mcp:` is what these were called before connectors had their
            // name, and runs from then keep it in `Run::tool_groups`.
            other => {
                return other
                    .strip_prefix("connector:")
                    .or_else(|| other.strip_prefix("mcp:"))
                    .map(|s| Group::Connector(s.to_string()))
            }
        })
    }

    /// What it is for, in one line, for `find_tools`.
    pub fn about(&self) -> String {
        match self {
            Group::Core => "notes, tasks, events, people and any kind: find, read, create, change, trash; the web; memory; plans; helpers".into(),
            Group::Finance => "money: summary, transactions, recording and correcting them".into(),
            Group::Feeds => "feed articles: search, read, mark read or starred".into(),
            Group::Files => "files in the vault: search, read text, spreadsheets".into(),
            Group::Boards => "whiteboards: read, draw, change".into(),
            Group::Timeline => "what happened when: a span of days, weeks or years".into(),
            Group::History => "the trash and earlier versions: restore what was removed or changed".into(),
            Group::Structure => "rename or remove a field or a kind across every note".into(),
            Group::Past => "your own earlier runs, and remembered things searched by hand".into(),
            Group::Safe => "the user's Safe: names of secrets you may use with connectors, and asking the user to add one".into(),
            Group::Tables => "rich tables in notes: add, change or remove rows by column".into(),
            Group::Compute => "totals, averages, counts and filters over a table a tool returned (crunch)".into(),
            Group::Writes => "a plan of sends to connectors for the user to approve (propose_writes)".into(),
            Group::Authoring => "connect to a new service: draft a connector from its API or OpenAPI spec, and ask the user to approve it".into(),
            Group::Jobs => "set up something Syn does on its own — at a time, or when a connected service brings something new (a routine), for the user to turn on".into(),
            Group::Machine => "this Mac, outside the vault: find a file by name anywhere in the person's folders, and how the battery, disk and memory are doing".into(),
            Group::Room => "a sealed room on this Mac with a real shell and no network: run python, ffmpeg, sqlite3 and other programs on copies of vault files — anything the utilities group cannot do to a video or a file, work through a large CSV, make a file, read a folder on this Mac the user opens to it — and keep the result in the vault; and delegate_out, a helper with a shell and the Internet, to download public data into the room".into(),
            Group::Utilities => if cfg!(target_os = "macos") {
                "the Utilities app on vault files, no code: merge, split, reorder, turn or number PDF pages, put them on A4; JPEGs to PDF; Office to PDF, PDF to Word; compress, cut, convert, join, rotate or mute videos, GIFs and frames; convert, cut, join, even out, clean up or speed up sound, ringtones"
            } else {
                "the Utilities app on vault files, no code: merge, split, reorder, turn or number PDF pages, put them on A4; JPEGs to PDF; take the location out of photos"
            }
            .into(),
            Group::Visuals => "draw an interactive visual under the answer: a diagram, a chart of the user's data, an explainer with sliders, a simulation".into(),
            Group::Connector(server) => format!("tools from the connected server `{server}`"),
        }
    }
}

/// The group a tool belongs to. Anything not listed is core: a tool added
/// without deciding is sent every turn, which costs tokens and never breaks
/// anything — the safe way to be wrong.
pub fn group_of(tool: &str) -> Group {
    if let Some(rest) = tool
        .strip_prefix(crate::syn::connector::PREFIX)
        .or_else(|| tool.strip_prefix(crate::syn::connector::provider::LEGACY_PREFIX))
    {
        return Group::Connector(rest.split("__").next().unwrap_or_default().to_string());
    }
    // A built-in tool's group is on its row in `syn::policy`. A name nothing
    // claims is core's, where it has always gone on to fail as unknown.
    crate::syn::policy::of(tool).map_or(Group::Core, |p| p.group.group())
}

/// The groups `find_tools` names, in the order it names them.
///
/// Every group but the core and the two that come on their own: `compute`
/// once a result carries a table (`brought_by`), `writes` with a connector
/// tool that sends. Connectors are named apart, by `naming_servers`. The
/// machine and the room are macOS only.
pub fn listed() -> Vec<Group> {
    let mut groups = vec![
        Group::Files,
        Group::Utilities,
        Group::Finance,
        Group::Feeds,
        Group::Boards,
        Group::Tables,
        Group::Timeline,
        Group::History,
        Group::Structure,
        Group::Past,
        Group::Safe,
        Group::Visuals,
        Group::Jobs,
        Group::Authoring,
    ];
    if cfg!(target_os = "macos") {
        groups.extend([Group::Machine, Group::Room]);
    }
    groups
}

/// `find_tools`'s description: what it does, and every group in `groups`
/// with what it is for, one to a line.
pub fn description(groups: &[Group]) -> String {
    let menu: String = groups.iter().map(|g| format!("\n- {}: {}", g.name(), g.about())).collect();
    format!(
        "Load more tools. Before saying you cannot do something, see whether a group below does it. \
         Pass a group's name, or what you need; the tools arrive on your next step. Groups:{menu}\n\
         - connector:<server>: a connected server's tools."
    )
}

/// The connected servers a question names, whose tools it brings.
///
/// `servers` are the connected connectors' slugs and names. Read off the
/// question because a person who says "Jira" means the Jira they connected,
/// and the name is the server's own: nobody keeps a list of it.
pub fn named_in(question: &str, servers: &[(String, String)]) -> BTreeSet<Group> {
    let asked = fold(question);
    let words: BTreeSet<&str> = asked
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .collect();
    let mut groups = BTreeSet::from([Group::Core]);
    for (slug, name) in servers {
        // "Thời tiết (Open-Meteo)" is asked for as "thời tiết": the name without
        // what is in brackets counts as saying it.
        let short = name.split('(').next().unwrap_or(name).trim().to_string();
        let name = fold(name);
        let short = fold(&short);
        let named = |n: &str| n.trim().chars().count() >= 3 && asked.contains(n.trim());
        if words.contains(fold(slug).as_str()) || named(&name) || named(&short) {
            groups.insert(Group::Connector(slug.clone()));
        }
    }
    groups
}

/// `find_tools`'s description, with the connected servers named in it.
///
/// The model is told `connector:<server>` exists and not which servers there
/// are. Written after Telegram, on 2026-10-15, answered "Check cho tao nhiệt
/// độ con Pi" by saying it had no connection to the Pi, with
/// `raspberry_pi_ssh` connected. Measured afterwards (`eval::tasks`,
/// `tg-pi-*`), it was not the cause: on `gpt-6-luna` the turn reached the Pi
/// 20 times in 20 without the names, asking `find_tools` for "connected
/// machines". The failing turn ran on `gpt-5.6-luna`, which Telegram is no
/// longer pinned to. Kept because naming what is there costs a line and
/// spares a weaker model the guess.
pub fn naming_servers(description: &str, servers: &[(String, String)]) -> String {
    if servers.is_empty() {
        return description.to_string();
    }
    let named: Vec<String> = servers
        .iter()
        .map(|(slug, name)| if name == slug { format!("connector:{slug}") } else { format!("connector:{slug} ({name})") })
        .collect();
    format!("{description} Connected now: {}.", named.join(", "))
}

/// The groups `find_tools` loads for what the model asked it.
///
/// The groups it names — one, or several as "utilities, files" — and every
/// group whose name or description shares a word with the rest of what it
/// asked for: "finance summary; chart visualization" names finance and wants
/// a visual. Utilities brings files with it: a file to work on is found with
/// `search_files`.
pub fn found(query: &str, known: &[Group]) -> Vec<Group> {
    let named = |word: &str| Group::from_name(word).filter(|g| known.contains(g) && *g != Group::Core);
    let (names, rest): (Vec<&str>, Vec<&str>) = query.split([',', ';', ' ', '\n']).partition(|w| named(w).is_some());
    let mut got: Vec<Group> = names.into_iter().filter_map(named).collect();
    // What is left once the names are taken out: "connector:jira" names a
    // server and is not a word about writing connectors.
    let wanted: BTreeSet<String> = fold(&rest.join(" "))
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.len() >= 3 && !FILLER.contains(w))
        .map(one)
        .collect();
    got.extend(
        known
            .iter()
            .filter(|g| **g != Group::Core)
            .filter(|g| {
                let said = fold(&format!("{} {}", g.name(), g.about()));
                said.split(|c: char| !c.is_alphanumeric()).any(|w| wanted.contains(&one(w)))
            })
            .cloned(),
    );
    if got.contains(&Group::Utilities) && known.contains(&Group::Files) {
        got.push(Group::Files);
    }
    let mut seen = BTreeSet::new();
    got.retain(|g| seen.insert(g.clone()));
    got
}

/// A word as one of it: "whiteboard" was asked for and "whiteboards" is what
/// the boards group says, and the two did not match — Syn asked twice in one
/// turn and was told nothing matched. English plurals only, which is what the
/// descriptions are written in.
fn one(word: &str) -> String {
    match word.strip_suffix('s') {
        Some(stem) if stem.len() >= 3 && !stem.ends_with('s') => stem.to_string(),
        _ => word.to_string(),
    }
}

/// Words every description has, which match a query to every group: "tools
/// for the video" is about the video.
const FILLER: &[&str] = &[
    "the", "and", "for", "with", "you", "your", "what", "any", "all", "are", "can", "from", "that", "this", "its", "not",
    "out", "tool", "tools", "need", "want", "into", "how", "one", "use", "user",
];

/// A group a tool's result makes worth sending for the rest of the run.
///
/// A result that filed a table says its handle; from then on `crunch` is the
/// way to a number about it, so it is sent. Read off the result rather than
/// off which tool ran, because any tool that returns a table — a connector,
/// `get_transactions`, `crunch` itself — does it the same way.
pub fn brought_by(content: &str) -> Option<Group> {
    (content.contains("\"handle\":\"r") || content.contains("\"handle\": \"r")).then_some(Group::Compute)
}

/// Whether a tool is sent, given the groups loaded.
pub fn offered(tool: &str, loaded: &BTreeSet<Group>) -> bool {
    let group = group_of(tool);
    group == Group::Core || loaded.contains(&group)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn groups(q: &str) -> Vec<String> {
        named_in(q, &[("jira".into(), "Jira".into())]).iter().map(Group::name).collect()
    }

    #[test]
    fn find_tools_names_what_is_connected() {
        let base = "Load more tools.";
        assert_eq!(naming_servers(base, &[]), base);
        let servers = [
            ("raspberry_pi_ssh".to_string(), "Raspberry Pi (SSH)".to_string()),
            ("jira".to_string(), "jira".to_string()),
        ];
        assert_eq!(
            naming_servers(base, &servers),
            "Load more tools. Connected now: connector:raspberry_pi_ssh (Raspberry Pi (SSH)), connector:jira."
        );
    }

    /// No list of words decides: what a question says another way is the
    /// model's to load, from the menu `find_tools` carries.
    #[test]
    fn the_words_bring_no_group_of_their_own() {
        for q in [
            "Tóm tắt note họp hôm nay",
            "Tháng này tôi tiêu bao nhiêu tiền?",
            "gộp 2 file pdf hợp đồng lại",
            "làm cái video này nhẹ đi",
            "tuần trước tôi làm gì",
        ] {
            assert_eq!(groups(q), vec!["core"], "{q}");
        }
    }

    /// Every group the model can load is on the menu, with what it is for.
    #[test]
    fn find_tools_names_every_group() {
        let said = description(&listed());
        for g in listed() {
            assert!(said.contains(&format!("\n- {}: {}", g.name(), g.about())), "{} is not named", g.name());
        }
        assert!(said.contains("connector:<server>"));
        assert!(!listed().contains(&Group::Core));
        assert!(!listed().contains(&Group::Compute) && !listed().contains(&Group::Writes));
        assert_eq!(Group::from_name("utilities"), Some(Group::Utilities));
        assert_eq!(group_of(crate::syn::utility::TOOL), Group::Utilities);
    }

    /// Several named at once are all loaded, and Utilities brings the way
    /// to find its file.
    #[test]
    fn find_tools_loads_several_and_utilities_brings_files() {
        let known = listed();
        assert_eq!(found("finance, timeline", &known), vec![Group::Finance, Group::Timeline]);
        assert_eq!(found("utilities", &known), vec![Group::Utilities, Group::Files]);
        let pi = [known.clone(), vec![Group::Connector("raspberry_pi_ssh".into())]].concat();
        assert_eq!(found("connector:raspberry_pi_ssh", &pi), vec![Group::Connector("raspberry_pi_ssh".into())]);
        assert!(found("compress a video", &known).contains(&Group::Utilities));
        // A group named and one described, in one ask: both.
        let both = found("finance summary; chart visualization", &known);
        assert!(both.contains(&Group::Finance) && both.contains(&Group::Visuals), "{both:?}");
        // What every description says matches no group by itself.
        assert!(found("the tools for this", &known).is_empty());
    }

    /// Asked for in the singular, found under the plural. Syn's two asks of
    /// 2026-11-19, both naming "whiteboard", were each told nothing matched.
    #[test]
    fn find_tools_matches_one_and_many() {
        let known = listed();
        let vi = "Tạo tranh minh họa phong cảnh dạng hình ảnh trên whiteboard, không phải sơ đồ hộp và mũi tên";
        assert!(found(vi, &known).contains(&Group::Boards), "{:?}", found(vi, &known));
        assert!(found("a picture", &known).contains(&Group::Boards));
        assert!(found("spreadsheet", &known).contains(&Group::Files));
        // Not every word ending in s is a plural.
        assert_eq!(super::one("address"), "address");
        assert_eq!(super::one("bus"), "bus");
    }

    #[test]
    fn a_question_naming_a_server_brings_its_tools() {
        assert!(groups("các issue Jira của tôi tuần này").contains(&"connector:jira".into()));
    }

    #[test]
    fn a_tool_belongs_to_one_group_and_the_unlisted_are_core() {
        assert_eq!(group_of("get_transactions"), Group::Finance);
        assert_eq!(group_of("connector__jira__search"), Group::Connector("jira".into()));
        assert_eq!(group_of("query_nodes"), Group::Core);
        assert_eq!(group_of("a_tool_added_tomorrow"), Group::Core);
    }

    #[test]
    fn find_tools_loads_by_name_or_by_what_it_is_for() {
        let known = vec![Group::Finance, Group::Files, Group::Timeline, Group::Connector("jira".into())];
        assert_eq!(found("finance", &known), vec![Group::Finance]);
        assert_eq!(found("connector:jira", &known), vec![Group::Connector("jira".into())]);
        // A run from before connectors had their name kept its groups as `mcp:`.
        assert_eq!(Group::from_name("mcp:jira"), Some(Group::Connector("jira".into())));
        assert_eq!(group_of("mcp__jira__search"), Group::Connector("jira".into()));
        assert!(found("spreadsheets", &known).contains(&Group::Files));
        assert!(found("money", &known).contains(&Group::Finance));
        assert!(found("nothing like it", &known).is_empty());
    }

    #[test]
    fn a_loaded_group_is_offered_and_others_are_not() {
        let loaded = BTreeSet::from([Group::Core, Group::Finance]);
        assert!(offered("query_nodes", &loaded));
        assert!(offered("search_finance", &loaded));
        assert!(!offered("draw_board", &loaded));
    }

    #[test]
    fn a_result_with_a_table_brings_crunch() {
        assert_eq!(brought_by(r#"{"handle":"r3","rows":3000}"#), Some(Group::Compute));
        assert_eq!(brought_by(r#"{"results":[]}"#), None);
        assert_eq!(group_of(crate::syn::crunch::TOOL), Group::Compute);
        assert!(!offered(crate::syn::crunch::TOOL, &BTreeSet::from([Group::Core])));
        assert_eq!(Group::from_name("compute"), Some(Group::Compute));
    }
}
