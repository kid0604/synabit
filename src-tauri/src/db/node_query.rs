//! Running a saved query over the `nodes` table.
//!
//! The search bar answers "which notes mention this"; a query answers "which
//! notes *are* this" — every task still open, every meeting note from last
//! month, every project over budget. The difference is that the second reads
//! frontmatter as data rather than as words, and returns columns rather than
//! snippets.
//!
//! # How the two halves divide the work
//!
//! Structure is SQL over `nodes`: types, tags, property equalities and
//! comparisons, ordering, the limit. Free text stays with FTS5, which already
//! knows about tone marks, `đ`, and BM25 — so a query carrying words asks the
//! index for the ids that match them and constrains the SQL to those.
//!
//! Neither half is asked to do the other's job, and there is one place each
//! kind of matching is defined.

use std::time::Instant;

use serde::Serialize;

use super::DbBridge;
use crate::error::{AppError, AppResult};
use crate::search::{json_path_for, ParsedQuery, MAX_QUERY_LIMIT, SORTABLE_COLUMNS};

/// One matching note, with the values the query asked to see.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct QueryRow {
    pub id: String,
    pub node_type: String,
    pub title: String,
    /// One entry per column, in the order the columns were requested.
    pub cells: Vec<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct QueryResult {
    /// The columns actually shown, after unusable names were dropped.
    pub columns: Vec<String>,
    pub rows: Vec<QueryRow>,
    /// How many nodes match, ignoring `limit`.
    ///
    /// A real count, not `rows.len()`. It was the latter until an assistant
    /// asked `type:task limit:1` in order to read this number and told the
    /// user they had two tasks out of a hundred and twenty-six: the read
    /// fetched `limit + 1` rows as a cheap "there is more" signal, and every
    /// comment around it — including the one on this field — described that
    /// signal as a count.
    pub total: usize,
    pub query_time_ms: u64,
}

/// A string, as a bound parameter.
fn text(value: &str) -> rusqlite::types::Value {
    rusqlite::types::Value::Text(value.to_string())
}

/// A comparison value bound with the type it is written in.
///
/// This is not a nicety. `json_extract` hands back a JSON number as an
/// SQLite integer, and SQLite sorts every integer before every string — so
/// `priority > '3'` bound as text is false for a priority of 5, and
/// `priority:>3` quietly matched nothing at all.
///
/// Dates stay text on purpose: `2026-09-01` compares correctly as a string,
/// and there is nothing to gain by taking it apart.
fn comparable(value: &str) -> rusqlite::types::Value {
    if let Ok(i) = value.parse::<i64>() {
        return rusqlite::types::Value::Integer(i);
    }
    if let Ok(f) = value.parse::<f64>() {
        return rusqlite::types::Value::Real(f);
    }
    rusqlite::types::Value::Text(value.to_string())
}

/// Columns a table falls back to when the query names none.
const DEFAULT_COLUMNS: &[&str] = &["title", "updated_at"];

/// The SQL for ordering by a name, which is either a node column or a
/// frontmatter key.
///
/// Both halves are safe by construction: the first is a fixed list, and the
/// second goes through `json_path_for`, which admits nothing but a plain key.
fn order_expression(key: &str) -> Option<String> {
    match key {
        "type" => Some("node_type".to_string()),
        "path" => Some("id".to_string()),
        k if SORTABLE_COLUMNS.contains(&k) => Some(k.to_string()),
        k => json_path_for(k).map(|path| format!("json_extract(properties, '{}')", path)),
    }
}

/// A frontmatter value as a table cell.
///
/// Arrays are joined rather than printed as JSON — `tags` is the common case,
/// and `["work","urgent"]` in a table cell is punctuation, not information.
fn cell_text(value: Option<&serde_json::Value>) -> String {
    match value {
        None | Some(serde_json::Value::Null) => String::new(),
        Some(serde_json::Value::String(s)) => s.clone(),
        Some(serde_json::Value::Bool(b)) => b.to_string(),
        Some(serde_json::Value::Number(n)) => n.to_string(),
        Some(serde_json::Value::Array(items)) => items
            .iter()
            .map(|v| cell_text(Some(v)))
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(", "),
        Some(other) => other.to_string(),
    }
}

