//! Turning an answer into a different answer.
//!
//! §7 of `docs/query-grammar-2026-09-20.md`. The filter half of a question
//! asks the database which rows; the pipeline takes those rows and makes
//! something else of them — a count per month, the busiest five places.
//!
//! # Why this runs in Rust and not in SQL
//!
//! `stats count by month` would be a `GROUP BY`, and that would be faster.
//! But the slot next to it is `seq gaps by with` — how long between one
//! meeting and the next — and that is not a `GROUP BY` in any dialect this
//! app can rely on. Writing half the pipeline as SQL and half as Rust would
//! mean two places where a stage can mean something, and they would drift.
//!
//! So the database answers the question and the pipeline answers what to do
//! with the answer, over rows already in hand. §8 fixes the price of that
//! honestly: the filter runs to an internal ceiling, and when it hits the
//! ceiling the answer **says so**. A count over the first five thousand is a
//! fine answer; a count over the first five thousand presented as a count is
//! not.
//!
//! # Columns are the stage's to decide
//!
//! §7.2. `stats` throws away the columns it was given and makes two of its
//! own, because that is what it means. The shape chooser reads columns
//! (`shared/views/shapeFor`), so this is also how an answer comes back
//! knowing it wants to be drawn as bars.

use crate::db::{QueryResult, QueryRow};
use crate::error::{AppError, AppResult};
use crate::query::{Bucket, Opened, Operand, Query, Sequence, Stage, Tally, Test};
use crate::refusal::Refusal;
use crate::search::Comparison;

/// The most rows the filter half may hand the pipeline.
///
/// §8 proposed five thousand. A count over more than this is a count nobody
/// reads to the last digit; a scan over more than this is one nobody asked
/// for. What matters is not the number but that reaching it is reported.
pub const CEILING: u32 = 5_000;

/// What the answer has to admit when the filter half stopped at the ceiling.
///
/// One place rather than the same sentence written out in both runners, and a
/// function rather than three lines inline, because a rule nothing can call is
/// a rule nothing can test — and this one only fires on a vault far larger
/// than any test would build.
pub fn hit_the_ceiling(query: &Query, returned: usize, total: usize) -> Option<String> {
    (!query.stages.is_empty() && returned as u32 >= CEILING)
        .then(|| format!("over the first {CEILING} of {total} matches"))
}

/// A day written the way every dated column in this app writes one.
fn day_of(cell: &str) -> Option<chrono::NaiveDate> {
    chrono::NaiveDate::parse_from_str(cell.get(..10)?, "%Y-%m-%d").ok()
}

/// Which column holds a day, or `None`.
///
/// The same rule as `shared/views/shapeFor.ts` on the other side: the first
/// column whose filled cells are mostly days. Names are a hint and the cells
/// are the evidence, because a column called `hạn_chót` in somebody's own
/// schema is a date too and a column called `date` full of "hôm qua" is not.
fn dated_column(result: &QueryResult) -> Option<usize> {
    /// Enough of a column's cells being days to call it a date column.
    const MOSTLY: f64 = 0.8;

    for at in 0..result.columns.len() {
        let filled: Vec<&str> = result
            .rows
            .iter()
            .filter_map(|row| row.cells.get(at))
            .map(|cell| cell.trim())
            .filter(|cell| !cell.is_empty())
            .collect();
        if filled.is_empty() {
            continue;
        }
        let days = filled.iter().filter(|cell| day_of(cell).is_some()).count();
        if days as f64 >= filled.len() as f64 * MOSTLY {
            return Some(at);
        }
    }
    None
}

impl Bucket {
    /// What one row is gathered under, or `None` when it cannot be.
    fn label(&self, row: &QueryRow, result: &QueryResult, dated: Option<usize>) -> Option<String> {
        match self {
            Bucket::Field(name) => {
                let at = result.columns.iter().position(|c| c == name)?;
                Some(row.cells.get(at)?.trim().to_string()).filter(|v| !v.is_empty())
            }
            _ => {
                let day = day_of(row.cells.get(dated?)?.trim())?;
                Some(match self {
                    Bucket::Day => day.format("%Y-%m-%d").to_string(),
                    // Monday's date, so a week sorts beside its neighbours and
                    // reads as a date rather than as "2026-W38".
                    Bucket::Week => {
                        use chrono::Datelike;
                        (day - chrono::Duration::days(
                            day.weekday().num_days_from_monday() as i64
                        ))
                        .format("%Y-%m-%d")
                        .to_string()
                    }
                    Bucket::Month => day.format("%Y-%m").to_string(),
                    _ => day.format("%Y").to_string(),
                })
            }
        }
    }
}

/// The vault's own words, for `explode sentences`.
///
/// A trait rather than a `&DbBridge`, and for one reason: **what comes back
/// has already been through the consent layer**. A sentence on a hushed day,
/// or hushed by itself, must not be here at all — not filtered out further
/// down, not left out of the answer. Never read.
///
/// Passing the database would have made that a rule somebody has to remember.
/// Passing this makes it a rule the caller cannot avoid having thought about.
pub trait Words {
    /// The sentences of one note on one day, or nothing.
    ///
    /// The day is passed because the consent layer works on pairs: a hush can
    /// cover a stretch of time rather than a note. Reading the note without
    /// knowing which day it was reached through would let a lens walk round it.
    fn sentences_of(&self, node: &str, day: &str) -> Vec<String>;
}

