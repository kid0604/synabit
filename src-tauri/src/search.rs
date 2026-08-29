use serde::Serialize;

/// Parsed representation of a user's search query.
/// Handles syntax: `is:note`, `#tag`, `"exact phrase"`, `-exclude`, `in:title`, `status:done`, `date:today`
#[derive(Debug, Default)]
pub struct ParsedQuery {
    /// FTS5 MATCH expression (tokenized terms + phrases)
    pub fts_terms: Vec<String>,
    /// Type filter: note, task, event, quickcap, file
    pub type_filter: Option<String>,
    /// Tag filters (without #)
    pub tag_filters: Vec<String>,
    /// Excluded terms (without -)
    pub exclude_terms: Vec<String>,
    /// `-status:done` — property values a node must *not* have.
    ///
    /// Split from `exclude_terms` because they are different questions. A bare
    /// `-draft` means "no note whose text says draft"; `-status:done` means
    /// "not finished", and a note that never had a status at all satisfies it.
    /// Written as a word exclusion, the second reads as "no note containing the
    /// literal text status:done", which is every note.
    ///
    /// Measured cost of not having it: asked how many tasks were unfinished,
    /// the assistant answered 7 (all of them) and 0 (none of them) on two
    /// separate runs. The number was 4, and neither run had a way to say so.
    pub property_exclusions: Vec<(String, String)>,
    /// Status filter for tasks: todo, in-progress, done
    pub status_filter: Option<String>,
    /// Date filter: today, this-week, this-month
    pub date_filter: Option<String>,
    /// Search only in title field
    pub title_only: bool,
    /// Generic property filters: key:value pairs not matching known keywords
    pub property_filters: Vec<(String, String)>,
    /// Property comparisons — `priority:>2`, `due_date:<2026-09-01`.
    ///
    /// Kept apart from the equality filters above because they are the same
    /// syntax asking a different question, and a query view lives on them.
    pub property_ranges: Vec<PropertyRange>,
    /// `sort:key`, or `sort:-key` for descending.
    pub sort: Option<SortOrder>,
    /// `columns:title,due_date` — which keys a table should show.
    pub columns: Vec<String>,
    /// `limit:20`, capped so one query cannot ask for the whole vault.
    pub limit: Option<u32>,
    /// Whether the query is empty (no meaningful search terms)
    pub is_empty: bool,
    /// Whether to enforce case-sensitive matching (post-filter)
    pub case_sensitive: bool,
    /// Match a node that has *any* of the terms, rather than all of them.
    ///
    /// Off for anything a person types. Somebody searching "meeting notes"
    /// wants notes about meetings, and OR would hand them every note.
    ///
    /// On for a question fed in by the assistant's retrieval, where the terms
    /// are whatever survived stopword removal — `decide`, `pricing`,
    /// `disagreed` — and requiring all of them means requiring the asker to
    /// have guessed the note's exact vocabulary. Measured before it was
    /// changed: a question about a decision recorded in two notes retrieved
    /// nothing, because one of its words was `decide` and the note says
    /// "we settled on".
    pub match_any: bool,
}

/// A single search result returned to the frontend
#[derive(Debug, Serialize, Clone)]
pub struct SearchResult {
    pub id: String,
    pub item_type: String,
    pub title: String,
    pub snippet: String,
    pub tags: Vec<String>,
    pub date: String,
    pub path: String,
    pub score: f64,
    pub status: Option<String>,
}

/// Wrapper for paginated search responses
#[derive(Debug, Serialize)]
pub struct SearchResponse {
    pub results: Vec<SearchResult>,
    pub total_count: u32,
    pub query_time_ms: u64,
}

/// Node fields a query may sort by or show, alongside frontmatter keys.
///
/// These live in columns of their own rather than inside `properties`, so they
/// are named here rather than reached through `json_extract`.
pub const SORTABLE_COLUMNS: &[&str] = &["title", "created_at", "updated_at", "type", "path"];

/// The most rows one query may ask for.
///
/// A query sits in a note and runs whenever the note is opened, so an
/// unbounded one is a table nobody scrolls and a scan nobody asked for.
pub const MAX_QUERY_LIMIT: u32 = 500;

/// How two values are being compared.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Comparison {
    GreaterThan,
    GreaterOrEqual,
    LessThan,
    LessOrEqual,
}

