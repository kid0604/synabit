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
//! Rust, first. This codebase has measured what happens when the model has to
//! think of reaching for something it was not shown: `recall` was called in
//! none of fifteen runs, skills were loaded in none of seventeen. So the
//! question is read before the model sees it (`for_question`), the same way
//! `tempo`, `timeline::asked` and `skill::chosen_for` read it: a question
//! about money brings the finance tools, one about last week brings the
//! timeline, one naming a connected server brings that server's tools.
//!
//! The model, second: `find_tools` names every group and loads one, for the
//! question the words did not give away. And a tool the model calls by name
//! — the prompt still names several — runs whether or not it was sent, and
//! brings its group for the rest of the run.
//!
//! A group once loaded stays loaded for the run. A run that has used the
//! finance tools will likely use them again, and taking them away between
//! rounds would make the second use depend on the words a second time.

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
    match tool {
        "get_finance_summary" | "search_finance" | "get_transactions" | "create_transaction"
        | "update_transaction" | "delete_transaction" => Group::Finance,
        "search_feed_articles" | "read_feed_article" | "update_feed_article" => Group::Feeds,
        "search_files" | "read_file_text" | "read_spreadsheet" | "write_spreadsheet" => Group::Files,
        "read_board" | "draw_board" | "edit_board" => Group::Boards,
        "timeline" => Group::Timeline,
        "list_trash" | "restore_node" | "list_versions" | "restore_version" => Group::History,
        "rename_field" | "delete_field" | "rename_kind" | "delete_kind" => Group::Structure,
        "recall" | crate::syn::tools::LOOK_BACK_TOOL => Group::Past,
        "safe_list" | "safe_health" | "safe_request" => Group::Safe,
        _ => Group::Core,
    }
}

/// Words that bring a group, written as a person types them.
///
/// With their marks, because folding them away makes Vietnamese words collide:
/// "tiền" (money) and "tiện" (handy) are both `tien`, and "tiện thể ghi lại"
/// is not a question about money. A question typed with marks is matched with
/// marks; one typed without any — which people do — is matched folded, where
/// the collision is the question's own and nothing better is possible.
fn cues(group: &Group) -> &'static [&'static str] {
    match group {
        Group::Finance => &[
            "tiền", "chi tiêu", "tiêu", "thu nhập", "lương", "giao dịch", "ngân sách", "tài khoản",
            "số dư", "tài chính", "vnd", "triệu", "hoá đơn", "hóa đơn", "thanh toán", "spend",
            "spent", "expense", "expenses", "budget", "income", "salary", "transaction",
            "transactions", "balance", "money", "finance", "paid", "bill",
        ],
        Group::Feeds => &[
            "feed", "feeds", "rss", "bài báo", "bài viết", "tin tức", "đọc báo", "article",
            "articles", "news", "newsletter",
        ],
        Group::Files => &[
            "file", "files", "tệp", "pdf", "docx", "word", "excel", "xlsx", "xls", "csv", "ods",
            "bảng tính", "spreadsheet", "tài liệu", "document", "attachment", "đính kèm",
        ],
        Group::Boards => &[
            "whiteboard", "board", "bảng trắng", "sơ đồ", "mindmap", "mind map", "diagram", "vẽ",
            "draw", "canvas",
        ],
        Group::Timeline => &[
            "hôm qua", "tuần trước", "tháng trước", "năm ngoái", "năm trước", "dạo này", "gần đây",
            "timeline", "dòng thời gian", "lúc đó", "khi nào", "ngày nào", "yesterday", "last week",
            "last month", "last year", "recently", "when did", "what happened",
        ],
        Group::History => &[
            "khôi phục", "thùng rác", "phục hồi", "phiên bản", "hoàn tác", "lấy lại", "undo",
            "restore", "trash", "version", "versions", "revert", "deleted",
        ],
        Group::Structure => &[
            "đổi tên trường", "xoá trường", "xóa trường", "đổi tên loại", "xoá loại", "xóa loại",
            "rename field", "delete field", "rename kind", "delete kind", "rename type",
        ],
        Group::Past => &[
            "lần trước", "trước đây", "bạn đã nói", "hôm trước", "bạn nói", "earlier", "last time",
            "you said", "you told", "you answered", "nhớ lại",
        ],
        Group::Safe => &[
            "mật khẩu", "khoá", "khóa", "api key", "token", "secret", "safe", "đăng nhập", "tài khoản",
            "password", "passwords", "key", "keys", "credential", "credentials", "login", "api",
        ],
        Group::Core | Group::Connector(_) => &[],
    }
}