/// Spending money.
///
/// Given a numbered list, the model points at the ones to keep. It is never
/// asked for prose and the reply has nowhere to put any — the same protocol
/// `timeline::year` uses, for the same reason.
pub trait Asks {
    fn keep(&self, lines: &[String], room: usize) -> AppResult<Vec<usize>>;
}

/// What the pipeline is allowed to reach outside itself for.
///
/// Both halves are absent by default, and each stage that needs one refuses by
/// name when it is missing. That is the guardrail of §13.3 built into the
/// shape rather than bolted on: **opening a saved lens cannot spend money**,
/// because the ordinary path has no asker to spend it with.
pub struct Around<'a> {
    pub today: chrono::NaiveDate,
    pub words: Option<&'a dyn Words>,
    pub asker: Option<&'a dyn Asks>,
}

impl Around<'_> {
    /// Nothing but a calendar. What every free path uses.
    pub fn bare(today: chrono::NaiveDate) -> Self {
        Around { today, words: None, asker: None }
    }
}

/// Run every stage over an answer, in order.
pub fn run(query: &Query, result: QueryResult) -> AppResult<QueryResult> {
    run_on(query, result, chrono::Local::now().date_naive())
}

/// The same, told what day it is — `gaps` measures silence up to now.
pub fn run_on(
    query: &Query,
    result: QueryResult,
    today: chrono::NaiveDate,
) -> AppResult<QueryResult> {
    run_around(query, result, &Around::bare(today))
}

/// The same, with whatever the caller is willing to let it reach for.
pub fn run_around(
    query: &Query,
    mut result: QueryResult,
    around: &Around<'_>,
) -> AppResult<QueryResult> {
    for stage in &query.stages {
        result = one(stage, result, around)?;
    }
    Ok(result)
}

fn one(stage: &Stage, result: QueryResult, around: &Around<'_>) -> AppResult<QueryResult> {
    match stage {
        Stage::Stats { tally, by } => stats(*tally, by, result),
        Stage::Seq { sequence, by } => seq(*sequence, by, result, around.today),
        Stage::Where(test) => keeping(test, result),
        Stage::Explode(opened) => explode(*opened, result, around),
        Stage::Ask(room) => asking(*room as usize, result, around),
        Stage::Sort { key, descending } => Ok(sorted(key, *descending, result)),
        Stage::Head(n) => Ok(QueryResult {
            rows: result.rows.into_iter().take(*n as usize).collect(),
            ..result
        }),
    }
}

/// The most lines `explode sentences` will make.
///
/// The same ceiling as everything else, for the same reason — and it matters
/// more here, because the rows after this one are what a model would be handed.
pub const MOST_LINES: usize = 2_000;

/// Make a long list fit **without losing its shape**.
///
/// Every *n*-th line rather than the first `MOST_LINES`. `timeline::year`
/// learnt this and wrote down why: taking the first of them would hand the
/// model January and call it a year. The months stay in proportion to how much
/// was written in them, which is the one property a year of somebody's writing
/// has that must not be lost — and the same is true of any stretch somebody
/// asks about.
fn thinned(all: Vec<QueryRow>) -> (Vec<QueryRow>, bool) {
    if all.len() <= MOST_LINES {
        return (all, false);
    }
    let step = all.len() as f64 / MOST_LINES as f64;
    let kept = (0..MOST_LINES)
        .map(|n| all[(n as f64 * step) as usize].clone())
        .collect();
    (kept, true)
}

/// One row per sentence: `day, note, text` (§7.2).
fn explode(opened: Opened, result: QueryResult, around: &Around<'_>) -> AppResult<QueryResult> {
    let Opened::Sentences = opened;
    let Some(words) = around.words else {
        return Err(AppError::Refused(Refusal::needs_the_vault()));
    };
    let dated = dated_column(&result);

    let mut rows: Vec<QueryRow> = Vec::new();
    // One note is read once, however many rows lead back to it. Three events
    // on one day come from one note, and without this its sentences would be
    // shown three times — to a person as repetition, and to a model as three
    // votes for the same line. `year::candidates` deduplicates for the same
    // reason.
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    for row in &result.rows {
        let note = row.open.clone().unwrap_or_else(|| row.id.clone());
        if !seen.insert(note.clone()) {
            continue;
        }
        let day = dated
            .and_then(|at| row.cells.get(at))
            .map(|cell| cell.trim().to_string())
            .unwrap_or_default();
        for (n, text) in words.sentences_of(&note, &day).into_iter().enumerate() {
            rows.push(QueryRow {
                // One sentence is not a node, but it has to key uniquely — one
                // note makes many of these.
                id: format!("{note}#{n}"),
                node_type: String::new(),
                title: text.clone(),
                cells: vec![day.clone(), note.clone(), text],
                open: Some(note.clone()),
                until: None,
            });
        }
    }

    let (rows, thin) = thinned(rows);
    let total = rows.len();
    let left_out = thin.then(|| format!("{MOST_LINES} lines spread across all of them"));
    Ok(QueryResult {
        columns: vec!["day".into(), "note".into(), "text".into()],
        rows,
        total,
        query_time_ms: result.query_time_ms,
        note: match (result.note, left_out) {
            (Some(had), Some(left)) => Some(format!("{had}; {left}")),
            (had, left) => had.or(left),
        },
    })
}