impl Comparison {
    /// The SQL operator, chosen from a fixed set rather than built from input.
    pub fn as_sql(self) -> &'static str {
        match self {
            Comparison::GreaterThan => ">",
            Comparison::GreaterOrEqual => ">=",
            Comparison::LessThan => "<",
            Comparison::LessOrEqual => "<=",
        }
    }
}

/// A comparison against a frontmatter value: `priority:>2`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PropertyRange {
    pub key: String,
    pub op: Comparison,
    pub value: String,
}

/// Which key to order by, and which way.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SortOrder {
    pub key: String,
    pub descending: bool,
}

/// Read a comparison off the front of a value: `>2` is greater-than two.
fn split_comparison(value: &str) -> Option<(Comparison, &str)> {
    // The two-character forms first: `>=` also starts with `>`.
    for (prefix, op) in [
        (">=", Comparison::GreaterOrEqual),
        ("<=", Comparison::LessOrEqual),
        (">", Comparison::GreaterThan),
        ("<", Comparison::LessThan),
    ] {
        if let Some(rest) = value.strip_prefix(prefix) {
            let rest = rest.trim();
            if !rest.is_empty() {
                return Some((op, rest));
            }
        }
    }
    None
}

/// Whether a name is something a query may sort by or show.
fn is_queryable_key(key: &str) -> bool {
    SORTABLE_COLUMNS.contains(&key) || json_path_for(key).is_some()
}

/// The JSON path for a frontmatter key, or `None` if the key is not one.
///
/// `json_extract` takes its path as SQL text rather than as a parameter, so
/// this is the one place where something a person typed would reach a query
/// as code. Nothing but letters, digits and underscores gets through, which
/// is also every key the app itself writes.
pub fn json_path_for(key: &str) -> Option<String> {
    if key.is_empty() || key.len() > 64 {
        return None;
    }
    if !key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return None;
    }
    Some(format!("$.{}", key))
}

