//! Rows of a Rich Table, changed by column name: what `table_rows` does.
//!
//! A Rich Table is a Markdown pipe table with a `<!-- rich-table` comment
//! under it (`docs/rich-table-2026-10-05.md` §4). Adding one row used to mean
//! rewriting the whole note through `update_node` — the model re-typing every
//! row it was not touching, any of which it could get wrong. This edits only
//! the rows asked for and leaves every other line of the note byte for byte.
//!
//! The format's rules are the app's (`src/shared/rich-table/markdown.ts`): a
//! `|` inside a cell is written `\|`, a line break `<br>`; numbers plain,
//! checkboxes `[x]` or `[ ]`. A formula column is not written to: the app
//! computes it, and does when the note is next shown.
//!
//! # What counts as a Rich Table here
//!
//! Less than the app will show as one, on purpose. The app reads the note
//! with a Markdown parser; this reads it by line, and where the two could
//! disagree this says *no table* rather than guess. A table in a code fence is
//! an example, not data. A table inside a list or a quote is not one the app
//! makes a Rich Table either. The comment must sit on the line right under
//! the last row, which is where the app writes it. Getting one of these wrong
//! the other way would mean rewriting a line the person never meant as a row.

use std::collections::HashSet;
use std::sync::LazyLock;

use serde_json::{Map, Value};

/// The newest comment format this understands: `FORMAT_VERSION` in
/// `markdown.ts`. A table written by a later one is left alone, as the app
/// leaves it alone — rewriting rows under settings this does not understand
/// is how a setting gets lost.
const FORMAT_VERSION: f64 = 1.0;

/// One Rich Table in a note body, by line.
#[derive(Debug)]
pub struct Table {
    pub name: Option<String>,
    pub columns: Vec<String>,
    /// Columns declared `type: formula`, long form or short.
    pub formulas: HashSet<String>,
    /// Columns declared `type: checkbox`, where a blank cell is unticked.
    pub checkboxes: HashSet<String>,
    /// The table's body rows, unescaped, as many cells as there are columns.
    pub rows: Vec<Vec<String>>,
    /// Why this table must not be written, when it must not: its settings
    /// would not parse, or came from a later version. The app shows such a
    /// table read-only, and so does this.
    pub refused: Option<String>,
    /// Each body row's block marker (` ^abc123`), which a rewrite keeps.
    markers: Vec<Option<String>>,
    /// Line numbers: the header, and one past the last body row.
    header: usize,
    end: usize,
}

/// Is this line the comment that makes the table above it a Rich Table?
/// At the very start of the line, as `isRichTableComment` reads it once the
/// app has found it right under a table.
fn is_comment(line: &str) -> bool {
    let Some(rest) = line.strip_prefix("<!--") else { return false };
    let Some(after) = rest.trim_start().strip_prefix("rich-table") else { return false };
    after.is_empty() || after.starts_with("-->") || after.starts_with(char::is_whitespace)
}

/// The line, and the block marker at its end if it carries one: the
/// ` ^abc123` that `place_block_marker` appends so a block can be linked to.
/// It is not a cell, and a row rewritten without it would break the link.
fn split_marker(line: &str) -> (&str, Option<&str>) {
    let trimmed = line.trim_end();
    let bytes = trimmed.as_bytes();
    let n = bytes.len();
    if n >= 8
        && bytes[n - 8] == b' '
        && bytes[n - 7] == b'^'
        && bytes[n - 6..].iter().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
    {
        (&trimmed[..n - 8], Some(&trimmed[n - 8..]))
    } else {
        (line, None)
    }
}

fn indent_of(line: &str) -> usize {
    line.len() - line.trim_start_matches(' ').len()
}

/// A line that goes on a table: a pipe first, indented no more than Markdown
/// allows a block to be before it turns into code.
fn is_row_line(line: &str) -> bool {
    indent_of(line) <= 3 && line.trim_start().starts_with('|')
}

/// The fence a line opens — its character and how many — when it opens one.
///
/// At any indent, more readily than Markdown would: a fence four spaces in
/// is code inside a code block or a fence inside a list, and either way what
/// follows is nothing to edit.
fn fence_of(line: &str) -> Option<(u8, usize)> {
    let t = line.trim_start();
    let c = *t.as_bytes().first()?;
    if c != b'`' && c != b'~' {
        return None;
    }
    let n = t.bytes().take_while(|b| *b == c).count();
    if n < 3 || (c == b'`' && t[n..].contains('`')) {
        return None;
    }
    Some((c, n))
}