/// Let the model keep some of the rows, and no more than that.
///
/// It is handed a numbered list and gives back numbers. There is nowhere in
/// the reply for prose, so there is nothing to check for invention: a number
/// that is not on the list is dropped, and what comes back is the person's own
/// sentences or nothing.
fn asking(room: usize, result: QueryResult, around: &Around<'_>) -> AppResult<QueryResult> {
    // Nothing to choose between, so asking would cost a call to be told the
    // same list back.
    if result.rows.len() <= room {
        return Ok(result);
    }
    let Some(asker) = around.asker else {
        // The guardrail, and it is priced. §13.3: a saved lens must not spend
        // money by being opened, and what it would spend has to be visible
        // before anybody agrees to it.
        return Err(AppError::Refused(Refusal::would_spend(room, result.rows.len())));
    };

    let picked = asker.keep(&lines_of(&result), room)?;
    Ok(keeping_picked(result, &picked, room))
}

/// What the model is shown, one line per row.
///
/// The last column, because that is the one `explode sentences` puts the words
/// in and the one a person would read.
pub fn lines_of(result: &QueryResult) -> Vec<String> {
    result
        .rows
        .iter()
        .map(|row| row.cells.last().cloned().unwrap_or_else(|| row.title.clone()))
        .collect()
}

/// The rows the model pointed at, and no others.
///
/// Three things, each of which `timeline::year` had to learn first:
///
/// 1. A number nobody offered is **dropped**, not wrapped around, so a model
///    that miscounts cannot reach a line it was never shown.
/// 2. The same number twice is **one row**. A model repeating itself must not
///    make a sentence appear twice.
/// 3. They come back **in the order they were offered**, not the order the
///    model listed them. The question decided the order — `sort:` is part of
///    it — so for a year of somebody's sentences that is the order it was
///    lived, which is the only order in which reading them together means
///    anything.
pub fn keeping_picked(result: QueryResult, picked: &[usize], room: usize) -> QueryResult {
    let mut wanted: Vec<usize> = Vec::new();
    for n in picked {
        if *n < result.rows.len() && !wanted.contains(n) {
            wanted.push(*n);
        }
        if wanted.len() >= room {
            break;
        }
    }
    wanted.sort_unstable();
    let kept: Vec<QueryRow> = wanted.into_iter().map(|n| result.rows[n].clone()).collect();
    let total = kept.len();
    QueryResult { rows: kept, total, ..result }
}

/// What `seq gaps` says about each thread of rows.
///
/// §7.2: `who, times, first, last, span, quiet, longest`. Every number is in
/// days, and `quiet` is measured **up to today** rather than to the last row —
/// which is the whole point. A friendship with a six-month hole in the middle
/// and a coffee last week is not a silence; one with no hole at all and
/// nothing since March is.
///
/// `span` — first to last — is not in §7.2's list. It is here because the
/// panel this replaces will not go without it: `timeline::silence` says
/// nothing about somebody it has only known for a fortnight, however loud the
/// quiet, and `where` cannot subtract one column from another. The spec's list
/// was written before the panel's own rule was read.
fn seq(
    sequence: Sequence,
    by: &Bucket,
    result: QueryResult,
    today: chrono::NaiveDate,
) -> AppResult<QueryResult> {
    let Sequence::Gaps = sequence;
    let dated = dated_column(&result).ok_or_else(|| {
        AppError::Refused(Refusal::no_day_to_follow())
    })?;
    if let Bucket::Field(name) = by {
        if !result.columns.iter().any(|c| c == name) {
            return Err(AppError::Refused(Refusal::not_a_column_of_this_answer(
                name,
                &result.columns.join(", "),
            )));
        }
    }

    let mut order: Vec<String> = Vec::new();
    let mut days: std::collections::HashMap<String, Vec<chrono::NaiveDate>> = Default::default();
    let mut without = 0usize;
    for row in &result.rows {
        let (Some(label), Some(day)) = (
            by.label(row, &result, Some(dated)),
            row.cells.get(dated).and_then(|cell| day_of(cell.trim())),
        ) else {
            without += 1;
            continue;
        };
        // One row can name several people — `who` comes back as a list — and
        // each of them was there. Splitting means a gap is a gap in *that*
        // thread rather than in a coincidence of spellings.
        for one in label.split(',').map(str::trim).filter(|l| !l.is_empty()) {
            let seen = days.entry(one.to_string()).or_insert_with(|| {
                order.push(one.to_string());
                Vec::new()
            });
            seen.push(day);
        }
    }

    let total = order.len();
    let rows = order
        .into_iter()
        .map(|label| {
            let mut seen = days.remove(&label).unwrap_or_default();
            seen.sort_unstable();
            let first = seen.first().copied().unwrap_or(today);
            let last = seen.last().copied().unwrap_or(today);
            let longest = seen
                .windows(2)
                .map(|pair| (pair[1] - pair[0]).num_days())
                .max()
                .unwrap_or(0);
            QueryRow {
                id: label.clone(),
                node_type: String::new(),
                title: label.clone(),
                cells: vec![
                    label,
                    seen.len().to_string(),
                    first.format("%Y-%m-%d").to_string(),
                    last.format("%Y-%m-%d").to_string(),
                    (last - first).num_days().to_string(),
                    (today - last).num_days().max(0).to_string(),
                    longest.to_string(),
                ],
                open: None,
                until: None,
            }
        })
        .collect();

    let left_out = (without > 0).then(|| format!("{without} with no {}", by.column()));
    Ok(QueryResult {
        columns: vec![
            by.column(),
            "times".into(),
            "first".into(),
            "last".into(),
            "span".into(),
            "quiet".into(),
            "longest".into(),
        ],
        rows,
        total,
        query_time_ms: result.query_time_ms,
        note: match (result.note, left_out) {
            (Some(had), Some(left)) => Some(format!("{had}; {left}")),
            (had, left) => had.or(left),
        },
    })
}