/// Parse a raw search query string into a structured ParsedQuery.
///
/// Supported syntax:
/// - Plain words: tokenized with AND logic (e.g. "đi tắm" → "đi" AND "tắm")
/// - `"exact phrase"`: matched as a phrase
/// - `#tag`: filter by tag
/// - `is:note` / `is:task` / `is:event` / `is:quickcap` / `is:file`: type filter
/// - `-word`: exclude results containing this word
/// - `in:title`: search only in title field
/// - `status:done` / `status:todo` / `status:in-progress`: task status filter
/// - `date:today` / `date:this-week` / `date:this-month`: date filter
pub fn parse_query(raw: &str) -> ParsedQuery {
    let mut pq = ParsedQuery {
        is_empty: true,
        ..Default::default()
    };

    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return pq;
    }

    let mut chars = trimmed.chars().peekable();
    let mut tokens: Vec<String> = Vec::new();

    // Tokenize: handle quoted phrases and regular words
    while let Some(&ch) = chars.peek() {
        if ch.is_whitespace() {
            chars.next();
            continue;
        }

        if ch == '"' {
            // Quoted phrase
            chars.next(); // consume opening quote
            let mut phrase = String::new();
            while let Some(&c) = chars.peek() {
                if c == '"' {
                    chars.next(); // consume closing quote
                    break;
                }
                phrase.push(c);
                chars.next();
            }
            if !phrase.trim().is_empty() {
                // FTS5 phrase syntax: "word1 word2"
                tokens.push(format!("\"{}\"", phrase.trim()));
            }
        } else {
            // Regular word (can contain quotes, e.g. tag:"one mount" or tag:“one mount”)
            let mut word = String::new();
            let mut in_quote = false;
            while let Some(&c) = chars.peek() {
                if c == '"' || c == '“' || c == '”' {
                    in_quote = !in_quote;
                    word.push(c);
                    chars.next();
                } else if c.is_whitespace() && !in_quote {
                    break;
                } else {
                    word.push(c);
                    chars.next();
                }
            }
            tokens.push(word);
        }
    }

    for token in tokens {
        let lower = token.to_lowercase();

        // is: filter
        // `type:` and `is:` are the same filter. `is:` came first and is what
        // the Tasks search bar sends; `type:` is what the frontmatter field is
        // called, so it is what anyone writing a query — or an assistant
        // reading `list_schemas` — reaches for first.
        if let Some(stripped) = lower
            .strip_prefix("is:")
            .or_else(|| lower.strip_prefix("type:"))
        {
            // Any type, not a list of five.
            //
            // This used to accept `note | task | event | quickcap | file` and
            // silently drop everything else — so `is:book` did not filter to
            // books, it filtered to nothing at all and returned the whole
            // vault. The rest of the engine never had that limit: `node_type`
            // is a free string in the schema, the column is compared through a
            // bound parameter, and search already indexes every type.
            //
            // A list in the code deciding which of the user's types are real
            // is the same mistake `NodeType::Other` exists to prevent, one
            // layer up.
            if !stripped.is_empty() {
                pq.type_filter = Some(stripped.to_string());
                pq.is_empty = false;
            }
            continue;
        }
        if let Some(stripped) = lower.strip_prefix("status:") {
            // Same widening, and here it was not merely narrow but wrong:
            // the list read `in-progress` while every task in every vault is
            // written `in_progress`, and `backlog` and `canceled` — both real
            // statuses the Tasks app writes — were not on it at all. All three
            // were dropped without a word.
            if !stripped.is_empty() {
                pq.status_filter = Some(stripped.to_string());
                pq.is_empty = false;
            }
            continue;
        }

        // date: filter
        if let Some(stripped) = lower.strip_prefix("date:") {
            let val = stripped.to_string();
            match val.as_str() {
                "today" | "this-week" | "this-month" => {
                    pq.date_filter = Some(val);
                }
                _ => {}
            }
            continue;
        }

        // in:title modifier
        if lower == "in:title" {
            pq.title_only = true;
            continue;
        }

        // #tag or tag:xxx filter
        if token.starts_with('#') && token.len() > 1 {
            pq.tag_filters.push(token[1..].to_string());
            pq.is_empty = false;
            continue;
        } else if lower.starts_with("tag:") && lower.len() > 4 {
            let mut val = lower[4..].to_string();
            if (val.starts_with('"') || val.starts_with('“') || val.starts_with('”'))
                && (val.ends_with('"') || val.ends_with('”') || val.ends_with('“'))
                && val.chars().count() >= 2
            {
                let mut chars = val.chars();
                chars.next();
                chars.next_back();
                val = chars.collect();
            }
            pq.tag_filters.push(val);
            pq.is_empty = false;
            continue;
        }

        // -exclude term
        if token.starts_with('-') && token.len() > 1 && !token.starts_with("--") {
            // `-key:value` is a property exclusion, not a word to avoid.
            if let Some((key, value)) = lower[1..].split_once(':') {
                if !key.is_empty() && !value.is_empty() && is_queryable_key(key) {
                    pq.property_exclusions
                        .push((key.to_string(), value.to_string()));
                    pq.is_empty = false;
                    continue;
                }
            }
            let mut val = token[1..].to_string();
            if (val.starts_with('"') || val.starts_with('“') || val.starts_with('”'))
                && (val.ends_with('"') || val.ends_with('”') || val.ends_with('“'))
                && val.chars().count() >= 2
            {
                let mut chars = val.chars();
                chars.next();
                chars.next_back();
                val = chars.collect();
            }
            pq.exclude_terms.push(val);
            continue;
        }

        // How a table built from this query should be shaped. These say nothing
        // about *which* notes match, only about how the ones that do are laid
        // out, so they are read before the catch-all below claims them.
        if let Some(rest) = lower.strip_prefix("sort:") {
            let (key, descending) = match rest.strip_prefix('-') {
                Some(k) => (k, true),
                None => (rest, false),
            };
            if is_queryable_key(key) {
                pq.sort = Some(SortOrder {
                    key: key.to_string(),
                    descending,
                });
                pq.is_empty = false;
                continue;
            }
        }

        if let Some(rest) = lower.strip_prefix("columns:") {
            pq.columns = rest
                .split(',')
                .map(str::trim)
                .filter(|c| is_queryable_key(c))
                .map(str::to_string)
                .collect();
            if !pq.columns.is_empty() {
                pq.is_empty = false;
                continue;
            }
        }

        if let Some(rest) = lower.strip_prefix("limit:") {
            if let Ok(n) = rest.trim().parse::<u32>() {
                pq.limit = Some(n.clamp(1, MAX_QUERY_LIMIT));
                pq.is_empty = false;
                continue;
            }
        }

        // Generic key:value property filter (catch-all for unknown key:value pairs)
        if let Some(colon_pos) = lower.find(':') {
            let key = &lower[..colon_pos];
            let mut val = lower[colon_pos + 1..].to_string();
            if (val.starts_with('"') || val.starts_with('“') || val.starts_with('”'))
                && (val.ends_with('"') || val.ends_with('”') || val.ends_with('“'))
                && val.chars().count() >= 2
            {
                let mut chars = val.chars();
                chars.next();
                chars.next_back();
                val = chars.collect();
            }
            if !key.is_empty() && !val.is_empty() {
                match split_comparison(&val) {
                    Some((op, rest)) => pq.property_ranges.push(PropertyRange {
                        key: key.to_string(),
                        op,
                        value: rest.to_string(),
                    }),
                    None => pq.property_filters.push((key.to_string(), val)),
                }
                pq.is_empty = false;
                continue;
            }
        }

        // Regular search term or quoted phrase
        pq.fts_terms.push(token);
        pq.is_empty = false;
    }

    // If we only have filters (type, status, tag) but no search terms, it's not empty
    if pq.type_filter.is_some()
        || pq.status_filter.is_some()
        || pq.date_filter.is_some()
        || !pq.tag_filters.is_empty()
        || !pq.property_filters.is_empty()
        || !pq.property_ranges.is_empty()
    {
        pq.is_empty = false;
    }

    pq
}