impl DbBridge {
    /// Notes matching a parsed query, with the columns it asked for.
    pub fn run_node_query(&self, parsed: &ParsedQuery) -> AppResult<QueryResult> {
        let start = Instant::now();

        if parsed.is_empty {
            return Err(AppError::General(
                "A query needs something to match on.".to_string(),
            ));
        }

        // Built on its own so the count and the page ask the same question.
        // `total` used to be whatever the paged read happened to return, which
        // is a different number from "how many match" whenever a limit bites.
        let mut sql = String::from("FROM nodes WHERE node_type NOT LIKE 'finance_%'");
        let mut params: Vec<rusqlite::types::Value> = Vec::new();
        let mut next = 1usize;

        if let Some(node_type) = &parsed.type_filter {
            sql.push_str(&format!(" AND node_type = ?{next}"));
            params.push(text(node_type));
            next += 1;
        }

        if let Some(status) = &parsed.status_filter {
            sql.push_str(&format!(
                " AND lower(CAST(json_extract(properties, '$.status') AS TEXT)) = ?{next}"
            ));
            params.push(text(&status.to_lowercase()));
            next += 1;
        }

        // A property the node must not have this value for.
        //
        // `IS NULL OR <>` rather than a bare `<>`: SQL comparison against NULL
        // is NULL, which filters the row out, so a task that never had a
        // status would be excluded by `-status:done` — the opposite of what
        // "not done" means.
        for (key, value) in &parsed.property_exclusions {
            let Some(path) = crate::search::json_path_for(key) else {
                continue;
            };
            sql.push_str(&format!(
                " AND (json_extract(properties, '{path}') IS NULL
                       OR lower(CAST(json_extract(properties, '{path}') AS TEXT)) <> ?{next})"
            ));
            params.push(text(&value.to_lowercase()));
            next += 1;
        }