fn closes_fence(line: &str, (c, n): (u8, usize)) -> bool {
    let t = line.trim();
    let m = t.bytes().take_while(|b| *b == c).count();
    m >= n && t[m..].trim().is_empty()
}

/// Whether the lines just above a table header make it something other than
/// a top-level table: the lazy continuation of a list item or a quote, which
/// is where Markdown puts a pipe line written straight under one.
fn continues_something(lines: &[&str], header: usize) -> bool {
    let mut first = header;
    while first > 0 && !lines[first - 1].trim().is_empty() {
        first -= 1;
    }
    if first == header {
        return false;
    }
    if lines[header - 1].trim_start().starts_with('|') {
        // A row of something tabular above: this line is part of that.
        return true;
    }
    let line = lines[first];
    if indent_of(line) > 0 {
        return true;
    }
    let t = line.trim_start();
    let list = t
        .strip_prefix(['-', '*', '+'])
        .or_else(|| {
            let digits = t.bytes().take_while(u8::is_ascii_digit).count();
            (1..=9).contains(&digits).then(|| t[digits..].strip_prefix(['.', ')'])).flatten()
        })
        .is_some_and(|rest| rest.is_empty() || rest.starts_with([' ', '\t']));
    list || t.starts_with('>') || (t.starts_with('<') && !t.starts_with("<!--"))
}

/// A delimiter row's cell: dashes, a colon at either end or both.
fn is_delimiter_cell(cell: &str) -> bool {
    let c = cell.trim();
    let c = c.strip_prefix(':').unwrap_or(c);
    let c = c.strip_suffix(':').unwrap_or(c);
    !c.is_empty() && c.bytes().all(|b| b == b'-')
}

static BR: LazyLock<regex::Regex> = LazyLock::new(|| regex::Regex::new(r"(?i)<br\s*/?>").unwrap());

/// A cell as it means, from a cell as the line holds it: `unescapeCell`.
fn unescape_cell(value: &str) -> String {
    BR.replace_all(&value.trim().replace("\\|", "|"), "\n").into_owned()
}

/// The cells of a table line, split on pipes that are not escaped, unescaped.
pub fn split_row(line: &str) -> Vec<String> {
    let mut s = line.trim();
    if let Some(rest) = s.strip_prefix('|') {
        s = rest;
    }
    if s.ends_with('|') && !s.ends_with("\\|") {
        s = &s[..s.len() - 1];
    }
    let mut cells = Vec::new();
    let mut current = String::new();
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' && chars.peek() == Some(&'|') {
            chars.next();
            current.push_str("\\|");
        } else if c == '|' {
            cells.push(current);
            current = String::new();
        } else {
            current.push(c);
        }
    }
    cells.push(current);
    cells.iter().map(|c| unescape_cell(c)).collect()
}

/// A row's cells, exactly as many as the columns. Extra cells are folded into
/// the last, as `parseRichTable` folds them: Markdown drops them from view,
/// and dropping them here would lose them on the next write.
fn fit_row(mut cells: Vec<String>, width: usize) -> Vec<String> {
    if cells.len() > width && width > 0 {
        let extra: Vec<String> = cells
            .split_off(width - 1)
            .into_iter()
            .enumerate()
            .filter(|(i, c)| *i == 0 || !c.trim().is_empty())
            .map(|(_, c)| c)
            .collect();
        cells.push(extra.join(" | "));
    }
    cells.resize(width, String::new());
    cells
}

/// A cell as the table line holds it: `|` escaped, every kind of line break
/// as `<br>`, so a value from anywhere stays on its one line.
pub fn escape_cell(value: &str) -> String {
    value
        .trim()
        .replace('|', "\\|")
        .replace("\r\n", "<br>")
        .replace(['\r', '\n'], "<br>")
}

fn row_line(cells: &[String]) -> String {
    let escaped: Vec<String> = cells.iter().map(|c| escape_cell(c)).collect();
    format!("| {} |", escaped.join(" | "))
}