/// Build a FTS5 MATCH expression from parsed query terms.
/// Returns None if there are no FTS terms to search.
pub fn build_fts_match(pq: &ParsedQuery) -> Option<String> {
    if pq.fts_terms.is_empty() {
        return None;
    }

    // A bare word is allowed to contain a quote — `tag:"one mount"` is a
    // supported spelling, and so is a typo like `foo"bar`. Wrapping such a
    // term in quotes without escaping ends the FTS5 string early and the whole
    // query comes back as a syntax error. FTS5 spells a literal quote inside a
    // string as two of them.
    fn quote(term: &str) -> String {
        format!("\"{}\"", term.replace('"', "\"\""))
    }

    let mut parts: Vec<String> = Vec::new();

    for term in &pq.fts_terms {
        if term.starts_with('"') && term.ends_with('"') && term.len() > 1 {
            // Already a phrase query, pass through
            parts.push(term.clone());
        } else {
            // Add wildcard for prefix matching (e.g. "proj" matches "project")
            // FTS5 uses * for prefix queries
            parts.push(format!("{} *", quote(term)));
        }
    }

    // Exclude terms with NOT
    for term in &pq.exclude_terms {
        parts.push(format!("NOT {}", quote(term)));
    }

    // Exclusions are always AND: "any of these words, but none of those" is
    // the only reading of a mixed query that makes sense.
    if pq.match_any && !pq.fts_terms.is_empty() {
        let matches = parts[..pq.fts_terms.len()].join(" OR ");
        let excluded = &parts[pq.fts_terms.len()..];
        let mut all = vec![format!("({matches})")];
        all.extend(excluded.iter().cloned());
        return Some(all.join(" AND "));
    }

    Some(parts.join(" AND "))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_tokenized_search() {
        let pq = parse_query("đi tắm");
        assert_eq!(pq.fts_terms, vec!["đi", "tắm"]);
        assert!(!pq.is_empty);
        assert!(pq.type_filter.is_none());
    }

    #[test]
    fn a_quote_inside_a_word_does_not_end_the_query() {
        // Wrapping `foo"bar` in quotes unescaped closes the FTS5 string early
        // and the whole MATCH comes back as a syntax error — which, in the
        // feeds search, meant results silently stopped changing as you typed.
        let pq = parse_query("foo\"bar");
        let expr = build_fts_match(&pq).expect("a query with a term in it");
        assert!(
            expr.contains("\"\""),
            "a literal quote should be doubled, got {expr}"
        );
        assert_eq!(
            expr.matches('"').count() % 2,
            0,
            "quotes must stay balanced, got {expr}"
        );
    }

    #[test]
    fn test_exact_phrase() {
        let pq = parse_query("\"đi tắm\"");
        assert_eq!(pq.fts_terms, vec!["\"đi tắm\""]);
    }

    #[test]
    fn test_type_filter() {
        let pq = parse_query("is:task urgent");
        assert_eq!(pq.type_filter, Some("task".to_string()));
        assert_eq!(pq.fts_terms, vec!["urgent"]);
    }

    #[test]
    fn test_tag_filter() {
        let pq = parse_query("#work meeting");
        assert_eq!(pq.tag_filters, vec!["work"]);
        assert_eq!(pq.fts_terms, vec!["meeting"]);

        let pq2 = parse_query("tag:urgent");
        assert_eq!(pq2.tag_filters, vec!["urgent"]);
        assert!(pq2.property_filters.is_empty());
    }

    #[test]
    fn test_exclude_term() {
        let pq = parse_query("project -archived");
        assert_eq!(pq.fts_terms, vec!["project"]);
        assert_eq!(pq.exclude_terms, vec!["archived"]);
    }

    #[test]
    fn test_status_filter() {
        let pq = parse_query("is:task status:done");
        assert_eq!(pq.type_filter, Some("task".to_string()));
        assert_eq!(pq.status_filter, Some("done".to_string()));
    }

    #[test]
    fn test_title_only() {
        let pq = parse_query("in:title meeting");
        assert!(pq.title_only);
        assert_eq!(pq.fts_terms, vec!["meeting"]);
    }

    #[test]
    fn test_complex_query() {
        let pq = parse_query("is:note #work \"meeting notes\" -draft in:title");
        assert_eq!(pq.type_filter, Some("note".to_string()));
        assert_eq!(pq.tag_filters, vec!["work"]);
        assert_eq!(pq.fts_terms, vec!["\"meeting notes\""]);
        assert_eq!(pq.exclude_terms, vec!["draft"]);
        assert!(pq.title_only);
    }

    #[test]
    fn test_empty_query() {
        let pq = parse_query("   ");
        assert!(pq.is_empty);
    }

    #[test]
    fn test_build_fts_match() {
        let pq = parse_query("đi tắm");
        let fts = build_fts_match(&pq);
        assert!(fts.is_some());
        let expr = fts.unwrap();
        assert!(expr.contains("đi"));
        assert!(expr.contains("tắm"));
    }

    #[test]
    fn test_build_fts_match_with_exclude() {
        let pq = parse_query("project -archived");
        let fts = build_fts_match(&pq).unwrap();
        assert!(fts.contains("NOT"));
        assert!(fts.contains("archived"));
    }

    #[test]
    fn test_property_filter() {
        let pq = parse_query("meeting priority:P2");
        assert_eq!(pq.fts_terms, vec!["meeting"]);
        assert_eq!(
            pq.property_filters,
            vec![("priority".to_string(), "p2".to_string())]
        );
        assert!(!pq.is_empty);
    }

    #[test]
    fn test_multiple_property_filters() {
        let pq = parse_query("is:task priority:P1 ext:pdf");
        assert_eq!(pq.type_filter, Some("task".to_string()));
        assert_eq!(
            pq.property_filters,
            vec![
                ("priority".to_string(), "p1".to_string()),
                ("ext".to_string(), "pdf".to_string()),
            ]
        );
    }

    #[test]
    fn test_property_filter_only() {
        let pq = parse_query("location:hanoi");
        assert!(pq.fts_terms.is_empty());
        assert_eq!(
            pq.property_filters,
            vec![("location".to_string(), "hanoi".to_string())]
        );
        assert!(!pq.is_empty);
    }

    // ── Edge Cases ────────────────────────────────

    #[test]
    fn test_unicode_search() {
        let pq = parse_query("日本語 テスト");
        assert_eq!(pq.fts_terms, vec!["日本語", "テスト"]);
        assert!(!pq.is_empty);
    }

    #[test]
    fn test_emoji_in_query() {
        let pq = parse_query("🚀 launch");
        assert_eq!(pq.fts_terms, vec!["🚀", "launch"]);
    }

    #[test]
    fn test_unclosed_quote() {
        // Unclosed quote should capture until end of string
        let pq = parse_query("\"unclosed phrase");
        assert_eq!(pq.fts_terms, vec!["\"unclosed phrase\""]);
    }

    #[test]
    fn test_empty_quotes() {
        let pq = parse_query("\"\"");
        // Empty quotes should not produce FTS terms
        assert!(pq.fts_terms.is_empty());
        assert!(pq.is_empty);
    }

    #[test]
    fn test_date_filter() {
        let pq = parse_query("date:today meeting");
        assert_eq!(pq.date_filter, Some("today".to_string()));
        assert_eq!(pq.fts_terms, vec!["meeting"]);

        let pq2 = parse_query("date:this-week");
        assert_eq!(pq2.date_filter, Some("this-week".to_string()));

        let pq3 = parse_query("date:this-month");
        assert_eq!(pq3.date_filter, Some("this-month".to_string()));
    }

    /// A type this app has not heard of is filtered on, not discarded.
    ///
    /// These two tests used to assert the opposite — `is:banana` and
    /// `status:banana` were expected to be dropped — and they were the reason
    /// the limit went unnoticed for so long: they read as careful input
    /// validation. They were not. There is no such thing as an unknown type in
    /// a vault where a type is whatever somebody wrote in a `type:` field, and
    /// `banana` is a perfectly good one. What the old behaviour actually did
    /// was answer a different question from the one asked: `is:banana meeting`
    /// returned every note mentioning "meeting" regardless of type, which is
    /// wrong far more quietly than returning nothing would have been.
    ///
    /// The filter is bound as a SQL parameter, so an unrecognised value costs
    /// an empty result set and nothing else.
    #[test]
    fn a_type_this_app_has_not_heard_of_is_still_a_filter() {
        let pq = parse_query("is:banana meeting");
        assert_eq!(pq.type_filter.as_deref(), Some("banana"));
        assert_eq!(pq.fts_terms, vec!["meeting"]);
    }

    #[test]
    fn a_status_this_app_has_not_heard_of_is_still_a_filter() {
        let pq = parse_query("status:banana");
        assert_eq!(pq.status_filter.as_deref(), Some("banana"));
    }

    #[test]
    fn test_sql_injection_attempt() {
        // Malicious input should be treated as regular search terms
        let pq = parse_query("'; DROP TABLE search_index; --");
        // Should be parsed as regular tokens, not executed
        assert!(pq.fts_terms.contains(&"';".to_string()));
        assert!(!pq.is_empty);
    }

    #[test]
    fn test_multiple_tags() {
        let pq = parse_query("#work #personal meeting");
        assert_eq!(pq.tag_filters, vec!["work", "personal"]);
        assert_eq!(pq.fts_terms, vec!["meeting"]);
    }

    #[test]
    fn test_hash_alone_not_tag() {
        let pq = parse_query("#");
        // Single # should not be a tag
        assert!(pq.tag_filters.is_empty());
    }

    #[test]
    fn test_dash_alone_not_exclude() {
        let pq = parse_query("-");
        // Single - should not be treated as exclude
        assert!(pq.exclude_terms.is_empty());
    }

    #[test]
    fn test_double_dash_not_exclude() {
        let pq = parse_query("--flag");
        // Double dash should not be exclude (it's a CLI flag pattern)
        assert!(pq.exclude_terms.is_empty());
        assert!(pq.fts_terms.contains(&"--flag".to_string()));
    }

    #[test]
    fn test_mixed_case_filters() {
        let pq = parse_query("IS:Task STATUS:Done IN:TITLE hello");
        assert_eq!(pq.type_filter, Some("task".to_string()));
        assert_eq!(pq.status_filter, Some("done".to_string()));
        assert!(pq.title_only);
        assert_eq!(pq.fts_terms, vec!["hello"]);
    }

    /// `-key:value` excludes a property; `-word` still excludes a word.
    #[test]
    fn a_negated_filter_is_told_apart_from_a_negated_word() {
        let pq = parse_query("is:task -status:done");
        assert_eq!(pq.property_exclusions, vec![("status".into(), "done".into())]);
        assert!(pq.exclude_terms.is_empty(), "it is not a word to avoid");

        let pq = parse_query("meeting -draft");
        assert_eq!(pq.exclude_terms, vec!["draft"]);
        assert!(pq.property_exclusions.is_empty());
    }

    /// The negative form reads a token exactly as the positive form does.
    ///
    /// Any frontmatter key is queryable — that is the point — so `key:value`
    /// cannot be told apart from a colon that happens to be in a word, and a
    /// bare `http://example.com` has always parsed as a filter on a key named
    /// `http`. That is a pre-existing wart, and the thing worth pinning is that
    /// negation does not invent a *second* rule: whatever `x:y` means,
    /// `-x:y` means the opposite of it.
    #[test]
    fn negation_reads_a_token_the_same_way_the_positive_form_does() {
        for token in ["http://example.com", "author:Herbert", "rating:5"] {
            let positive = parse_query(token);
            let negative = parse_query(&format!("-{token}"));
            assert_eq!(
                positive.property_filters, negative.property_exclusions,
                "`{token}` and `-{token}` disagree about what kind of token this is"
            );
        }
    }

    /// `status` has a dedicated branch on the positive side and goes through
    /// the generic one when negated. The asymmetry is in the parser only —
    /// both end up reading `json_extract(properties, '$.status')`, so
    /// `status:done` and `-status:done` are exact opposites where it counts.
    #[test]
    fn status_is_filtered_one_way_and_excluded_the_other_but_means_the_same_field() {
        assert_eq!(parse_query("status:done").status_filter.as_deref(), Some("done"));
        assert_eq!(
            parse_query("-status:done").property_exclusions,
            vec![("status".to_string(), "done".to_string())]
        );
    }

    /// `match_any` is off unless something asks for it, because everything a
    /// person types goes through here. Widening the search box to OR would
    /// turn "meeting notes" into every note.
    #[test]
    fn a_query_a_person_typed_still_requires_every_word() {
        let pq = parse_query("meeting notes");
        assert!(!pq.match_any);
        let expr = build_fts_match(&pq).expect("an expression");
        assert!(expr.contains(" AND "), "{expr}");
        assert!(!expr.contains(" OR "), "{expr}");
    }

    #[test]
    fn match_any_widens_the_terms_but_never_the_exclusions() {
        let mut pq = parse_query("decide pricing -draft");
        pq.match_any = true;
        let expr = build_fts_match(&pq).expect("an expression");

        assert!(expr.contains(" OR "), "terms should widen: {expr}");
        assert!(
            expr.contains("AND NOT"),
            "an exclusion stays an exclusion: {expr}"
        );
    }

    /// A type nobody hard-coded still filters.
    ///
    /// Found by running the roadmap's own gate: the assistant read the vault's
    /// schema correctly, wrote a correct query, and got nothing back, because
    /// the parser recognised five type names and dropped the rest without a
    /// word. Dropping a filter is the worst of the three options — refusing it
    /// would have been visible, honouring it would have been right, and
    /// ignoring it answers a question nobody asked.
    #[test]
    fn any_type_can_be_filtered_on_not_a_list_of_five() {
        for known in ["note", "task", "event", "quickcap", "file"] {
            assert_eq!(parse_query(&format!("is:{known}")).type_filter.as_deref(), Some(known));
        }
        // The point of the whole exercise.
        for invented in ["book", "recipe", "habit", "réunion"] {
            assert_eq!(
                parse_query(&format!("is:{invented}")).type_filter.as_deref(),
                Some(invented),
                "`is:{invented}` must filter to {invented}, not to everything"
            );
        }
    }

    /// `type:` is what the frontmatter field is called, so it is what anyone
    /// writing a query reaches for. It used to fall through to the generic
    /// property filter and look for a frontmatter key named `type`, which no
    /// node has — the type is a column.
    #[test]
    fn type_and_is_are_the_same_filter() {
        assert_eq!(parse_query("type:book").type_filter.as_deref(), Some("book"));
        assert_eq!(
            parse_query("type:task status:todo").type_filter.as_deref(),
            Some("task")
        );
        assert!(
            parse_query("type:book").property_filters.is_empty(),
            "`type:` must not also become a property filter on a key named `type`"
        );
    }

    /// The status list was not merely narrow, it was wrong: it read
    /// `in-progress` while every task ever written by this app says
    /// `in_progress`, so the Tasks search bar's own in-progress filter was
    /// being discarded before it reached SQL.
    #[test]
    fn every_status_the_app_writes_can_be_filtered_on() {
        for status in ["todo", "in_progress", "done", "backlog", "canceled"] {
            assert_eq!(
                parse_query(&format!("status:{status}")).status_filter.as_deref(),
                Some(status),
                "`status:{status}` is a status the Tasks app writes and must filter"
            );
        }
    }

    /// A bare `is:` or `status:` says nothing and must not become a filter on
    /// the empty string, which would match no node at all.
    #[test]
    fn an_empty_filter_is_not_a_filter() {
        assert!(parse_query("is:").type_filter.is_none());
        assert!(parse_query("type:").type_filter.is_none());
        assert!(parse_query("status:").status_filter.is_none());
    }

    #[test]
    fn test_all_node_types() {
        for t in &["note", "task", "event", "quickcap", "file"] {
            let pq = parse_query(&format!("is:{}", t));
            assert_eq!(pq.type_filter, Some(t.to_string()));
        }
    }

    // ── build_fts_match edge cases ────────────────

    #[test]
    fn test_build_fts_match_empty() {
        let pq = parse_query("is:task status:done");
        // No FTS terms, only filters
        let fts = build_fts_match(&pq);
        assert!(fts.is_none());
    }

    #[test]
    fn test_build_fts_match_phrase_passthrough() {
        let pq = parse_query("\"exact match\"");
        let fts = build_fts_match(&pq).unwrap();
        // Phrase should be passed through as-is
        assert_eq!(fts, "\"exact match\"");
    }

    #[test]
    fn test_build_fts_match_mixed() {
        let pq = parse_query("hello \"world peace\"");
        let fts = build_fts_match(&pq).unwrap();
        assert!(fts.contains("\"hello\" *")); // regular term gets wildcard
        assert!(fts.contains("\"world peace\"")); // phrase passed through
        assert!(fts.contains(" AND ")); // joined with AND
    }

    #[test]
    fn test_build_fts_match_exclude_only() {
        // Edge case: exclude terms without FTS terms
        let pq = parse_query("project -draft -archived");
        let fts = build_fts_match(&pq).unwrap();
        assert!(fts.contains("\"project\" *"));
        assert!(fts.contains("NOT \"draft\""));
        assert!(fts.contains("NOT \"archived\""));
    }
}