        // `tags` is usually an array and occasionally a bare string, so it is
        // wrapped into an array either way rather than handled twice.
        for tag in &parsed.tag_filters {
            sql.push_str(&format!(
                " AND EXISTS (SELECT 1 FROM json_each(
                    CASE WHEN json_type(properties, '$.tags') = 'array'
                         THEN json_extract(properties, '$.tags')
                         ELSE json_array(json_extract(properties, '$.tags')) END
                  ) WHERE lower(CAST(value AS TEXT)) = ?{next})"
            ));
            params.push(text(&tag.to_lowercase()));
            next += 1;
        }

        for (key, value) in &parsed.property_filters {
            let Some(path) = json_path_for(key) else {
                log::warn!("query: ignoring a filter on unusable key '{}'", key);
                continue;
            };
            // A boolean has three spellings by the time it reaches here, and a
            // person writing `pinned:true` means all of them.
            //
            // `json_extract` hands a JSON boolean back as the integer 1 or 0,
            // so casting it to text gives `'1'` — which matched nothing, and
            // said nothing about it. On top of that, a writer that stringified
            // its booleans leaves `'true'` in the file; three task files in
            // this vault carry exactly that. So the comparison is made against
            // the word and its number, and only for the two words that have
            // one.
            let spellings = match value.to_lowercase().as_str() {
                "true" => Some(("true", "1")),
                "false" => Some(("false", "0")),
                _ => None,
            };

            match spellings {
                Some((word, digit)) => {
                    sql.push_str(&format!(
                        " AND lower(CAST(json_extract(properties, '{path}') AS TEXT)) IN (?{next}, ?{})",
                        next + 1
                    ));
                    params.push(text(word));
                    params.push(text(digit));
                    next += 2;
                }
                None => {
                    sql.push_str(&format!(
                        " AND lower(CAST(json_extract(properties, '{path}') AS TEXT)) = ?{next}"
                    ));
                    params.push(text(&value.to_lowercase()));
                    next += 1;
                }
            }
        }

        for range in &parsed.property_ranges {
            let Some(path) = json_path_for(&range.key) else {
                log::warn!(
                    "query: ignoring a comparison on unusable key '{}'",
                    range.key
                );
                continue;
            };
            // The operator comes from a fixed set; only the value is a parameter.
            sql.push_str(&format!(
                " AND json_extract(properties, '{path}') {} ?{next}",
                range.op.as_sql()
            ));
            params.push(comparable(&range.value));
            next += 1;
        }

        // Words are the index's business. Asking it for the ids and narrowing
        // to those keeps one definition of what "matches these words" means —
        // the one that folds tone marks and knows about `đ`.
        if !parsed.fts_terms.is_empty() {
            let matched = self.search_fts(parsed, 1, MAX_QUERY_LIMIT * 4)?;
            if matched.results.is_empty() {
                return Ok(QueryResult {
                    columns: requested_columns(parsed),
                    rows: Vec::new(),
                    total: 0,
                    query_time_ms: start.elapsed().as_millis() as u64,
                });
            }
            let placeholders: Vec<String> = matched
                .results
                .iter()
                .enumerate()
                .map(|(i, _)| format!("?{}", next + i))
                .collect();
            sql.push_str(&format!(" AND id IN ({})", placeholders.join(", ")));
            for hit in &matched.results {
                params.push(text(&hit.id));
            }
        }

        // How many match, before any limit. One extra statement over the same
        // WHERE clause, which is what makes `total` an answer rather than a
        // hint — see the doc comment on the field.
        let total: usize = self
            .conn()
            .query_row(
                &format!("SELECT COUNT(*) {sql}"),
                rusqlite::params_from_iter(params.iter()),
                |row| row.get::<_, i64>(0),
            )
            .map_err(|e| AppError::General(format!("Query count error: {e}")))?
            as usize;

        // Newest first when nothing else is asked for, matching the note list.
        let order = parsed
            .sort
            .as_ref()
            .and_then(|s| order_expression(&s.key).map(|expr| (expr, s.descending)))
            .unwrap_or_else(|| ("updated_at".to_string(), true));
        sql.push_str(&format!(
            " ORDER BY {} {}",
            order.0,
            if order.1 { "DESC" } else { "ASC" }
        ));

        let limit = parsed.limit.unwrap_or(MAX_QUERY_LIMIT).min(MAX_QUERY_LIMIT);
        sql.push_str(&format!(" LIMIT {limit}"));
        // Only when asked. `OFFSET 0` is the same query and a different plan in
        // some engines, and every existing caller passes nothing.
        if parsed.offset > 0 {
            sql.push_str(&format!(" OFFSET {}", parsed.offset));
        }

        let columns = requested_columns(parsed);
        let sql = format!(
            "SELECT id, node_type, title, properties, created_at, updated_at {sql}"
        );
        let mut stmt = self
            .conn()
            .prepare(&sql)
            .map_err(|e| AppError::General(format!("Query prepare error: {e}")))?;

        let mut collected: Vec<QueryRow> = Vec::new();
        let mut rows = stmt
            .query(rusqlite::params_from_iter(params.iter()))
            .map_err(|e| AppError::General(format!("Query error: {e}")))?;

        while let Some(row) = rows
            .next()
            .map_err(|e| AppError::General(format!("Query read error: {e}")))?
        {
            let id: String = row.get(0).unwrap_or_default();
            let node_type: String = row.get(1).unwrap_or_default();
            let title: String = row.get(2).unwrap_or_default();
            let props_text: String = row.get(3).unwrap_or_default();
            let created_at: String = row.get(4).unwrap_or_default();
            let updated_at: String = row.get(5).unwrap_or_default();
            let props: serde_json::Value =
                serde_json::from_str(&props_text).unwrap_or(serde_json::Value::Null);

            let cells = columns
                .iter()
                .map(|column| match column.as_str() {
                    "title" => title.clone(),
                    "type" => node_type.clone(),
                    "path" => id.clone(),
                    "created_at" => created_at.clone(),
                    "updated_at" => updated_at.clone(),
                    key => cell_text(props.get(key)),
                })
                .collect();

            collected.push(QueryRow {
                id,
                node_type,
                title,
                cells,
            });
        }

        Ok(QueryResult {
            columns,
            rows: collected,
            total,
            query_time_ms: start.elapsed().as_millis() as u64,
        })
    }
}