/// A YAML scalar as text: a column called `2026` is a number to YAML and a
/// name to the table.
fn yaml_text(value: &serde_yaml::Value) -> Option<String> {
    match value {
        serde_yaml::Value::String(s) => Some(s.clone()),
        serde_yaml::Value::Number(n) => Some(n.to_string()),
        serde_yaml::Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

/// What the comment says, for this: the name, the formula and checkbox
/// columns, or why the table is to be left alone.
#[derive(Default)]
struct Settings {
    name: Option<String>,
    formulas: HashSet<String>,
    checkboxes: HashSet<String>,
    refused: Option<String>,
}

/// The comment from line `at`, read as the app reads it: the YAML between
/// `rich-table` and `-->`, through a YAML parser, so a quoted column name, a
/// block mapping and the short form `Tổng: formula` all count. Returns the
/// settings and the line after the comment.
fn read_comment(lines: &[&str], at: usize) -> (Settings, usize) {
    let refuse = |why: String, next: usize| (Settings { refused: Some(why), ..Settings::default() }, next);

    let start = lines[at].find("rich-table").map_or(0, |i| i + "rich-table".len());
    let mut yaml = String::new();
    let mut line = at;
    let mut rest = &lines[at][start..];
    let after = loop {
        if let Some(end) = rest.find("-->") {
            yaml.push_str(&rest[..end]);
            break &rest[end + 3..];
        }
        yaml.push_str(rest);
        yaml.push('\n');
        line += 1;
        if line == lines.len() {
            return refuse("This table's `<!-- rich-table` comment never closes, so the app shows it read-only. It was left as it is.".into(), line);
        }
        rest = lines[line];
    };
    let next = line + 1;
    if !after.trim().is_empty() {
        return refuse("This table's `<!-- rich-table` comment has text after its `-->`, so the app shows it read-only. It was left as it is.".into(), next);
    }

    let parsed: serde_yaml::Value = match serde_yaml::from_str(&yaml.replace("--\\>", "-->")) {
        Ok(v) => v,
        Err(e) => {
            return refuse(
                format!("This table's settings (the YAML in `<!-- rich-table`) do not parse ({e}), so the app shows it read-only. It was left as it is; fix the comment first."),
                next,
            )
        }
    };
    let meta = match parsed {
        serde_yaml::Value::Null => return (Settings::default(), next),
        serde_yaml::Value::Mapping(m) => m,
        _ => return refuse("This table's settings are not a YAML mapping, so the app shows it read-only. It was left as it is.".into(), next),
    };

    if let Some(version) = meta.get("version").and_then(serde_yaml::Value::as_f64) {
        if version > FORMAT_VERSION {
            return refuse(
                format!("This table was written by a newer Synabit (format version {version}), so this one shows it read-only. It was left as it is."),
                next,
            );
        }
    }

    let mut settings = Settings { name: meta.get("name").and_then(yaml_text), ..Settings::default() };
    if let Some(columns) = meta.get("columns").and_then(serde_yaml::Value::as_mapping) {
        for (key, spec) in columns {
            let Some(column) = yaml_text(key) else { continue };
            let declared = match spec {
                serde_yaml::Value::Mapping(m) => m.get("type").and_then(serde_yaml::Value::as_str),
                other => other.as_str(),
            };
            match declared {
                Some("formula") => settings.formulas.insert(column),
                Some("checkbox") => settings.checkboxes.insert(column),
                _ => false,
            };
        }
    }
    (settings, next)
}

/// The table whose header is line `at`, if one starts there, and the first
/// line after it — its comment included. `None` for the table when the lines
/// make an ordinary table, which is still skipped whole.
fn table_at(lines: &[&str], at: usize) -> Option<(Option<Table>, usize)> {
    let (header_line, _) = split_marker(lines[at]);
    if !header_line.starts_with('|') || continues_something(lines, at) {
        return None;
    }
    let (delimiter, _) = split_marker(lines.get(at + 1)?);
    if !is_row_line(delimiter) {
        return None;
    }
    let columns = split_row(header_line);
    let delimiters = split_row(delimiter);
    if delimiters.len() != columns.len() || !delimiters.iter().all(|c| is_delimiter_cell(c)) {
        return None;
    }

    let mut end = at + 2;
    while end < lines.len() && is_row_line(lines[end]) {
        end += 1;
    }
    if end == lines.len() || !is_comment(lines[end]) {
        return Some((None, end));
    }

    let (rows, markers) = lines[at + 2..end]
        .iter()
        .map(|line| {
            let (line, marker) = split_marker(line);
            (fit_row(split_row(line), columns.len()), marker.map(str::to_string))
        })
        .unzip();
    let (settings, next) = read_comment(lines, end);
    let table = Table {
        name: settings.name,
        columns,
        formulas: settings.formulas,
        checkboxes: settings.checkboxes,
        rows,
        refused: settings.refused,
        markers,
        header: at,
        end,
    };
    Some((Some(table), next))
}

/// The note's lines without their endings, `\r\n` or `\n`.
fn lines_of(body: &str) -> Vec<&str> {
    body.split_inclusive('\n')
        .map(|l| {
            let l = l.strip_suffix('\n').unwrap_or(l);
            l.strip_suffix('\r').unwrap_or(l)
        })
        .collect()
}

/// Every Rich Table in a body, in order, outside code fences and comments.
pub fn find_tables(body: &str) -> Vec<Table> {
    let lines = lines_of(body);
    let mut tables = Vec::new();
    let mut fence = None;
    let mut at = 0;
    while at < lines.len() {
        let line = lines[at];
        if let Some(open) = fence {
            if closes_fence(line, open) {
                fence = None;
            }
            at += 1;
            continue;
        }
        if let Some(open) = fence_of(line) {
            fence = Some(open);
            at += 1;
            continue;
        }
        if let Some((table, next)) = table_at(&lines, at) {
            tables.extend(table);
            at = next;
            continue;
        }
        // Some other comment: what is inside it is not shown, so not a table.
        if let Some(rest) = line.trim_start().strip_prefix("<!--") {
            if !rest.contains("-->") {
                at += 1;
                while at < lines.len() && !lines[at].contains("-->") {
                    at += 1;
                }
            }
        }
        at += 1;
    }
    tables
}

/// A JSON value as a cell holds it.
fn cell_of(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::Bool(b) => if *b { "[x]" } else { "[ ]" }.into(),
        Value::Number(n) => n.to_string(),
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// Plain numbers as `numberOf` reads them, and `45,000` as a person types one.
static PLAIN_NUMBER: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"^-?(\d+(\.\d*)?|\.\d+)([eE][-+]?\d+)?$").unwrap());
static GROUPED_NUMBER: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"^-?\d{1,3}(,\d{3})+(\.\d+)?$").unwrap());