/// Whether the text carries any Vietnamese marks at all.
fn has_marks(text: &str) -> bool {
    fold(text) != text.to_lowercase()
}

/// The groups a question brings, read from its words.
///
/// `servers` are the connected connectors' slugs and names: a question that
/// names one brings its tools.
pub fn for_question(question: &str, servers: &[(String, String)]) -> BTreeSet<Group> {
    // Matched as typed when it was typed with marks; folded when it was not.
    let marked = has_marks(question);
    let shape = |text: &str| if marked { text.to_lowercase() } else { fold(text) };
    let asked = shape(question);
    let words: BTreeSet<&str> = asked
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .collect();
    let said = |cue: &str| {
        let cue = shape(cue);
        if cue.contains(' ') {
            asked.contains(&cue)
        } else {
            words.contains(cue.as_str())
        }
    };

    let mut groups = BTreeSet::from([Group::Core]);
    for group in [
        Group::Finance,
        Group::Feeds,
        Group::Files,
        Group::Boards,
        Group::Timeline,
        Group::History,
        Group::Structure,
        Group::Past,
        Group::Safe,
    ] {
        if cues(&group).iter().any(|cue| said(cue)) {
            groups.insert(group);
        }
    }
    for (slug, name) in servers {
        let name = shape(name);
        if said(slug) || (!name.trim().is_empty() && asked.contains(name.trim())) {
            groups.insert(Group::Connector(slug.clone()));
        }
    }
    groups
}

/// The groups `find_tools` loads for what the model asked it.
///
/// A group named exactly, or every group whose name or description shares a
/// word with the query.
pub fn found(query: &str, known: &[Group]) -> Vec<Group> {
    if let Some(group) = Group::from_name(query).filter(|g| known.contains(g)) {
        return vec![group];
    }
    let wanted: BTreeSet<String> = fold(query)
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.len() >= 3)
        .map(str::to_string)
        .collect();
    known
        .iter()
        .filter(|g| **g != Group::Core)
        .filter(|g| {
            let said = fold(&format!("{} {}", g.name(), g.about()));
            said.split(|c: char| !c.is_alphanumeric()).any(|w| wanted.contains(w))
                || for_question(query, &[]).contains(g)
        })
        .cloned()
        .collect()
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
        for_question(q, &[("jira".into(), "Jira".into())]).iter().map(Group::name).collect()
    }

    #[test]
    fn a_plain_question_is_sent_the_core_only() {
        assert_eq!(groups("Tóm tắt note họp hôm nay"), vec!["core"]);
        assert_eq!(groups("What are my tasks today?"), vec!["core"]);
    }

    #[test]
    fn the_words_bring_their_tools() {
        assert!(groups("Tháng này tôi tiêu bao nhiêu tiền?").contains(&"finance".into()));
        assert!(groups("how much did I spend on food").contains(&"finance".into()));
        assert!(groups("Có bài báo nào hay trên feed không").contains(&"feeds".into()));
        assert!(groups("đọc file excel báo cáo").contains(&"files".into()));
        assert!(groups("vẽ sơ đồ kiến trúc").contains(&"boards".into()));
        assert!(groups("tuần trước tôi làm gì").contains(&"timeline".into()));
        assert!(groups("khôi phục note vừa xoá").contains(&"history".into()));
        assert!(groups("lần trước bạn nói gì về FPT").contains(&"past".into()));
    }

    /// A word inside another is not the word: "tiền" is not in "tiện".
    #[test]
    fn a_cue_is_a_whole_word() {
        assert!(!groups("tiện thể ghi lại").contains(&"finance".into()));
        assert!(!groups("wordy notes").contains(&"files".into()));
        // Typed without marks, it is matched folded: "tien" is money.
        assert!(groups("thang nay tieu bao nhieu tien").contains(&"finance".into()));
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
        assert!(!offered("read_board", &loaded));
    }
}