/// The columns to show, falling back when the query names none.
/// The columns a table will draw, with the title guaranteed to lead them.
///
/// A caller choosing its own columns used to replace the lot, title included,
/// and a table of `tags · pinned · full_width` is a hundred rows of `daily`
/// and `false` with no way to tell one node from another. The name is not a
/// column somebody opts into; it is what a row *is*, and every view draws its
/// first cell as the name.
///
/// Enforced here rather than in a view because the columns are decided here,
/// and there is more than one caller: the table, saved views, and the
/// assistant's own queries.
fn requested_columns(parsed: &ParsedQuery) -> Vec<String> {
    if parsed.columns.is_empty() {
        return DEFAULT_COLUMNS.iter().map(|c| c.to_string()).collect();
    }

    let mut columns = parsed.columns.clone();
    if !columns.iter().any(|c| c == "title") {
        columns.insert(0, "title".to_string());
    }
    columns
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::DbBridge;
    use crate::models::node::NodeMetadata;
    use crate::search::parse_query;

    fn db() -> DbBridge {
        DbBridge::new_in_memory_full().expect("full in-memory schema")
    }

    fn seed(db: &DbBridge, id: &str, node_type: &str, title: &str, props: serde_json::Value) {
        let node = NodeMetadata {
            id: id.to_string(),
            node_type: node_type.to_string(),
            title: title.to_string(),
            content: format!("body of {title}"),
            properties: props,
            created_at: "2026-01-01 00:00:00".to_string(),
            updated_at: format!("2026-01-{:02} 00:00:00", (id.len() % 28) + 1),
            timestamp: 0,
            blocks: None,
        };
        db.upsert_node(&node).expect("seed node");
        let tags = node
            .properties
            .get("tags")
            .and_then(|t| t.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str())
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .unwrap_or_default();
        db.upsert_search_entry(
            id,
            node_type,
            title,
            &tags,
            &node.content,
            &node.properties.to_string(),
            node.properties.get("status").and_then(|s| s.as_str()),
            &node.updated_at,
            id,
        );
    }

    fn tasks() -> DbBridge {
        let d = db();
        seed(
            &d,
            "Tasks/a.md",
            "task",
            "viết báo cáo",
            serde_json::json!({ "status": "todo", "priority": 3, "tags": ["work"] }),
        );
        seed(
            &d,
            "Tasks/bb.md",
            "task",
            "họp nhóm",
            serde_json::json!({ "status": "todo", "priority": 1, "tags": ["work", "urgent"] }),
        );
        seed(
            &d,
            "Tasks/ccc.md",
            "task",
            "xong rồi",
            serde_json::json!({ "status": "done", "priority": 5, "tags": ["work"] }),
        );
        seed(
            &d,
            "Notes/dddd.md",
            "note",
            "một ghi chú",
            serde_json::json!({ "tags": ["work"] }),
        );
        d
    }

    fn ids(result: &QueryResult) -> Vec<&str> {
        result.rows.iter().map(|r| r.id.as_str()).collect()
    }

    fn run(db: &DbBridge, query: &str) -> QueryResult {
        db.run_node_query(&parse_query(query)).expect("query")
    }

    #[test]
    fn a_type_and_a_property_narrow_to_what_was_asked_for() {
        let d = tasks();
        let got = run(&d, "is:task status:todo");

        let mut got_ids = ids(&got);
        got_ids.sort();
        assert_eq!(got_ids, vec!["Tasks/a.md", "Tasks/bb.md"]);
    }

    #[test]
    fn a_comparison_reads_a_number_as_a_number() {
        // As text, "5" < "3". Frontmatter carries these as JSON numbers and
        // `json_extract` hands them back as such, so the comparison is
        // arithmetic rather than alphabetical.
        let d = tasks();
        assert_eq!(ids(&run(&d, "is:task priority:>3")), vec!["Tasks/ccc.md"]);
        assert_eq!(ids(&run(&d, "is:task priority:>=3")).len(), 2);
    }

    #[test]
    fn a_tag_is_matched_against_the_list_rather_than_the_text_around_it() {
        let d = tasks();
        assert_eq!(ids(&run(&d, "#urgent")), vec!["Tasks/bb.md"]);
        assert_eq!(run(&d, "#work").rows.len(), 4);
    }

    #[test]
    fn sorting_runs_on_the_frontmatter_value() {
        let d = tasks();
        let asc = run(&d, "is:task sort:priority");
        assert_eq!(ids(&asc), vec!["Tasks/bb.md", "Tasks/a.md", "Tasks/ccc.md"]);

        let desc = run(&d, "is:task sort:-priority");
        assert_eq!(
            ids(&desc),
            vec!["Tasks/ccc.md", "Tasks/a.md", "Tasks/bb.md"]
        );
    }

    #[test]
    fn the_columns_asked_for_come_back_in_the_order_they_were_written() {
        let d = tasks();
        let got = run(&d, "is:task priority:1 columns:title,priority,status");

        assert_eq!(got.columns, vec!["title", "priority", "status"]);
        assert_eq!(got.rows[0].cells, vec!["họp nhóm", "1", "todo"]);
    }

    #[test]
    fn a_column_a_note_does_not_have_reads_as_blank_rather_than_failing() {
        let d = tasks();
        let got = run(&d, "is:note columns:title,priority");

        assert_eq!(got.rows[0].cells, vec!["một ghi chú", ""]);
    }

    #[test]
    fn a_list_value_is_written_out_as_a_list() {
        // `["work","urgent"]` in a table cell is punctuation, not information.
        let d = tasks();
        let got = run(&d, "is:task priority:1 columns:tags");

        // By position rather than by index nought: the title leads every set of
        // columns, so this asks for the tags cell rather than the first one.
        let at = got.columns.iter().position(|c| c == "tags").expect("tags column");
        assert_eq!(got.rows[0].cells[at], "work, urgent");
    }

    #[test]
    fn words_in_a_query_still_go_through_the_search_index() {
        // Which means they fold tone marks, exactly as the search bar does —
        // one definition of "matches these words", not two.
        let d = tasks();
        assert_eq!(ids(&run(&d, "is:task hop")), vec!["Tasks/bb.md"]);
        assert_eq!(ids(&run(&d, "is:task họp")), vec!["Tasks/bb.md"]);
    }

    #[test]
    fn words_that_match_nothing_return_nothing_rather_than_everything() {
        // Dropping the text half of a query silently would turn "the tasks
        // about X" into "all the tasks".
        let d = tasks();
        let got = run(&d, "is:task khôngcótừnày");
        assert!(got.rows.is_empty(), "{:?}", ids(&got));
    }

    /// Proof that "0 events" is an answer and not a silence.
    ///
    /// A count of zero looks identical whether nothing matched or the filter
    /// was dropped on the floor — which is exactly what `type:` used to do to
    /// every type outside a list of five. Seeding one event and finding it is
    /// the only way to tell the two apart.
    #[test]
    fn an_event_is_found_when_there_is_one_to_find() {
        let d = tasks();
        d.upsert_node(&crate::models::node::NodeMetadata {
            id: "Events/standup.md".into(),
            node_type: "event".into(),
            title: "Standup".into(),
            content: String::new(),
            properties: serde_json::json!({ "start_date": "2026-09-01" }),
            created_at: "2026-08-01T00:00:00Z".into(),
            updated_at: "2026-08-01T00:00:00Z".into(),
            timestamp: 0,
            blocks: None,
        })
        .expect("seed an event");

        assert_eq!(run(&d, "type:event").total, 1);
        assert_eq!(run(&d, "is:event").total, 1);
        // And the type actually narrows: tasks must not come back as events.
        assert_eq!(run(&d, "type:event").rows[0].title, "Standup");
    }

    /// "Everything that is not done", which is what people actually ask.
    ///
    /// Measured before it existed: asked how many tasks were unfinished
    /// against a vault of seven, the assistant answered 7 on one run and 0 on
    /// another. `-status:done` was parsed as a word to avoid, so it either
    /// did nothing or excluded everything.
    #[test]
    fn a_property_can_be_excluded_rather_than_only_required() {
        let d = tasks();
        let all = run(&d, "is:task").total;
        let done = run(&d, "is:task status:done").total;
        let not_done = run(&d, "is:task -status:done").total;

        assert!(done > 0 && done < all, "the fixture needs a mix");
        assert_eq!(
            not_done,
            all - done,
            "not-done plus done must account for every task"
        );
    }

    /// A node that never had the property is not excluded by a filter on it.
    ///
    /// `NOT (status = 'done')` is NULL for a node with no status, and SQL
    /// drops the row — so a task with no status set would vanish from "not
    /// done", which is precisely where it belongs.
    #[test]
    fn a_node_without_the_property_counts_as_not_having_the_value() {
        let d = tasks();
        d.upsert_node(&crate::models::node::NodeMetadata {
            id: "Tasks/statusless.md".into(),
            node_type: "task".into(),
            title: "No status at all".into(),
            content: String::new(),
            properties: serde_json::json!({}),
            created_at: "2026-08-01T00:00:00Z".into(),
            updated_at: "2026-08-01T00:00:00Z".into(),
            timestamp: 0,
            blocks: None,
        })
        .expect("seed");

        let titles: Vec<String> = run(&d, "is:task -status:done")
            .rows
            .iter()
            .map(|r| r.title.clone())
            .collect();
        assert!(
            titles.iter().any(|t| t == "No status at all"),
            "a task with no status is not done: {titles:?}"
        );
    }

    /// A row without its name is a row you cannot tell from any other.
    #[test]
    fn a_chosen_set_of_columns_still_leads_with_the_title() {
        let d = db();
        seed(&d, "Notes/a.md", "note", "Lỗi kênh truyền", serde_json::json!({ "tags": ["mdp"] }));

        let got = run(&d, "type:note columns:tags,pinned");

        assert_eq!(got.columns, vec!["title", "tags", "pinned"]);
        assert_eq!(
            got.rows[0].cells[0], "Lỗi kênh truyền",
            "the first cell is the name, which is what every view draws it as",
        );
    }

    /// Asking for it explicitly must not produce it twice.
    #[test]
    fn a_title_asked_for_by_name_is_not_added_again() {
        let d = db();
        seed(&d, "Notes/a.md", "note", "x", serde_json::json!({}));

        let got = run(&d, "type:note columns:tags,title");

        assert_eq!(got.columns, vec!["tags", "title"]);
    }

    /// Reaching past the cap, which is the only way to see a big kind whole.
    ///
    /// The count stays the count: paging changes which rows come back and
    /// nothing about how many matched, so a footer can say "50 of 1000" and
    /// mean it on every page.
    #[test]
    fn an_offset_walks_past_the_cap_without_moving_the_count() {
        let d = db();
        let cap = crate::search::MAX_QUERY_LIMIT as usize;
        let over = cap + 120;
        for i in 0..over {
            seed(&d, &format!("Notes/{i:04}.md"), "note", "n", serde_json::json!({}));
        }

        let first = run(&d, "type:note");
        let mut parsed = crate::search::parse_query("type:note");
        parsed.offset = cap as u32;
        let second = d.run_node_query(&parsed).expect("second page");

        assert_eq!(second.total, first.total, "the count is of matches, not of a page");
        assert_eq!(second.rows.len(), over - cap, "the remainder, and no more");

        // No row appears on both pages: the two together are the whole kind.
        let seen: std::collections::HashSet<_> =
            first.rows.iter().map(|r| r.id.clone()).collect();
        assert!(
            second.rows.iter().all(|r| !seen.contains(&r.id)),
            "a paged row came back twice",
        );
    }

    /// `pinned:true` against a real YAML boolean.
    ///
    /// SQLite's `json_extract` hands back a JSON boolean as the integer 1 or
    /// 0, so casting it to text gives `'1'` — and comparing that to `'true'`
    /// matches nothing. Every pin in the vault was invisible to the one query
    /// that goes looking for pins, with no error to say so.
    #[test]
    fn a_boolean_property_is_matched_by_the_word_for_it() {
        let d = db();
        seed(&d, "Notes/a.md", "note", "ghim", serde_json::json!({ "pinned": true }));
        seed(&d, "Notes/b.md", "note", "không", serde_json::json!({ "pinned": false }));
        seed(&d, "Notes/c.md", "note", "chưa có", serde_json::json!({}));

        assert_eq!(run(&d, "type:note pinned:true").total, 1, "the pinned one");
        assert_eq!(run(&d, "type:note pinned:false").total, 1, "and the unpinned one");
    }

    /// The words a person writes, and the shapes a file can hold.
    ///
    /// A boolean reaches the vault as `true`, as `'true'` from a writer that
    /// stringified it — three task files here carry exactly that — and as `1`
    /// from anything that went through JSON. All three are the same answer to
    /// the same question.
    #[test]
    fn a_boolean_matches_however_it_was_written_down() {
        let d = db();
        seed(&d, "Notes/a.md", "note", "bool", serde_json::json!({ "pinned": true }));
        seed(&d, "Notes/b.md", "note", "chuỗi", serde_json::json!({ "pinned": "true" }));

        assert_eq!(run(&d, "type:note pinned:true").total, 2);
    }

    /// Every query is capped, asked for or not.
    ///
    /// A vault with more of one kind than the cap gets a page and a true
    /// count, and the screen showing them has to say so: "1000 results" over a
    /// list that stops at five hundred is a number that is right about the
    /// vault and wrong about what you are looking at.
    #[test]
    fn a_query_with_no_limit_is_still_capped() {
        let d = db();
        let over = crate::search::MAX_QUERY_LIMIT as usize + 100;
        for i in 0..over {
            seed(&d, &format!("Notes/{i}.md"), "note", "n", serde_json::json!({}));
        }

        let got = run(&d, "type:note");

        assert_eq!(got.total as usize, over, "the count is every match");
        assert_eq!(
            got.rows.len(),
            crate::search::MAX_QUERY_LIMIT as usize,
            "the rows stop at the cap",
        );
    }

    /// A limit caps the rows, never the count.
    ///
    /// This used to assert `total > 2`, which the old implementation satisfied
    /// by reading `limit + 1` rows — so `total` was `3` whether three matched
    /// or three hundred did. A boolean dressed as a number: enough for the "and
    /// more" label in the note table, which was its only reader, and wrong for
    /// anything that treats it as an answer.
    ///
    /// It is now asserted as an exact number, because that is the whole claim.
    #[test]
    fn a_limit_caps_the_rows_and_never_the_count() {
        let d = tasks();
        let all = run(&d, "#work");
        let matching = all.total;
        assert!(matching >= 3, "the fixture needs enough rows to limit");

        for limit in 1..matching {
            let got = run(&d, &format!("#work limit:{limit}"));
            assert_eq!(got.rows.len(), limit, "limit:{limit} returned the wrong page");
            assert_eq!(
                got.total, matching,
                "limit:{limit} changed the count from {matching} to {}",
                got.total
            );
        }
    }

    /// The exact shape that misreported a vault.
    ///
    /// `type:task limit:1` is what an assistant asks when it wants the number
    /// and not the rows. It answered "2" against a hundred and twenty-six
    /// tasks, because `total` was however many rows the capped read returned.
    #[test]
    fn asking_for_one_row_still_reports_how_many_there_are() {
        let d = tasks();
        let everything = run(&d, "is:task");
        let got = run(&d, "is:task limit:1");

        assert_eq!(got.rows.len(), 1);
        assert_eq!(
            got.total,
            everything.total,
            "a caller reading `total` off a one-row page must get the real count"
        );
    }

    #[test]
    fn an_empty_query_is_refused_rather_than_returning_the_vault() {
        let d = tasks();
        assert!(d.run_node_query(&parse_query("")).is_err());
        assert!(d.run_node_query(&parse_query("   ")).is_err());
    }

    /// `json_extract` needs its path as SQL text, so a key is the one thing a
    /// person types that reaches the query as code. It never gets there.
    #[test]
    fn a_key_that_is_not_a_key_cannot_reach_the_sql() {
        let d = tasks();

        // Were the key interpolated, this would drop the table.
        let got = run(&d, "is:task a'); DROP TABLE nodes; --:1");
        assert!(got.rows.is_empty() || !got.rows.is_empty());

        let still_there = run(&d, "is:task");
        assert_eq!(still_there.rows.len(), 3, "the table must still be there");
    }

    #[test]
    fn money_nodes_are_left_out_of_query_results() {
        let d = tasks();
        seed(
            &d,
            "Finance/2026-01.json",
            "finance_month",
            "tháng một",
            serde_json::json!({ "tags": ["work"] }),
        );

        let got = run(&d, "#work");
        assert!(
            !ids(&got).iter().any(|id| id.starts_with("Finance/")),
            "{:?}",
            ids(&got)
        );
    }
}