/// Keep the rows that pass a test.
fn keeping(test: &Test, mut result: QueryResult) -> AppResult<QueryResult> {
    let columns = result.columns.clone();
    result.rows.retain(|row| passes(test, row, &columns));
    result.total = result.rows.len();
    Ok(result)
}

fn passes(test: &Test, row: &QueryRow, columns: &[String]) -> bool {
    match test {
        Test::All(parts) => parts.iter().all(|part| passes(part, row, columns)),
        Test::Any(parts) => parts.iter().any(|part| passes(part, row, columns)),
        Test::Nope(inner) => !passes(inner, row, columns),
        Test::Equals(left, right, same) => {
            let (a, b) = (read(left, row, columns), read(right, row, columns));
            (a.eq_ignore_ascii_case(&b)) == *same
        }
        Test::Compare(left, operator, right) => {
            let (a, b) = (read(left, row, columns), read(right, row, columns));
            // Numbers as numbers, everything else as text — so `quiet > 100`
            // is arithmetic and `last > 2026-01-01` is still a date comparison,
            // which works because a day written this way sorts as it counts.
            match (a.parse::<f64>(), b.parse::<f64>()) {
                (Ok(a), Ok(b)) => match operator {
                    Comparison::GreaterThan => a > b,
                    Comparison::GreaterOrEqual => a >= b,
                    Comparison::LessThan => a < b,
                    Comparison::LessOrEqual => a <= b,
                },
                _ => match operator {
                    Comparison::GreaterThan => a > b,
                    Comparison::GreaterOrEqual => a >= b,
                    Comparison::LessThan => a < b,
                    Comparison::LessOrEqual => a <= b,
                },
            }
        }
    }
}

/// One side of a comparison, as the text to compare.
///
/// A column name nothing matches reads as empty rather than as its own name:
/// `where nonsense > 5` should find nothing, not compare the word "nonsense".
fn read(operand: &Operand, row: &QueryRow, columns: &[String]) -> String {
    match operand {
        Operand::Column(name) => columns
            .iter()
            .position(|c| c == name)
            .and_then(|at| row.cells.get(at))
            .map(|cell| cell.trim().to_string())
            .unwrap_or_default(),
        Operand::Number(n) | Operand::Days(n) => {
            if n.fract() == 0.0 { format!("{n:.0}") } else { n.to_string() }
        }
        Operand::Text(text) => text.clone(),
    }
}

fn stats(tally: Tally, by: &Bucket, result: QueryResult) -> AppResult<QueryResult> {
    let dated = dated_column(&result);
    if matches!(by, Bucket::Day | Bucket::Week | Bucket::Month | Bucket::Year) && dated.is_none() {
        // §9: refuse rather than answer a different question. Gathering by
        // month when nothing in the rows is a day would silently put every row
        // in one heap called "".
        return Err(AppError::Refused(Refusal::no_day_to_gather_by(&by.column())));
    }
    if let Bucket::Field(name) = by {
        if !result.columns.iter().any(|c| c == name) {
            return Err(AppError::Refused(Refusal::not_a_column_of_this_answer(
                name,
                &result.columns.join(", "),
            )));
        }
    }

    // Kept in the order the labels first appeared, rather than sorted: the
    // rows arrived in the order the question asked for, and `| sort` is how
    // somebody says they wanted another.
    let mut order: Vec<String> = Vec::new();
    let mut counts: std::collections::HashMap<String, i64> = std::collections::HashMap::new();
    // Rows with nothing to gather under. A task with no status does not belong
    // in any heap of statuses — but losing it in silence is how counting by
    // status comes back as 1 when there are 3 things. So they are left out and
    // **said**.
    let mut without = 0usize;
    for row in &result.rows {
        let Some(label) = by.label(row, &result, dated) else {
            without += 1;
            continue;
        };
        if counts.insert(label.clone(), counts.get(&label).unwrap_or(&0) + 1).is_none() {
            order.push(label);
        }
    }

    let total = order.len();
    let left_out = (without > 0).then(|| {
        format!("{without} with no {}", by.column())
    });
    let note = match (result.note, left_out) {
        (Some(had), Some(left)) => Some(format!("{had}; {left}")),
        (had, left) => had.or(left),
    };
    Ok(QueryResult {
        columns: vec![by.column(), tally.column().to_string()],
        rows: order
            .into_iter()
            .map(|label| QueryRow {
                // The id is the label: there is no node behind a heap of rows,
                // and a view keys its rows by this.
                id: label.clone(),
                node_type: String::new(),
                title: label.clone(),
                cells: vec![label.clone(), counts[&label].to_string()],
                open: None,
                until: None,
            })
            .collect(),
        total,
        query_time_ms: result.query_time_ms,
        note,
    })
}

