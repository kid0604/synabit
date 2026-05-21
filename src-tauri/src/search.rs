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
    /// Status filter for tasks: todo, in-progress, done
    pub status_filter: Option<String>,
    /// Date filter: today, this-week, this-month
    pub date_filter: Option<String>,
    /// Search only in title field
    pub title_only: bool,
    /// Generic property filters: key:value pairs not matching known keywords
    pub property_filters: Vec<(String, String)>,
    /// Whether the query is empty (no meaningful search terms)
    pub is_empty: bool,
    /// Whether to enforce case-sensitive matching (post-filter)
    pub case_sensitive: bool,
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
            // Regular word
            let mut word = String::new();
            while let Some(&c) = chars.peek() {
                if c.is_whitespace() {
                    break;
                }
                word.push(c);
                chars.next();
            }
            tokens.push(word);
        }
    }

    for token in tokens {
        let lower = token.to_lowercase();

        // is: filter
        if let Some(stripped) = lower.strip_prefix("is:") {
            let val = stripped.to_string();
            match val.as_str() {
                "note" | "task" | "event" | "quickcap" | "file" => {
                    pq.type_filter = Some(val);
                }
                _ => {} // ignore unknown types
            }
            continue;
        }

        // status: filter
        if let Some(stripped) = lower.strip_prefix("status:") {
            let val = stripped.to_string();
            match val.as_str() {
                "todo" | "in-progress" | "done" => {
                    pq.status_filter = Some(val);
                }
                _ => {}
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
            pq.tag_filters.push(lower[4..].to_string());
            pq.is_empty = false;
            continue;
        }

        // -exclude term
        if token.starts_with('-') && token.len() > 1 && !token.starts_with("--") {
            pq.exclude_terms.push(token[1..].to_string());
            continue;
        }

        // Generic key:value property filter (catch-all for unknown key:value pairs)
        if let Some(colon_pos) = lower.find(':') {
            let key = &lower[..colon_pos];
            let val = &lower[colon_pos + 1..];
            if !key.is_empty() && !val.is_empty() {
                pq.property_filters.push((key.to_string(), val.to_string()));
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

    let mut parts: Vec<String> = Vec::new();

    for term in &pq.fts_terms {
        if term.starts_with('"') && term.ends_with('"') {
            // Already a phrase query, pass through
            parts.push(term.clone());
        } else {
            // Add wildcard for prefix matching (e.g. "proj" matches "project")
            // FTS5 uses * for prefix queries
            parts.push(format!("\"{}\" *", term));
        }
    }

    // Exclude terms with NOT
    for term in &pq.exclude_terms {
        parts.push(format!("NOT \"{}\"", term));
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

    #[test]
    fn test_unknown_type_filter_ignored() {
        let pq = parse_query("is:banana meeting");
        // Unknown type should be ignored, not set as type_filter
        assert!(pq.type_filter.is_none());
        assert_eq!(pq.fts_terms, vec!["meeting"]);
    }

    #[test]
    fn test_unknown_status_ignored() {
        let pq = parse_query("status:banana");
        assert!(pq.status_filter.is_none());
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