fn number_of(text: &str) -> Option<f64> {
    if PLAIN_NUMBER.is_match(text) {
        text.parse().ok()
    } else if GROUPED_NUMBER.is_match(text) {
        text.replace(',', "").parse().ok()
    } else {
        None
    }
}

/// A tick, from however it was said: `[x]`, `true`; blank is unticked.
fn tick_of(text: &str) -> Option<bool> {
    match text.to_lowercase().as_str() {
        "[x]" | "true" => Some(true),
        "[ ]" | "[]" | "" | "false" => Some(false),
        _ => None,
    }
}

/// Text as it means: line breaks however written, as `\n`.
fn plain(text: &str) -> String {
    unescape_cell(&text.replace("\r\n", "\n").replace('\r', "\n"))
}

/// Whether a cell holds the value the model named. Numbers as numbers, so
/// `45000` finds `45000.0`; a tick as a tick, so `false` finds a blank
/// checkbox; text ignoring case, and with its line breaks however written.
fn same(cell: &str, value: &Value, checkbox: bool) -> bool {
    let cell = plain(cell);
    let wanted = plain(&cell_of(value));
    if checkbox || value.is_boolean() {
        if let (Some(a), Some(b)) = (tick_of(&cell), tick_of(&wanted)) {
            return a == b;
        }
    }
    if let (Some(a), Some(b)) = (number_of(&cell), number_of(&wanted)) {
        return a == b;
    }
    cell.to_lowercase() == wanted.to_lowercase()
}

/// One change asked of the rows: `where`, then `set`.
pub type Update = (Map<String, Value>, Map<String, Value>);

#[derive(Debug, Default, serde::Serialize)]
pub struct Outcome {
    pub table: String,
    pub added: usize,
    pub updated: usize,
    pub deleted: usize,
    pub columns: Vec<String>,
    /// Formula columns asked to be written, which were left to the app.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub formula_columns_skipped: Vec<String>,
}