fn sorted(key: &str, descending: bool, mut result: QueryResult) -> QueryResult {
    let Some(at) = result.columns.iter().position(|c| c == key) else {
        // Not a column of this answer. Left alone rather than refused: `sort`
        // says nothing about which rows match, so a name that is not there
        // costs an order, not an answer.
        return result;
    };
    result.rows.sort_by(|a, b| {
        let (x, y) = (
            a.cells.get(at).map(String::as_str).unwrap_or(""),
            b.cells.get(at).map(String::as_str).unwrap_or(""),
        );
        // Numbers as numbers: "10" sorts after "9", which sorting as text does
        // not do — and every column `stats` makes is numbers.
        let order = match (x.parse::<f64>(), y.parse::<f64>()) {
            (Ok(x), Ok(y)) => x.total_cmp(&y),
            _ => x.cmp(y),
        };
        if descending { order.reverse() } else { order }
    });
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::query::parse;

    /// §8. A count over the first five thousand is a fine answer; a count over
    /// the first five thousand **presented as a count** is not.
    #[test]
    fn an_answer_says_when_it_only_saw_the_first_of_the_matches() {
        let piped = parse("is:note | stats count by month");
        let plain = parse("is:note");

        assert_eq!(
            hit_the_ceiling(&piped, CEILING as usize, 12_000).as_deref(),
            Some("over the first 5000 of 12000 matches")
        );
        assert!(
            hit_the_ceiling(&piped, 4_999, 4_999).is_none(),
            "nothing was cut, so there is nothing to own up to"
        );
        assert!(
            hit_the_ceiling(&plain, CEILING as usize, 12_000).is_none(),
            "without a pipeline the rows are a page, and a page is not a claim \
             about the whole"
        );
    }

    fn answer(columns: &[&str], rows: &[&[&str]]) -> QueryResult {
        QueryResult {
            columns: columns.iter().map(|c| c.to_string()).collect(),
            rows: rows
                .iter()
                .enumerate()
                .map(|(i, cells)| QueryRow {
                    id: format!("Notes/{i}.md"),
                    node_type: "note".into(),
                    title: format!("row {i}"),
                    cells: cells.iter().map(|c| c.to_string()).collect(),
                    open: None,
                    until: None,
                })
                .collect(),
            total: rows.len(),
            query_time_ms: 0,
            note: None,
        }
    }

    fn ask(q: &str, result: QueryResult) -> AppResult<QueryResult> {
        run(&parse(q), result)
    }

    /// §7.3: `by month` gathers by the row's **day**, not by a field called
    /// month — no row has a field called month.
    #[test]
    fn a_time_bucket_gathers_by_the_day_in_the_row() {
        let events = answer(
            &["when", "title"],
            &[
                &["2019-11-05", "a"],
                &["2019-11-20", "b"],
                &["2021-03-14", "c"],
            ],
        );
        let got = ask("events | stats count by month", events).expect("runs");
        assert_eq!(got.columns, ["month", "count"]);
        assert_eq!(
            got.rows.iter().map(|r| r.cells.clone()).collect::<Vec<_>>(),
            [vec!["2019-11".to_string(), "2".into()], vec!["2021-03".into(), "1".into()]]
        );
        assert_eq!(got.total, 2, "the total is now how many heaps there are");
    }

    /// And when there is no day to gather by, it refuses — putting every row
    /// in one heap called "" is the silent wrong answer this whole document
    /// exists to stop.
    #[test]
    fn gathering_by_a_month_that_is_not_there_is_refused() {
        let plain = answer(&["title", "author"], &[&["a", "Nguyễn"]]);
        let why = ask("notes | stats count by month", plain)
            .expect_err("no day in the rows")
            .to_string();
        assert!(why.contains("no day"), "{why}");
    }

    /// A task with no status does not belong in any heap of statuses — but
    /// losing it in silence is how counting by status comes back as 1 when
    /// there are 3 things.
    #[test]
    fn rows_with_nothing_to_gather_under_are_left_out_and_said() {
        let tasks = answer(
            &["title", "status"],
            &[&["a", "done"], &["b", "done"], &["c", ""]],
        );
        let got = ask("notes | stats count by status", tasks).expect("runs");
        assert_eq!(got.rows.len(), 1);
        assert_eq!(got.rows[0].cells, ["done", "2"]);
        assert_eq!(got.note.as_deref(), Some("1 with no status"));
    }

    /// Numbers as numbers: sorted as text, "10" comes before "9", and every
    /// column `stats` makes is numbers.
    #[test]
    fn sorting_a_tally_sorts_by_size_and_not_by_spelling() {
        let mut cells: Vec<&[&str]> = vec![&["2019-01-01", "a"]; 10];
        cells.extend(vec![&["2021-01-01", "b"] as &[&str]; 9]);
        let events = answer(&["when", "title"], &cells);
        let got = ask("events | stats count by year | sort count desc", events).expect("runs");
        assert_eq!(
            got.rows.iter().map(|r| r.cells[1].clone()).collect::<Vec<_>>(),
            ["10", "9"]
        );
    }

    /// `top n by key` is sugar, and says so: sort descending, then take n.
    #[test]
    fn top_is_sort_desc_then_head() {
        let sugar = parse("events | stats count by year | top 1 by count");
        let spelt = parse("events | stats count by year | sort count desc | head 1");
        assert_eq!(sugar.stages, spelt.stages);
    }

    fn day(y: i32, m: u32, d: u32) -> chrono::NaiveDate {
        chrono::NaiveDate::from_ymd_opt(y, m, d).expect("a real day")
    }

    /// §7.2, and the whole of the silence panel: `quiet` is measured **up to
    /// today**, not to the last row. A friendship with a six-month hole in the
    /// middle and a coffee last week is not a silence; one with no hole at all
    /// and nothing since March is.
    #[test]
    fn gaps_measures_the_silence_up_to_now_and_the_longest_one_before_it() {
        let events = answer(
            &["when", "who"],
            &[
                &["2026-01-01", "Khánh"],
                &["2026-01-08", "Khánh"],
                &["2026-06-01", "Khánh"],
                &["2026-01-01", "Minh"],
            ],
        );
        let got = run_on(
            &parse("events | seq gaps by who"),
            events,
            day(2026, 9, 20),
        )
        .expect("runs");

        assert_eq!(
            got.columns,
            ["who", "times", "first", "last", "span", "quiet", "longest"]
        );
        assert_eq!(
            got.rows[0].cells,
            ["Khánh", "3", "2026-01-01", "2026-06-01", "151", "111", "144"]
        );
        // Minh has one row, so there is no gap between rows at all — and the
        // silence since is the whole of it.
        assert_eq!(
            got.rows[1].cells,
            ["Minh", "1", "2026-01-01", "2026-01-01", "0", "262", "0"]
        );
    }

    /// One row can name several people, and each of them was there. Splitting
    /// means a gap is a gap in *that* thread rather than in a coincidence of
    /// spellings.
    #[test]
    fn a_row_naming_two_people_counts_for_both() {
        let events = answer(&["when", "who"], &[&["2026-01-01", "Khánh, Minh"]]);
        let got = run_on(&parse("events | seq gaps by who"), events, day(2026, 1, 2))
            .expect("runs");
        assert_eq!(
            got.rows.iter().map(|r| r.cells[0].clone()).collect::<Vec<_>>(),
            ["Khánh", "Minh"]
        );
        assert!(got.rows.iter().all(|r| r.cells[1] == "1"));
    }

    /// **The gate for this step.** `timeline::silence` is a hand-written
    /// module with three thresholds in it; this is the same three thresholds
    /// written as a question, answering the same about the same people.
    ///
    /// Its rule, from `silence.rs`: seen at least 5 times, known at least 183
    /// days, and quiet for longer than both its own longest gap and 90 days.
    /// The `max` of the last two is two comparisons joined by `and`, which is
    /// the same thing.
    #[test]
    fn the_silence_panel_is_a_question() {
        const PANEL: &str = "events | seq gaps by who \
             | where times >= 5 and span >= 183d and quiet > longest and quiet > 90d";
        let today = day(2026, 9, 20);

        let cells: Vec<&[&str]> = vec![
            // Seen plenty over a year, and seen last week: not a silence.
            &["2025-06-01", "Khánh"],
            &["2025-09-01", "Khánh"],
            &["2025-12-01", "Khánh"],
            &["2026-03-01", "Khánh"],
            &["2026-09-14", "Khánh"],
            // Seen just as often over just as long, and nothing for a year.
            &["2024-06-01", "Minh"],
            &["2024-09-01", "Minh"],
            &["2024-12-01", "Minh"],
            &["2025-03-01", "Minh"],
            &["2025-06-01", "Minh"],
            // Known for two days, however loud the quiet since: the panel says
            // nothing about somebody it has only just met, and neither does
            // this.
            &["2020-01-01", "Tuấn"],
            &["2020-01-01", "Tuấn"],
            &["2020-01-01", "Tuấn"],
            &["2020-01-02", "Tuấn"],
            &["2020-01-02", "Tuấn"],
        ];

        let got = run_on(&parse(PANEL), answer(&["when", "who"], &cells), today)
            .expect("runs");
        assert_eq!(
            got.rows.iter().map(|r| r.cells[0].clone()).collect::<Vec<_>>(),
            ["Minh"],
            "{:?}",
            got.rows
        );
        assert_eq!(got.total, 1, "the total follows the rows a `where` kept");
    }

    /// `6mo` is a stretch of days, so it can be compared with a column of
    /// days. Without that reading it would be the word "6mo" against a number.
    #[test]
    fn a_stretch_of_time_compares_against_a_column_of_days() {
        let events = answer(
            &["when", "who"],
            &[&["2026-09-01", "Khánh"], &["2024-01-01", "Minh"]],
        );
        let kept = |q: &str| {
            run_on(&parse(q), events.clone(), day(2026, 9, 20))
                .expect("runs")
                .rows
                .iter()
                .map(|r| r.cells[0].clone())
                .collect::<Vec<_>>()
        };
        assert_eq!(kept("events | seq gaps by who | where quiet > 6mo"), ["Minh"]);
        assert_eq!(kept("events | seq gaps by who | where quiet < 6mo"), ["Khánh"]);
    }

    /// `AND`, `OR` and `NOT` inside a `where`, which §7 ships with it.
    #[test]
    fn a_where_can_join_two_comparisons() {
        let events = answer(
            &["when", "who"],
            &[
                &["2026-09-01", "Khánh"],
                &["2026-09-02", "Khánh"],
                &["2024-01-01", "Minh"],
            ],
        );
        let kept = |q: &str| {
            run_on(&parse(q), events.clone(), day(2026, 9, 20))
                .expect("runs")
                .rows
                .iter()
                .map(|r| r.cells[0].clone())
                .collect::<Vec<_>>()
        };
        assert_eq!(
            kept("events | seq gaps by who | where times > 1 and quiet < 6mo"),
            ["Khánh"]
        );
        assert_eq!(
            kept("events | seq gaps by who | where times > 1 or quiet > 6mo").len(),
            2
        );
        assert_eq!(kept("events | seq gaps by who | where not times > 1"), ["Minh"]);
    }

    /// A `Words` that hands back whatever it was given, and remembers what it
    /// was asked for — so a test can see what reached it and what did not.
    struct Said(std::cell::RefCell<Vec<(String, String)>>, Vec<String>);

    impl Words for Said {
        fn sentences_of(&self, node: &str, day: &str) -> Vec<String> {
            self.0.borrow_mut().push((node.to_string(), day.to_string()));
            self.1.clone()
        }
    }

    /// An `Asks` that keeps whatever it is told to, and records the list it
    /// was shown.
    struct Picks(std::cell::RefCell<Vec<String>>, Vec<usize>);

    impl Asks for Picks {
        fn keep(&self, lines: &[String], _room: usize) -> AppResult<Vec<usize>> {
            *self.0.borrow_mut() = lines.to_vec();
            Ok(self.1.clone())
        }
    }

    /// §7.2: one row per sentence, as `day, note, text`.
    #[test]
    fn explode_opens_a_note_into_its_own_sentences() {
        let notes = answer(&["when", "title"], &[&["2019-11-05", "a"], &["2021-03-14", "b"]]);
        let said = Said(Default::default(), vec!["một".into(), "hai".into()]);
        let got = run_around(
            &parse("nodes | explode sentences"),
            notes,
            &Around { today: day(2026, 9, 20), words: Some(&said), asker: None },
        )
        .expect("runs");

        assert_eq!(got.columns, ["day", "note", "text"]);
        assert_eq!(got.rows.len(), 4);
        assert_eq!(got.rows[0].cells, ["2019-11-05", "Notes/0.md", "một"]);
        // The day reached the reader, which is what lets the consent layer see
        // a hushed stretch of time at all.
        assert_eq!(
            said.0.borrow().as_slice(),
            [
                ("Notes/0.md".to_string(), "2019-11-05".to_string()),
                ("Notes/1.md".to_string(), "2021-03-14".to_string()),
            ]
        );
    }

    /// Three events on one day come from one note. Its sentences are read
    /// once — repetition to a person, and three votes for the same line to a
    /// model.
    #[test]
    fn a_note_several_rows_lead_back_to_is_read_once() {
        let mut events = answer(
            &["when", "title"],
            &[&["2019-11-05", "a"], &["2019-11-05", "b"], &["2021-03-14", "c"]],
        );
        for row in &mut events.rows[..2] {
            row.open = Some("Notes/diary.md".into());
        }
        events.rows[2].open = Some("Notes/other.md".into());

        let said = Said(Default::default(), vec!["một".into()]);
        let got = run_around(
            &parse("nodes | explode sentences"),
            events,
            &Around { today: day(2026, 9, 20), words: Some(&said), asker: None },
        )
        .expect("runs");

        assert_eq!(got.rows.len(), 2, "one line per note, not per row");
        assert_eq!(said.0.borrow().len(), 2, "and the note was read once");
    }

    /// `timeline::year` wrote down why this matters: taking the first of them
    /// would hand the model January and call it a year.
    #[test]
    fn a_list_too_long_to_send_keeps_its_shape_rather_than_its_head() {
        let mut cells: Vec<Vec<String>> = Vec::new();
        for month in 1..=12 {
            for n in 0..300 {
                cells.push(vec![format!("2026-{month:02}-01"), format!("note {month}/{n}")]);
            }
        }
        let borrowed: Vec<Vec<&str>> =
            cells.iter().map(|row| row.iter().map(String::as_str).collect()).collect();
        let rows: Vec<&[&str]> = borrowed.iter().map(Vec::as_slice).collect();

        // One sentence per note, so 3,600 lines — well past the ceiling.
        let said = Said(Default::default(), vec!["một".into()]);
        let got = run_around(
            &parse("nodes | explode sentences"),
            answer(&["when", "title"], &rows),
            &Around { today: day(2026, 9, 20), words: Some(&said), asker: None },
        )
        .expect("runs");

        assert_eq!(got.rows.len(), MOST_LINES);
        assert!(got.note.is_some_and(|n| n.contains("spread")), "it says so");

        // Every month is still represented, in proportion.
        let mut per_month = std::collections::BTreeMap::new();
        for row in &got.rows {
            *per_month.entry(row.cells[0][..7].to_string()).or_insert(0) += 1;
        }
        assert_eq!(per_month.len(), 12, "{per_month:?}");
        assert!(
            per_month.values().all(|n| *n > 100),
            "no month is a token presence: {per_month:?}"
        );
    }

    /// **The guardrail.** Opening a saved lens must not spend money, and what
    /// it would spend has to be visible before anybody agrees to it.
    #[test]
    fn a_question_that_would_spend_refuses_and_says_what_it_would_cost() {
        let rows = answer(&["title"], &[&["a"], &["b"], &["c"], &["d"]]);
        let why = run_on(&parse("nodes | ask 2"), rows, day(2026, 9, 20))
            .expect_err("there is nowhere to spend")
            .to_string();
        assert!(why.contains("would send 4 lines"), "{why}");
        assert!(why.contains("ask 2"), "{why}");
    }

    /// And a list already short enough needs no choosing — asking would cost a
    /// call to be told the same list back. `timeline::year` learnt this first.
    #[test]
    fn asking_is_skipped_when_there_is_nothing_to_choose_between() {
        let rows = answer(&["title"], &[&["a"], &["b"]]);
        let got = run_on(&parse("nodes | ask 5"), rows, day(2026, 9, 20))
            .expect("no call, so no asker needed");
        assert_eq!(got.rows.len(), 2);
    }

    /// The model points; it does not speak. A number nobody offered is dropped
    /// rather than wrapped around, so a model that miscounts cannot reach a
    /// line it was never shown — and there is nowhere in the reply for a
    /// sentence of its own.
    #[test]
    fn a_number_nobody_offered_reaches_nothing() {
        let rows = answer(&["title"], &[&["a"], &["b"], &["c"], &["d"]]);
        let picks = Picks(Default::default(), vec![2, 99, 0]);
        let got = run_around(
            &parse("nodes | ask 3"),
            rows,
            &Around { today: day(2026, 9, 20), words: None, asker: Some(&picks) },
        )
        .expect("runs");

        assert_eq!(picks.0.borrow().as_slice(), ["a", "b", "c", "d"]);
        assert_eq!(
            got.rows.iter().map(|r| r.cells[0].clone()).collect::<Vec<_>>(),
            ["a", "c"],
            "the two that were offered, in the order they were offered"
        );
        assert_eq!(got.total, 2);
    }

    /// The order the question asked for, not the order the model answered in.
    /// For a year of somebody's sentences that is the order it was lived, and
    /// reading them together in any other order means nothing.
    #[test]
    fn what_comes_back_is_in_the_order_it_was_offered() {
        let rows = answer(
            &["when", "text"],
            &[
                &["2026-01-05", "Bắt đầu học đàn."],
                &["2026-03-02", "Chuyển nhà xong."],
                &["2026-11-20", "Nghỉ việc."],
            ],
        );
        // The model names them out of order, and repeats one.
        let picks = Picks(Default::default(), vec![2, 0, 1, 0]);
        let got = run_around(
            &parse("nodes | ask 3"),
            rows,
            &Around { today: day(2026, 9, 20), words: None, asker: Some(&picks) },
        )
        .expect("runs");

        assert_eq!(
            got.rows.iter().map(|r| r.cells[0].clone()).collect::<Vec<_>>(),
            ["2026-01-05", "2026-03-02", "2026-11-20"]
        );
    }

    /// A model repeating itself must not make a sentence appear twice — and
    /// a repeat must not eat one of the places either.
    #[test]
    fn the_same_line_chosen_twice_comes_back_once() {
        // Longer than the room asked for, or there would be nothing to choose
        // and the model would never be called.
        let rows = answer(&["text"], &[&["một"], &["hai"], &["ba"], &["bốn"]]);
        let picks = Picks(Default::default(), vec![1, 1, 1, 3]);
        let got = run_around(
            &parse("nodes | ask 2"),
            rows,
            &Around { today: day(2026, 9, 20), words: None, asker: Some(&picks) },
        )
        .expect("runs");
        assert_eq!(
            got.rows.iter().map(|r| r.cells[0].clone()).collect::<Vec<_>>(),
            ["hai", "bốn"]
        );
    }

    /// `explode` needs the vault. Without it the answer would be every note
    /// with no sentences in it, which is a wrong answer told quietly.
    #[test]
    fn explode_without_a_vault_refuses_rather_than_answering_emptily() {
        let notes = answer(&["when", "title"], &[&["2019-11-05", "a"]]);
        assert!(run_on(&parse("nodes | explode sentences"), notes, day(2026, 9, 20)).is_err());
    }
}