#[cfg(test)]
mod query_syntax_tests {
    use super::*;

    #[test]
    fn a_comparison_is_read_apart_from_an_equality() {
        let pq = parse_query("priority:>2 status:done");

        assert_eq!(
            pq.property_ranges,
            vec![PropertyRange {
                key: "priority".to_string(),
                op: Comparison::GreaterThan,
                value: "2".to_string(),
            }]
        );
        // `status:` is a filter of its own and is not swept into the generic list.
        assert_eq!(pq.status_filter.as_deref(), Some("done"));
    }

    #[test]
    fn both_characters_of_a_two_character_operator_are_read() {
        // `>=` also starts with `>`, so the order the prefixes are tried in is
        // the difference between `>= 2` and `> =2`.
        let pq = parse_query("priority:>=2 due_date:<=2026-09-01");

        assert_eq!(pq.property_ranges[0].op, Comparison::GreaterOrEqual);
        assert_eq!(pq.property_ranges[0].value, "2");
        assert_eq!(pq.property_ranges[1].op, Comparison::LessOrEqual);
        assert_eq!(pq.property_ranges[1].value, "2026-09-01");
    }

    #[test]
    fn sort_reads_its_direction_off_a_leading_minus() {
        assert_eq!(
            parse_query("sort:due_date").sort,
            Some(SortOrder {
                key: "due_date".to_string(),
                descending: false
            })
        );
        assert_eq!(
            parse_query("sort:-updated_at").sort,
            Some(SortOrder {
                key: "updated_at".to_string(),
                descending: true
            })
        );
    }