/// Change the rows of one table in `body`.
///
/// `which` is the table's `name:`, or its number counting from 1; the first
/// table when absent. `add` is rows as `{column: value}`; `update` is
/// `{where: {…}, set: {…}}`; `delete` is `{column: value}` matches. A match is
/// every named column equal: see `same`.
pub fn apply(
    body: &str,
    which: Option<&str>,
    add: &[Map<String, Value>],
    update: &[Update],
    delete: &[Map<String, Value>],
) -> Result<(String, Outcome), String> {
    let tables = find_tables(body);
    if tables.is_empty() {
        return Err("This note has no Rich Table.".into());
    }
    let (index, table) = match which.map(str::trim).filter(|w| !w.is_empty()) {
        None => (0, &tables[0]),
        Some(w) => tables
            .iter()
            .enumerate()
            .find(|(_, t)| t.name.as_deref() == Some(w))
            .or_else(|| w.parse::<usize>().ok().and_then(|n| tables.get(n.wrapping_sub(1)).map(|t| (n - 1, t))))
            .ok_or_else(|| {
                let names: Vec<String> = tables
                    .iter()
                    .enumerate()
                    .map(|(i, t)| t.name.clone().unwrap_or_else(|| format!("{}", i + 1)))
                    .collect();
                format!("No table `{w}` in this note. Its tables: {}.", names.join(", "))
            })?,
    };
    if let Some(why) = &table.refused {
        return Err(why.clone());
    }

    let column_of = |name: &str| table.columns.iter().position(|c| c == name.trim());
    let check = |m: &Map<String, Value>| -> Result<(), String> {
        for key in m.keys() {
            if column_of(key).is_none() {
                return Err(format!("No column `{key}`. The columns: {}.", table.columns.join(", ")));
            }
        }
        Ok(())
    };
    for m in add.iter().chain(delete) {
        check(m)?;
    }
    for (w, s) in update {
        check(w)?;
        check(s)?;
    }

    let mut skipped: Vec<String> = Vec::new();
    let mut write = |row: &mut Vec<String>, set: &Map<String, Value>| {
        for (key, value) in set {
            let key = key.trim();
            if table.formulas.contains(key) {
                if !skipped.iter().any(|s| s == key) {
                    skipped.push(key.to_string());
                }
                continue;
            }
            row[column_of(key).unwrap()] = cell_of(value);
        }
    };
    let matches = |row: &[String], m: &Map<String, Value>| {
        m.iter().all(|(k, v)| same(&row[column_of(k).unwrap()], v, table.checkboxes.contains(k.trim())))
    };

    // Each row keeps its line, ending and all, unless it changed: an edit to
    // one row is a one-line change in the file, which is what sync merges best.
    let segments: Vec<&str> = body.split_inclusive('\n').collect();
    let eol = if segments[table.header].ends_with("\r\n") { "\r\n" } else { "\n" };
    let first_row = table.header + 2;
    let mut rows: Vec<(Vec<String>, Option<&str>, Option<&str>)> = table
        .rows
        .iter()
        .enumerate()
        .map(|(i, r)| (r.clone(), Some(segments[first_row + i]), table.markers[i].as_deref()))
        .collect();

    let before = rows.len();
    if !delete.is_empty() {
        rows.retain(|(r, _, _)| !delete.iter().any(|m| !m.is_empty() && matches(r, m)));
    }
    let deleted = before - rows.len();

    let mut updated = 0;
    for (w, s) in update {
        if w.is_empty() {
            return Err("An update needs `where`, naming the rows it changes.".into());
        }
        for (r, original, _) in rows.iter_mut() {
            if matches(r, w) {
                write(r, s);
                *original = None;
                updated += 1;
            }
        }
    }

    for m in add {
        let mut row = vec![String::new(); table.columns.len()];
        write(&mut row, m);
        rows.push((row, None, None));
    }

    let mut text: String = segments[..first_row].concat();
    for (r, original, marker) in rows {
        match original {
            Some(line) => text.push_str(line),
            None => {
                text.push_str(&row_line(&r));
                text.push_str(marker.unwrap_or(""));
                text.push_str(eol);
            }
        }
    }
    text.push_str(&segments[table.end..].concat());

    Ok((
        text,
        Outcome {
            table: table.name.clone().unwrap_or_else(|| format!("{}", index + 1)),
            added: add.len(),
            updated,
            deleted,
            columns: table.columns.clone(),
            formula_columns_skipped: skipped,
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const NOTE: &str = "# Chi tiêu\n\nText above.\n\n| Ngày | Khoản | Số tiền | Thuế | Xong |\n| --- | --- | ---: | ---: | :---: |\n| 2026-10-01 | Cà phê |   45000 | 4500 | [x] |\n| 2026-10-02 | Grab \\| xe | 62000 | 6200 | [ ] |\n<!-- rich-table\nname: chi-tieu\nversion: 1\ncolumns:\n  Số tiền: number\n  Thuế: { type: formula, expr: \"[Số tiền] * 0.1\" }\n  Xong: checkbox\n-->\n\nText below.\n";

    fn obj(v: Value) -> Map<String, Value> {
        v.as_object().unwrap().clone()
    }

    /// A small table with the given comment body, for the settings tests.
    fn with_comment(yaml: &str) -> String {
        format!("| A | B |\n| --- | --- |\n| 1 | 2 |\n<!-- rich-table\n{yaml}-->\n")
    }

    #[test]
    fn finds_the_table_its_name_columns_and_formulas() {
        let t = &find_tables(NOTE)[0];
        assert_eq!(t.name.as_deref(), Some("chi-tieu"));
        assert_eq!(t.columns, vec!["Ngày", "Khoản", "Số tiền", "Thuế", "Xong"]);
        assert!(t.formulas.contains("Thuế"));
        assert_eq!(t.rows[1][1], "Grab | xe");
    }

    #[test]
    fn adds_a_row_and_leaves_every_other_line_as_it_was() {
        let add = [obj(json!({ "Ngày": "2026-10-03", "Khoản": "Trà | sữa", "Số tiền": 30000, "Thuế": 999, "Xong": true }))];
        let (body, out) = apply(NOTE, Some("chi-tieu"), &add, &[], &[]).unwrap();
        assert!(body.contains("| 2026-10-03 | Trà \\| sữa | 30000 |  | [x] |\n<!-- rich-table"));
        assert!(body.contains("| 2026-10-01 | Cà phê |   45000 | 4500 | [x] |"), "an untouched row keeps its padding");
        assert!(body.starts_with("# Chi tiêu\n\nText above.\n") && body.ends_with("Text below.\n"));
        assert_eq!(out.added, 1);
        assert_eq!(out.formula_columns_skipped, vec!["Thuế"]);
    }

    #[test]
    fn updates_and_deletes_the_rows_that_match() {
        let update = [(obj(json!({ "Khoản": "cà PHÊ" })), obj(json!({ "Số tiền": 50000, "Xong": false })))];
        let delete = [obj(json!({ "Khoản": "Grab | xe" }))];
        let (body, out) = apply(NOTE, None, &[], &update, &delete).unwrap();
        assert!(body.contains("| 2026-10-01 | Cà phê | 50000 | 4500 | [ ] |"));
        assert!(!body.contains("Grab"));
        assert_eq!((out.updated, out.deleted), (1, 1));
    }

    #[test]
    fn says_what_is_there_when_a_name_is_wrong() {
        let err = apply(NOTE, Some("nope"), &[], &[], &[]).unwrap_err();
        assert!(err.contains("chi-tieu"), "{err}");
        let err = apply(NOTE, None, &[obj(json!({ "Giá": 1 }))], &[], &[]).unwrap_err();
        assert!(err.contains("No column `Giá`") && err.contains("Số tiền"), "{err}");
        assert!(apply("no tables here", None, &[], &[], &[]).is_err());
        let err = apply(NOTE, None, &[], &[(Map::new(), obj(json!({ "Khoản": "x" })))], &[]).unwrap_err();
        assert!(err.contains("where"), "{err}");
    }

    #[test]
    fn finds_a_table_by_its_number_and_ignores_an_ordinary_one() {
        let note = format!("| a | b |\n| --- | --- |\n| 1 | 2 |\n\n{NOTE}");
        let tables = find_tables(&note);
        assert_eq!(tables.len(), 1, "a table without the comment is an ordinary table");
        let (_, out) = apply(&note, Some("1"), &[], &[], &[]).unwrap();
        assert_eq!(out.table, "chi-tieu");
    }

    /// A table shown in a code example is an example. Editing it would change
    /// what the note teaches, not what it records.
    #[test]
    fn a_table_inside_a_code_fence_is_never_one() {
        for fence in ["```", "~~~", "````markdown"] {
            let close = &fence[..fence.bytes().take_while(|b| *b == fence.as_bytes()[0]).count()];
            let note = format!("Example:\n\n{fence}\n{}{close}\n", with_comment("name: shown\n"));
            assert!(find_tables(&note).is_empty(), "{fence}");
            assert!(apply(&note, None, &[obj(json!({ "A": "x" }))], &[], &[]).is_err());
        }
        // A shorter run does not close a longer fence; the real table after the
        // example is still found.
        let note = format!("````\n```\n{}````\n\n{NOTE}", with_comment(""));
        let tables = find_tables(&note);
        assert_eq!(tables.len(), 1);
        assert_eq!(tables[0].name.as_deref(), Some("chi-tieu"));
    }

    /// Only a table at the top of the note, with its comment flush under the
    /// last row, is one the app makes a Rich Table. Anything less certain is
    /// left alone.
    #[test]
    fn only_a_top_level_table_with_its_comment_right_under_it_counts() {
        let table = with_comment("");
        let indented: String = table.lines().map(|l| format!("    {l}\n")).collect();
        let in_list = format!("- an item\n{table}");
        let in_quote = format!("> quoted\n{table}");
        let comment_indented = table.replace("<!-- rich-table", "  <!-- rich-table");
        let gap = table.replace("| 1 | 2 |\n", "| 1 | 2 |\n\n");
        let bad_delimiter = table.replace("| --- | --- |", "| --- | x |");
        let short_delimiter = table.replace("| --- | --- |", "| --- |");
        let stray = table.replace("| 1 | 2 |\n", "| 1 | 2 |\n    | 3 | 4 |\n");
        for note in [indented, in_list, in_quote, comment_indented, gap, bad_delimiter, short_delimiter, stray] {
            assert!(find_tables(&note).is_empty(), "{note}");
        }
        assert_eq!(find_tables(&format!("# Heading\n{table}")).len(), 1);
        assert_eq!(find_tables(&table.replace("| --- | --- |", "|:--|--:|")).len(), 1);
    }

    /// A body row that happens to look like a delimiter does not start a new
    /// table in the middle of the one it is in.
    #[test]
    fn a_row_of_dashes_is_a_row_not_a_new_table() {
        let note = "| A | B |\n| --- | --- |\n| x | y |\n| --- | --- |\n| 1 | 2 |\n<!-- rich-table\n-->\n";
        let tables = find_tables(note);
        assert_eq!(tables.len(), 1);
        assert_eq!(tables[0].rows.len(), 3);
    }

    /// GFM drops the extra cells from view; the app folds them into the last
    /// cell so the next save does not lose them, and so does this.
    #[test]
    fn extra_cells_fold_into_the_last_and_a_block_marker_stays_put() {
        let note = "| A | B |\n| --- | --- |\n| 1 | 2 | 3 |  | 4 |\n| 5 | 6 | ^abc123\n<!-- rich-table\n-->\n";
        let t = &find_tables(note)[0];
        assert_eq!(t.rows[0], vec!["1", "2 | 3 | 4"]);
        assert_eq!(t.rows[1], vec!["5", "6"], "the marker is not a cell");

        let update = [(obj(json!({ "A": 1 })), obj(json!({ "A": "one" })))];
        let (body, _) = apply(note, None, &[], &update, &[]).unwrap();
        assert!(body.contains("| one | 2 \\| 3 \\| 4 |\n"), "{body}");

        let update = [(obj(json!({ "A": 5 })), obj(json!({ "B": "six" })))];
        let (body, _) = apply(note, None, &[], &update, &[]).unwrap();
        assert!(body.contains("| 5 | six | ^abc123\n"), "{body}");
    }

    /// The app shows these read-only, because rewriting them would rewrite
    /// settings it did not understand. The model is told why, in words.
    #[test]
    fn a_newer_or_unreadable_comment_is_refused_with_a_reason() {
        let add = [obj(json!({ "A": "x" }))];
        let newer = with_comment("version: 2\n");
        let err = apply(&newer, None, &add, &[], &[]).unwrap_err();
        assert!(err.contains("newer") && err.contains('2'), "{err}");

        let broken = with_comment("columns: [unclosed\n");
        let err = apply(&broken, None, &add, &[], &[]).unwrap_err();
        assert!(err.contains("do not parse"), "{err}");

        let unclosed = "| A |\n| --- |\n| 1 |\n<!-- rich-table\nname: x\n";
        assert!(apply(unclosed, None, &add, &[], &[]).unwrap_err().contains("never closes"));

        let trailing = "| A |\n| --- |\n| 1 |\n<!-- rich-table\n--> and more\n";
        assert!(apply(trailing, None, &add, &[], &[]).unwrap_err().contains("after its `-->`"));

        // One bad table does not stop another in the same note.
        let both = format!("{newer}\n{NOTE}");
        assert!(apply(&both, Some("chi-tieu"), &[obj(json!({ "Khoản": "x" }))], &[], &[]).is_ok());
        assert!(apply(&with_comment("version: 1\n"), None, &add, &[], &[]).is_ok());
    }

    /// Formula columns come from the parsed settings, however they are
    /// written: quoted names, block mappings, the short form.
    #[test]
    fn formula_columns_are_read_from_the_yaml_in_every_shape() {
        let note = "| \"Giá\" | Thuế: VAT | Tổng | Ghi chú |\n| --- | --- | --- | --- |\n| 1 | 2 | 3 | 4 |\n<!-- rich-table\ncolumns:\n  '\"Giá\"': number\n  \"Thuế: VAT\":\n    type: formula\n    expr: \"[Giá] * 0.1\"\n  Tổng: formula\n  Ghi chú: { type: text, expr: \"type: formula\" }\n-->\n";
        let t = &find_tables(note)[0];
        let mut formulas: Vec<&str> = t.formulas.iter().map(String::as_str).collect();
        formulas.sort();
        assert_eq!(formulas, vec!["Thuế: VAT", "Tổng"]);

        let add = [obj(json!({ "\"Giá\"": 5, "Thuế: VAT": 9, "Tổng": 9, "Ghi chú": "ok" }))];
        let (body, out) = apply(note, None, &add, &[], &[]).unwrap();
        assert!(body.contains("| 5 |  |  | ok |\n<!--"), "{body}");
        assert_eq!(out.formula_columns_skipped, vec!["Thuế: VAT", "Tổng"]);
    }

    /// Rows are found by what their cells mean, not how they are spelled.
    #[test]
    fn a_match_compares_numbers_ticks_and_line_breaks_by_meaning() {
        let note = "| Khoản | Số tiền | Xong |\n| --- | ---: | :---: |\n| Cà phê<br>sáng | 45000 |  |\n| Trà | 30000.0 | [X] |\n<!-- rich-table\ncolumns:\n  Xong: checkbox\n-->\n";
        let count = |m: Value| apply(note, None, &[], &[], &[obj(m)]).unwrap().1.deleted;
        assert_eq!(count(json!({ "Số tiền": 45000.0 })), 1);
        assert_eq!(count(json!({ "Số tiền": "45,000" })), 1);
        assert_eq!(count(json!({ "Số tiền": 30000 })), 1);
        assert_eq!(count(json!({ "Xong": false })), 1, "a blank checkbox is unticked");
        assert_eq!(count(json!({ "Xong": "[ ]" })), 1);
        assert_eq!(count(json!({ "Xong": true })), 1);
        assert_eq!(count(json!({ "Khoản": "cà phê\nsáng" })), 1);
        assert_eq!(count(json!({ "Khoản": "Cà phê<br/>sáng" })), 1);
        assert_eq!(count(json!({ "Khoản": "Cà phê" })), 0);
        assert_eq!(count(json!({ "Số tiền": 4500 })), 0);
    }

    /// A file written on Windows stays a Windows file: the new row ends the
    /// way its neighbours do, and nothing else changes.
    #[test]
    fn a_crlf_note_keeps_its_line_endings() {
        let crlf = NOTE.replace('\n', "\r\n");
        let add = [obj(json!({ "Khoản": "Bánh" }))];
        let update = [(obj(json!({ "Khoản": "Grab | xe" })), obj(json!({ "Số tiền": 1 })))];
        let (body, _) = apply(&crlf, None, &add, &update, &[]).unwrap();
        assert_eq!(body.matches('\n').count(), body.matches("\r\n").count(), "{body:?}");
        assert!(body.contains("| 2026-10-02 | Grab \\| xe | 1 | 6200 | [ ] |\r\n"), "{body:?}");
        assert!(body.contains("|  | Bánh |  |  |  |\r\n<!-- rich-table\r\n"), "{body:?}");
        assert!(body.ends_with("Text below.\r\n"));
    }

    #[test]
    fn every_kind_of_line_break_is_escaped_as_br() {
        assert_eq!(escape_cell("a\r\nb\rc\nd | e"), "a<br>b<br>c<br>d \\| e");
        assert_eq!(split_row(&row_line(&["a\r\nb".into(), "c|d".into()])), vec!["a\nb", "c|d"]);
    }
}