    #[test]
    fn columns_are_split_and_kept_in_the_order_they_were_written() {
        let pq = parse_query("columns:title,due_date,priority");
        assert_eq!(pq.columns, vec!["title", "due_date", "priority"]);
    }

    /// A key reaches the SQL as text, so anything that is not a key is dropped
    /// here rather than escaped later.
    #[test]
    fn a_key_that_could_not_be_a_column_is_not_accepted_as_one() {
        assert!(parse_query("sort:a'b").sort.is_none());
        assert_eq!(
            parse_query("columns:title,a'b,due_date").columns,
            vec!["title", "due_date"]
        );
    }

    /// A query sits in a note and runs every time it is opened.
    #[test]
    fn a_limit_is_capped_rather_than_taken_at_its_word() {
        assert_eq!(parse_query("limit:20").limit, Some(20));
        assert_eq!(parse_query("limit:999999").limit, Some(MAX_QUERY_LIMIT));
        assert_eq!(parse_query("limit:0").limit, Some(1));
        assert_eq!(parse_query("limit:abc").limit, None);
    }

    /// The shaping words must not be mistaken for something to search for.
    #[test]
    fn shaping_a_table_is_not_the_same_as_filtering_it() {
        let pq = parse_query("is:task sort:due_date columns:title limit:5");

        assert!(pq.fts_terms.is_empty(), "{:?}", pq.fts_terms);
        assert!(pq.property_filters.is_empty(), "{:?}", pq.property_filters);
        assert_eq!(pq.type_filter.as_deref(), Some("task"));
        assert!(!pq.is_empty);
    }

    #[test]
    fn a_plain_value_is_still_an_equality() {
        let pq = parse_query("priority:2");
        assert!(pq.property_ranges.is_empty());
        assert_eq!(
            pq.property_filters,
            vec![("priority".to_string(), "2".to_string())]
        );
    }
}
