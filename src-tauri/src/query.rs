//! A question as a tree, before it is flattened into anything.
//!
//! The design is §3 and §15.2 of `docs/query-grammar-2026-09-20.md`. This is
//! step 1 of §15.3, and step 1 is the step where **nothing changes**: the
//! golden file `search_gate.txt` must come out byte for byte the same.
//!
//! # Why a tree at all, when everything downstream wants a flat struct
//!
//! `ParsedQuery` is sixteen flat fields, and a flat struct can only say one
//! thing: *all of these, at once*. There is nowhere in it to put `OR`, nowhere
//! to put a bracket, and — the part that already bites today — nowhere to put
//! "not (this timeline thing)". That is why `-with:khánh` is quietly read as
//! *a note whose `with` frontmatter key is not khánh*: the flat struct has a
//! slot for that reading and no slot for the one the person meant.
//!
//! So the grammar is parsed into a tree, and the flat struct is **derived from
//! the tree** rather than parsed alongside it. One reading of the words, one
//! place to change when the reading changes.
//!
//! # And why the flat struct stays
//!
//! Twelve files build or read `ParsedQuery`, and one of them — `tempo.rs` —
//! constructs it by hand rather than from text. Replacing it everywhere in one
//! commit is the change nobody can review. §15.2 chose the other way: the tree
//! is the truth, `ParsedQuery` becomes a *view* of it (`ParsedQuery::of`), and
//! callers move to the tree one at a time.
//!
//! The view is only honest for a plain conjunction. When the tree holds an
//! `Or`, or a negation of something the flat struct has no slot for, the view
//! **refuses in words** instead of dropping the branch. That refusal is what
//! makes step 3 safe to write: the day `OR` starts parsing, every caller still
//! on the flat view says so out loud rather than answering a smaller question.

use crate::search::{
    is_queryable_key, split_comparison, strip_quotes, unquoted, Comparison, SortOrder,
    MAX_QUERY_LIMIT,
};

/// Which table a question is asked of.
///
/// §4 of the grammar. Today the table is chosen **implicitly**, by whether the
/// question happens to use one of the timeline's words — which works until a
/// question uses words from both, and then one half is silently dropped.
/// Naming it is how a question gets one meaning, and how the bar can say what
/// it is showing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// Everything the vault holds that is a thing — notes, tasks, people,
    /// books, and whatever type somebody invented this morning.
    ///
    /// Called `nodes` and not `notes`, which is what it was called until
    /// somebody read `nodes sort:title` coming back with a person and a book
    /// in it. The word has to survive being read by somebody who did not write
    /// the query, and "notes" promised something the table does not hold to.
    Nodes,
    Events,
}

impl Source {
    /// The word a person writes, and the word shown back to them.
    pub fn word(self) -> &'static str {
        match self {
            Source::Nodes => "nodes",
            Source::Events => "events",
        }
    }

    /// The source a leading word names, if it names one.
    pub fn of(word: &str) -> Option<Source> {
        match word {
            "nodes" => Some(Source::Nodes),
            "events" => Some(Source::Events),
            _ => None,
        }
    }
}

/// What a single condition is about.
///
/// Deliberately not one variant per keyword: `with:`, `where:` and `about:`
/// are three roles of the same question, and `is:`/`type:` are two spellings
/// of one field. The tree records the *field*, not the word that named it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Field {
    /// `is:note`, `type:task`.
    Kind,
    /// `status:done`.
    Status,
    /// `#tag`, `tag:name`.
    Tag,
    /// Any other frontmatter key: `priority:3`, `author:Nguyễn`.
    Prop(String),
    /// A bare word or a quoted phrase — matched against the text.
    Text,
    /// `when:2019` — kept as written, because `timeline::when` reads every
    /// shape a date can take and a second reading here would disagree with it.
    When,
    /// `when:same-day-as(today)` — this day in other years.
    ///
    /// Its own field rather than a shape of `when:`, because it is not a span
    /// and cannot be answered as one: a span is two ends, and this is a day of
    /// the year with the year taken off. §6.2 — it exists to delete a step
    /// from the pipeline, because `anniversary` was never a primitive. It is a
    /// date condition and a derived column, and pretending otherwise is how a
    /// language grows an operation per question.
    SameDay,
    /// `with:khánh` — who was there.
    With,
    /// `where:hanoi` — where it happened.
    Place,
    /// `about:synabit` — what it was about.
    About,
    /// `shape:occasion`.
    Shape,
    /// `magnitude:>4`.
    Size,
}

impl Field {
    /// How the field is written in a question, for a message a person reads.
    pub fn written(&self) -> &'static str {
        match self {
            Field::Kind => "type:",
            Field::Status => "status:",
            Field::Tag => "#tag",
            Field::Prop(_) => "a note's own field",
            Field::Text => "a bare word",
            Field::When => "when:",
            Field::SameDay => "when:same-day-as()",
            Field::With => "with:",
            Field::Place => "place:",
            Field::About => "about:",
            Field::Shape => "shape:",
            Field::Size => "size:",
        }
    }
}

/// What a condition is compared against.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// Equal to this text, as the person wrote it.
    Text(String),
    /// Compared against text: `due_date:<2026-09-01`.
    Compare(Comparison, String),
    /// Compared against a number, already read: `magnitude:>4`.
    ///
    /// Held as a number rather than as text because the parser refuses to make
    /// the term at all when the text is not a number, so nothing downstream
    /// has to cope with a `magnitude` that never was one.
    Number(Comparison, f64),
}

/// One condition.
#[derive(Debug, Clone, PartialEq)]
pub struct Term {
    pub field: Field,
    pub value: Value,
}

impl Term {
    /// A condition on a written value.
    pub fn text(field: Field, value: impl Into<String>) -> Self {
        Term {
            field,
            value: Value::Text(value.into()),
        }
    }
}

/// A question's filter half, as a tree.
///
/// `And(vec![])` is the empty question — nothing was asked. It is a natural
/// zero rather than a special variant: an empty conjunction matches
/// everything, and everything is what an empty search bar shows.
#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Term(Term),
    Not(Box<Expr>),
    And(Vec<Expr>),
    /// Not built by the parser yet — step 3 of §15.3 does that. It is here
    /// now so that `ParsedQuery::of` can already refuse it, which is the whole
    /// safety of step 3: the flat view has no slot for an alternative, and a
    /// flat view that silently kept one branch would answer a smaller question
    /// than the one asked.
    Or(Vec<Expr>),
}

impl Default for Expr {
    /// Asking nothing. An empty conjunction is true of everything, which is
    /// what an empty search bar shows.
    fn default() -> Self {
        Expr::And(Vec::new())
    }
}

/// What a `stats` stage counts up.
///
/// §7.1: the grammar slot is fixed and the **table of names inside it is
/// open**. Adding `sum(x)` later costs an entry here and not one comma of
/// syntax, which is the whole reason the slot was drawn this wide while only
/// one name ships.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tally {
    Count,
}

impl Tally {
    /// What the column of numbers is called in the answer.
    pub fn column(self) -> &'static str {
        match self {
            Tally::Count => "count",
        }
    }
}

/// What rows are gathered under.
///
/// §7.3: `day`, `week`, `month` and `year` are **special**. They gather by the
/// row's *day*, wherever that is, rather than by a field that happens to be
/// called "month" — because no row has a field called month, and gathering by
/// one would answer nothing and explain nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Bucket {
    Day,
    Week,
    Month,
    Year,
    Field(String),
}

impl Bucket {
    /// What the column of labels is called in the answer.
    pub fn column(&self) -> String {
        match self {
            Bucket::Day => "day".into(),
            Bucket::Week => "week".into(),
            Bucket::Month => "month".into(),
            Bucket::Year => "year".into(),
            Bucket::Field(name) => name.clone(),
        }
    }

    fn of(word: &str) -> Bucket {
        match word {
            "day" => Bucket::Day,
            "week" => Bucket::Week,
            "month" => Bucket::Month,
            "year" => Bucket::Year,
            other => Bucket::Field(other.to_string()),
        }
    }
}

/// What a `seq` stage works out about each run of rows.
///
/// §7.1 again: one slot, an open table of names. `streak`, `since-prev` and
/// `running` are named in the design and cost an entry here when they arrive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sequence {
    Gaps,
}

/// What an `explode` stage opens up into more rows.
///
/// §7.1 once more: one slot, an open table. `tags` and `people` are named in
/// the design and cost an entry here when they come.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Opened {
    Sentences,
}

/// One side of a comparison inside `| where`.
#[derive(Debug, Clone, PartialEq)]
pub enum Operand {
    /// The value in this column of the row.
    Column(String),
    /// A plain number.
    Number(f64),
    /// A stretch of time, in days: `6mo`, `30d`, `2y`.
    Days(f64),
    /// Anything else, compared as text.
    Text(String),
}

/// A test one row either passes or does not.
#[derive(Debug, Clone, PartialEq)]
pub enum Test {
    Compare(Operand, Comparison, Operand),
    Equals(Operand, Operand, bool),
    All(Vec<Test>),
    Any(Vec<Test>),
    Nope(Box<Test>),
}

/// One step of the pipeline: the answer so far, turned into another answer.
#[derive(Debug, Clone, PartialEq)]
pub enum Stage {
    /// `| stats count by month`
    Stats { tally: Tally, by: Bucket },
    /// `| sort count desc`
    Sort { key: String, descending: bool },
    /// `| seq gaps by who`
    Seq { sequence: Sequence, by: Bucket },
    /// `| where quiet > longest`
    Where(Test),
    /// `| explode sentences`
    Explode(Opened),
    /// `| ask 15` — the one stage that spends money.
    Ask(u32),
    /// `| head 5`
    Head(u32),
}

/// A whole question: what to match, and how to lay out what matched.
///
/// The shaping words are not part of the filter tree on purpose. `sort:`,
/// `columns:` and `limit:` say nothing about *which* rows match — they are
/// about the table, not the question — so putting them in the tree would mean
/// every walker had to skip over them.
///
/// The pipeline is `stages`, read from what follows each `|`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Query {
    /// Which table, when the question said. `None` means it did not, and the
    /// words it used decide — see `ParsedQuery::source_of`.
    pub source: Option<Source>,
    /// What has to be true of a row. `Expr::And(vec![])` asks nothing, which
    /// is what an empty search bar asks.
    pub filter: Expr,
    pub title_only: bool,
    pub sort: Option<SortOrder>,
    pub columns: Vec<String>,
    pub limit: Option<u32>,
    /// Why this question cannot be answered, if it cannot.
    ///
    /// Parsing stays infallible — a dozen callers rely on it — so a word the
    /// grammar cannot make sense of lands here rather than becoming a filter
    /// on a property of that name. That was the whole of §9 and step 0.
    pub refused: Vec<String>,
    /// How many matching rows to skip, for reaching past the cap.
    ///
    /// Not query syntax and deliberately not: it is a property of the page
    /// being looked at, not of the question being asked, and a saved view
    /// carrying `offset:500` in its text would reopen on page two for ever.
    /// The caller sets it after parsing.
    pub offset: u32,
    /// What to do with the rows once they are found (§7).
    pub stages: Vec<Stage>,
}

/// Words that used to mean something and now mean it under another name.
///
/// §11. They are listed rather than deleted because deleting a keyword does
/// not make it stop parsing — it makes it parse as *a frontmatter key of that
/// name*, which answers 0 and explains nothing.
const RENAMED: &[(&str, &str)] = &[("where:", "place:"), ("magnitude:", "size:")];

/// And the one source that was renamed.
///
/// Apart from [`RENAMED`] because it is not a `key:` — it is the first word of
/// a question, and only there. On its own it is still the ordinary English
/// word somebody may be searching for.
const RENAMED_SOURCE: (&str, &str) = ("notes", "nodes");

impl Query {
    /// Whether there is nothing here to match on.
    ///
    /// Naming a table is asking something, and so is every condition — except
    /// one. A bare word exclusion is not a question: "not draft" asks for the
    /// whole vault minus a little, and nobody means that by typing `-draft`
    /// into a search bar. So it does not make an empty bar non-empty, though
    /// it narrows a question that has something else in it.
    /// Which table answers this question.
    ///
    /// A question that names its source gets that one. A question that does
    /// not is read the way it always was: the timeline's words are the
    /// selector, and they are a fair one — asking who was somewhere is only
    /// answerable of an *event*, so writing `with:` is already the act of
    /// saying which table to read.
    ///
    /// §4 proposed defaulting to `notes` instead. That was written before this
    /// code was read: it would turn every `with:khánh` anybody has ever typed
    /// into a refusal, and `when:` is the word the timeline is *made of*.
    /// Naming the source is worth having because it lets a question mean one
    /// thing when it uses words from both halves — not because guessing was
    /// wrong when it had only one half to guess from.
    pub fn source_of(&self) -> Source {
        fn timeline(expr: &Expr) -> bool {
            match expr {
                Expr::Term(term) => matches!(
                    term.field,
                    Field::When
                        | Field::SameDay
                        | Field::With
                        | Field::Place
                        | Field::About
                        | Field::Shape
                        | Field::Size
                ),
                Expr::Not(inner) => timeline(inner),
                Expr::And(branches) | Expr::Or(branches) => branches.iter().any(timeline),
            }
        }
        match self.source {
            Some(source) => source,
            None if timeline(&self.filter) => Source::Events,
            None => Source::Nodes,
        }
    }

    /// The same question, asking for **any** of its words rather than all.
    ///
    /// What arrives at the assistant's `query_nodes` is not a search phrase —
    /// it is the words of a *question*, and requiring all of them requires the
    /// asker to have guessed the note's own vocabulary. Asked what was decided
    /// about pricing and who disagreed, it queried `decide pricing disagreed`,
    /// got nothing, and reported that the vault held no notes about pricing.
    /// It holds two, and both are entirely about it.
    ///
    /// This used to be a flag on the flat struct that only the FTS path read.
    /// With a tree it is the thing it always meant: the words become one `OR`
    /// group, and everything else stays required.
    pub fn any_word(&self) -> Query {
        let Expr::And(branches) = &self.filter else {
            return self.clone();
        };
        let is_word = |e: &Expr| matches!(e, Expr::Term(Term { field: Field::Text, .. }));
        let words: Vec<Expr> = branches.iter().filter(|e| is_word(e)).cloned().collect();
        if words.len() < 2 {
            return self.clone();
        }
        let mut kept: Vec<Expr> = branches.iter().filter(|e| !is_word(e)).cloned().collect();
        kept.push(Expr::Or(words));
        Query {
            filter: Expr::And(kept),
            ..self.clone()
        }
    }

    /// How many bare words the question carries, at the top level.
    pub fn word_count(&self) -> usize {
        match &self.filter {
            Expr::And(branches) => branches
                .iter()
                .filter(|e| matches!(e, Expr::Term(Term { field: Field::Text, .. })))
                .count(),
            Expr::Term(Term { field: Field::Text, .. }) => 1,
            _ => 0,
        }
    }

    pub fn asks_nothing(&self) -> bool {
        fn says_something(expr: &Expr) -> bool {
            match expr {
                Expr::Term(_) => true,
                Expr::Not(inner) => !matches!(
                    &**inner,
                    Expr::Term(Term {
                        field: Field::Text,
                        ..
                    })
                ),
                Expr::And(branches) | Expr::Or(branches) => {
                    branches.iter().any(says_something)
                }
            }
        }
        self.source.is_none()
            && self.sort.is_none()
            && self.columns.is_empty()
            && self.limit.is_none()
            && !says_something(&self.filter)
    }
}

/// A quoted token with its quotes straightened.
///
/// A phone turns `"` into `“` without being asked, and FTS5 knows one kind of
/// quote. Only when the token both opens and closes with one, so a typo like
/// `foo"bar` stays the word somebody typed.
fn as_phrase(token: &str) -> String {
    let quote = |c: char| c == '"' || c == '\u{201c}' || c == '\u{201d}';
    let mut chars = token.chars();
    match (chars.next(), chars.next_back()) {
        (Some(open), Some(close)) if quote(open) && quote(close) => {
            format!("\"{}\"", chars.as_str())
        }
        _ => token.to_string(),
    }
}

/// Whether what has been read so far is a name that could take a bracket.
///
/// `same-day-as` is, and so is the `same-day-as` in `when:same-day-as`, which
/// is why only the part after the last colon is looked at — the keyword in
/// front of a value is not part of the value's name. `-` is not a name, so
/// `-(a OR b)` is a minus and then a group rather than a call to something
/// called "-".
fn names_a_call(word: &str) -> bool {
    let name = word.rsplit(':').next().unwrap_or("");
    !name.is_empty()
        && name.chars().any(char::is_alphabetic)
        && name
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
}

/// Split a raw query into tokens, keeping a quoted phrase whole.
fn tokenize(trimmed: &str) -> Vec<String> {
    let mut chars = trimmed.chars().peekable();
    let mut tokens: Vec<String> = Vec::new();

    while let Some(&ch) = chars.peek() {
        if ch.is_whitespace() {
            chars.next();
            continue;
        }

        if ch == '"' {
            chars.next(); // the opening quote
            let mut phrase = String::new();
            while let Some(&c) = chars.peek() {
                if c == '"' {
                    chars.next(); // the closing quote
                    break;
                }
                phrase.push(c);
                chars.next();
            }
            if !phrase.trim().is_empty() {
                // FTS5 spells a phrase this way, and the token is handed to it
                // unchanged further down.
                tokens.push(format!("\"{}\"", phrase.trim()));
            }
        } else {
            // A word may contain a quote — `tag:"one mount"` is one token, and
            // so is a typo like `foo"bar`.
            let mut word = String::new();
            let mut in_quote = false;
            // Brackets opened inside this word, so that `same-day-as(today)`
            // stays one token while `(a OR b)` becomes five.
            let mut depth = 0usize;
            while let Some(&c) = chars.peek() {
                if c == '"' || c == '\u{201c}' || c == '\u{201d}' {
                    in_quote = !in_quote;
                    word.push(c);
                    chars.next();
                } else if in_quote {
                    word.push(c);
                    chars.next();
                } else if c.is_whitespace() {
                    break;
                } else if c == '|' {
                    // The pipe is always its own token: it is the one mark
                    // that says the answer so far is about to be turned into
                    // a different answer.
                    break;
                } else if c == '(' {
                    // A bracket right after a name is the name's own bracket —
                    // `same-day-as(today)` is one thing, not a group. Anywhere
                    // else it opens a group, so the word so far ends here.
                    if names_a_call(&word) {
                        depth += 1;
                        word.push(c);
                        chars.next();
                    } else {
                        break;
                    }
                } else if c == ')' {
                    // Only the bracket this word opened belongs to it.
                    if depth == 0 {
                        break;
                    }
                    depth -= 1;
                    word.push(c);
                    chars.next();
                } else {
                    word.push(c);
                    chars.next();
                }
            }
            // An empty word means the character in front is a bracket of its
            // own, which is a token.
            if word.is_empty() {
                word.push(chars.next().expect("a character that is not whitespace"));
            }
            tokens.push(word);
        }
    }

    tokens
}

/// Read a question into a tree.
///
/// Infallible by design: a question the grammar cannot read comes back with a
/// reason in `refused`, not as an `Err` a caller can forget to look at.
pub fn parse(raw: &str) -> Query {
    let mut q = Query::default();

    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return q;
    }

    let mut tokens = tokenize(trimmed);

    // A source names what a question is **about**, so it is the first word or
    // it is not the source at all: `#work events` is about nodes tagged work
    // and the word "events", and reading it the other way would take a word
    // out of somebody's search.
    //
    // And on its own it is not a question — it is the word. `nodes` and
    // `events` are ordinary English, and the free-text boxes (`search_notes`,
    // `search_tasks`, …) hand whatever was typed straight to this parser.
    // Swallowing a lone word there would quietly turn a search into a listing
    // of everything, with nothing on the screen to say so. "The whole
    // timeline" is still sayable — `events sort:-when` or `events limit:200`
    // — and those say what they want anyway.
    let leading = tokens.first().map(|first| first.to_lowercase()).unwrap_or_default();
    if tokens.len() > 1 {
        if Source::of(&leading).is_some() {
            q.source = Source::of(&leading);
            tokens.remove(0);
        } else if leading == RENAMED_SOURCE.0 {
            // Where it used to name a table. Refused rather than left to
            // become a bare word, which would turn `notes when:2019` from a
            // question about notes into a search for the word "notes" — and
            // then, having no node words left, into a question about events.
            // Exactly the silent change of meaning §9 exists to stop.
            q.refused.push(format!(
                "'{}' is now '{}'",
                RENAMED_SOURCE.0, RENAMED_SOURCE.1
            ));
            tokens.remove(0);
        }
    }

    // ── the pipeline, cut off before the expression is read ──
    //
    // Split rather than woven in: everything before the first `|` is the
    // question, everything after it is what to do with the answer, and keeping
    // them apart means the expression reader never has to know a pipe exists.
    let mut runs = tokens.split(|token| token == "|");
    let filter_tokens: Vec<String> = runs.next().unwrap_or(&[]).to_vec();
    let stage_runs: Vec<Vec<String>> = runs.map(<[String]>::to_vec).collect();
    let tokens = filter_tokens;

    // ── the expression ──
    let mut reader = Reader { tokens: &tokens, at: 0, q: &mut q };
    let filter = reader.expr();
    if let Some(extra) = reader.peek().map(str::to_string) {
        // Something is left that nothing could attach to — a stray `)`, or a
        // word after one. Refused rather than dropped: a bracket in the wrong
        // place changes what the rest of the question means.
        q.refused.push(format!("'{extra}' has nothing to join onto"));
    }
    q.filter = filter.unwrap_or_default();

    for run in stage_runs {
        read_stage(&run, &mut q);
    }

    q
}

/// One `| …` run, read onto the query.
fn read_stage(run: &[String], q: &mut Query) {
    let words: Vec<String> = run.iter().map(|w| w.to_lowercase()).collect();
    let Some(name) = words.first().map(String::as_str) else {
        q.refused.push("'|' needs something after it".into());
        return;
    };
    let rest = &words[1..];

    match name {
        "stats" => {
            // `stats <fn> by <key>` — the slot is fixed, the table of names
            // inside it is open (§7.1).
            let tally = match rest.first().map(String::as_str) {
                Some("count") => Tally::Count,
                Some(other) => {
                    q.refused
                        .push(format!("'{other}' is not something stats can work out yet"));
                    return;
                }
                None => {
                    q.refused.push("stats needs to be told what to work out".into());
                    return;
                }
            };
            match (rest.get(1).map(String::as_str), rest.get(2)) {
                (Some("by"), Some(key)) => q.stages.push(Stage::Stats {
                    tally,
                    by: Bucket::of(key),
                }),
                _ => q
                    .refused
                    .push("stats needs `by` and something to gather under".into()),
            }
        }
        "seq" => {
            let sequence = match rest.first().map(String::as_str) {
                Some("gaps") => Sequence::Gaps,
                Some(other) => {
                    q.refused
                        .push(format!("'{other}' is not something seq can work out yet"));
                    return;
                }
                None => {
                    q.refused.push("seq needs to be told what to work out".into());
                    return;
                }
            };
            match (rest.get(1).map(String::as_str), rest.get(2)) {
                (Some("by"), Some(key)) => q.stages.push(Stage::Seq {
                    sequence,
                    by: Bucket::of(key),
                }),
                _ => q
                    .refused
                    .push("seq needs `by` and something to follow through time".into()),
            }
        }
        "explode" => match rest.first().map(String::as_str) {
            Some("sentences") => q.stages.push(Stage::Explode(Opened::Sentences)),
            Some(other) => q
                .refused
                .push(format!("'{other}' is not something explode can open up yet")),
            None => q
                .refused
                .push("explode needs to be told what to open up".into()),
        },
        "ask" => match rest.first().and_then(|n| n.parse::<u32>().ok()) {
            Some(n) => q.stages.push(Stage::Ask(n)),
            None => q.refused.push("ask needs a number of lines to keep".into()),
        },
        "where" => match read_test(rest) {
            Ok(test) => q.stages.push(Stage::Where(test)),
            Err(why) => q.refused.push(why),
        },
        "sort" => match rest.first() {
            Some(key) => q.stages.push(Stage::Sort {
                key: key.clone(),
                descending: rest.get(1).is_some_and(|d| d == "desc"),
            }),
            None => q.refused.push("sort needs something to sort by".into()),
        },
        "head" => match rest.first().and_then(|n| n.parse::<u32>().ok()) {
            Some(n) => q.stages.push(Stage::Head(n)),
            None => q.refused.push("head needs a number of rows".into()),
        },
        // Plain sugar, and said to be: `top 5 by count` is the question people
        // actually ask, and writing it out as two stages every time is noise.
        // Named here rather than pretending to be a step of its own, which is
        // the mistake `anniversary` made in the old design.
        "top" => match (rest.first().and_then(|n| n.parse::<u32>().ok()), rest.get(1).map(String::as_str), rest.get(2)) {
            (Some(n), Some("by"), Some(key)) => {
                q.stages.push(Stage::Sort {
                    key: key.clone(),
                    descending: true,
                });
                q.stages.push(Stage::Head(n));
            }
            _ => q
                .refused
                .push("top needs a number and `by` something — `top 5 by count`".into()),
        },
        other => q.refused.push(format!("'{other}' is not something a question can do")),
    }
}

/// A token stream being read as a tree.
///
/// Recursive descent over §3's grammar, which is Lucene's precedence and
/// everyone else's: `NOT` binds tighter than `AND`, which binds tighter than
/// `OR`, and brackets beat all three.
struct Reader<'a> {
    tokens: &'a [String],
    at: usize,
    q: &'a mut Query,
}

/// The words that are structure rather than something to search for.
///
/// Upper case only, as in Lucene — and here it earns its keep twice over,
/// because `or` and `and` are ordinary English and `không` is not the point:
/// a vault holds sentences, and a search for the word "or" must stay a search
/// for the word "or".
const OR: &str = "OR";
const AND: &str = "AND";
const NOT: &str = "NOT";

impl Reader<'_> {
    fn peek(&self) -> Option<&str> {
        self.tokens.get(self.at).map(String::as_str)
    }

    fn take(&mut self) -> Option<&str> {
        let token = self.tokens.get(self.at).map(String::as_str);
        if token.is_some() {
            self.at += 1;
        }
        token
    }

    /// `or := and { "OR" and }`
    fn expr(&mut self) -> Option<Expr> {
        let mut branches = Vec::new();
        if let Some(first) = self.all() {
            branches.push(first);
        }
        while self.peek() == Some(OR) {
            self.at += 1;
            match self.all() {
                Some(next) => branches.push(next),
                // `a OR` on its own is somebody mid-sentence, and answering it
                // as though the OR were not there would quietly narrow the
                // question. The bar re-parses on every keystroke, so this is
                // the state between two words — it has to say so, not guess.
                None => self.q.refused.push("'OR' needs something on both sides".into()),
            }
        }
        match branches.len() {
            0 => None,
            1 => branches.pop(),
            _ => Some(Expr::Or(branches)),
        }
    }

    /// `and := unary { [ "AND" ] unary }` — juxtaposition is AND.
    fn all(&mut self) -> Option<Expr> {
        let mut branches = Vec::new();
        loop {
            match self.peek() {
                None | Some(OR) | Some(")") => break,
                Some(AND) => {
                    self.at += 1;
                    continue;
                }
                _ => {}
            }
            let before = self.at;
            if let Some(branch) = self.unary() {
                branches.push(branch);
            }
            // A token that was shaping (`sort:`) or a refusal yields nothing,
            // and that is fine — but it must still have been consumed, or this
            // loops for ever.
            if self.at == before {
                self.at += 1;
            }
        }
        match branches.len() {
            0 => None,
            1 => branches.pop(),
            _ => Some(Expr::And(branches)),
        }
    }

    /// `unary := [ "-" | "NOT" ] atom`
    ///
    /// A minus glued to the front of a word is the same thing as the word
    /// `NOT` in front of it, so both strip and then read what is left **as an
    /// ordinary term**. That is the whole fix for `-with:khánh`: the old
    /// reader knew its own small list of what could be negated — a tag, a
    /// queryable key, a word — and `with:` was not on it, so the question
    /// quietly became *a note whose `with` frontmatter field is not khánh*.
    /// There is no list now. Whatever can be asked can be un-asked.
    fn unary(&mut self) -> Option<Expr> {
        if matches!(self.peek(), Some(NOT) | Some("-")) {
            self.at += 1;
            let inner = self.atom()?;
            return Some(Expr::Not(Box::new(inner)));
        }
        // `-x`, with nothing between the minus and the x. `--` is left alone:
        // it is how a command line writes a flag, not how a person writes
        // "without".
        if let Some(token) = self.peek() {
            if token.starts_with('-') && token.chars().count() > 1 && !token.starts_with("--") {
                let rest = self.take().expect("a token that was just peeked at")[1..].to_string();
                return self.read_term(rest).map(|inner| Expr::Not(Box::new(inner)));
            }
        }
        self.atom()
    }

    /// `atom := "(" expr ")" | term`
    fn atom(&mut self) -> Option<Expr> {
        if self.peek() == Some("(") {
            self.at += 1;
            let inner = self.expr();
            if self.peek() == Some(")") {
                self.at += 1;
            } else {
                self.q.refused.push("a bracket was opened and not closed".into());
            }
            return inner;
        }
        let token = self.take()?.to_string();
        self.read_term(token)
    }

    /// One token as a condition, or nothing when it was not one.
    ///
    /// `sort:`, `columns:`, `limit:` and `in:title` land on the query rather
    /// than in the tree — they say nothing about *which* rows match — and a
    /// word the grammar cannot read lands in `refused`. Both come back as
    /// `None`, which the caller treats as "that token was not a condition".
    fn read_term(&mut self, token: String) -> Option<Expr> {
        let q = &mut *self.q;
        let lower = token.to_lowercase();

        // ── The timeline's own words ───────────────────────────────
        //
        // Read before `is:` and the property filters so they are never taken
        // for a property named `with` on a note. Each keeps the person's text
        // as written: a name becomes a node id only where the vault can be
        // read, and a date only where `timeline::when` can read it.
        if let Some(stripped) = lower.strip_prefix("when:") {
            let value = unquoted(stripped);
            // `same-day-as(x)` is the one call the grammar ships (§6.2). The
            // tokenizer already keeps a name and its bracket together, so this
            // arrives whole.
            if let Some(inner) = value
                .strip_prefix("same-day-as(")
                .and_then(|rest| rest.strip_suffix(')'))
            {
                let inner = inner.trim();
                if inner.is_empty() {
                    q.refused
                        .push("same-day-as() needs a day — `same-day-as(today)`".into());
                    return None;
                }
                return Some(Expr::Term(Term::text(Field::SameDay, inner)));
            }
            return (!value.is_empty()).then(|| Expr::Term(Term::text(Field::When, value)));
        }
        // `with:`, `where:` and `about:` are three of §4.2's four roles. The
        // fourth, `evidence`, is not a question anybody asks: nobody looks for
        // "events a photograph belongs to" — they look at the photograph.
        //
        // The value keeps its original casing, because a name is a name.
        if let Some(word) = ["with:", "place:", "about:"]
            .into_iter()
            .find(|word| lower.starts_with(word))
        {
            let value = unquoted(&token[word.len()..]);
            let field = match word {
                "with:" => Field::With,
                "place:" => Field::Place,
                _ => Field::About,
            };
            return (!value.is_empty()).then(|| Expr::Term(Term::text(field, value)));
        }
        // The two words §11 renamed. Refused rather than quietly left to the
        // property catch-all below, where `where:hanoi` would become a filter
        // on a frontmatter key named `where` and answer 0 without a word about
        // why — which is the whole failure this document exists to stop.
        if let Some(renamed) = RENAMED
            .iter()
            .find(|(old, _)| lower.starts_with(old))
        {
            q.refused.push(format!(
                "'{}' is now '{}'",
                renamed.0.trim_end_matches(':'),
                renamed.1.trim_end_matches(':')
            ));
            return None;
        }
        if let Some(stripped) = lower.strip_prefix("shape:") {
            let value = unquoted(stripped);
            return (!value.is_empty()).then(|| Expr::Term(Term::text(Field::Shape, value)));
        }
        if let Some(stripped) = lower.strip_prefix("size:") {
            let value = unquoted(stripped);
            // `size:4` used to mean **at least** 4 while `priority:3` meant
            // exactly 3 — one shape, two meanings (§6.1). The exception is
            // gone, and it is not replaced by equality: size is a computed
            // real number, so `= 4` would be a filter that almost never
            // matches and never says why. So a bare number is refused and the
            // comparison has to be written.
            //
            // The word scale of §6.3 (`size:big`) is the reading for a person
            // rather than a machine, and it waits on thresholds measured
            // against a vault — §16 Bước 8 showed the distribution is
            // currently degenerate, so fixing numbers to words now would be
            // fixing them to a broken one.
            return match split_comparison(value) {
                Some((comparison, rest)) => match rest.trim().parse::<f64>() {
                    Ok(number) => Some(Expr::Term(Term {
                        field: Field::Size,
                        value: Value::Number(comparison, number),
                    })),
                    Err(_) => {
                        q.refused
                            .push(format!("'{rest}' is not a size to compare against"));
                        None
                    }
                },
                None if value.is_empty() => None,
                None => {
                    q.refused.push(format!(
                        "'size:{value}' has to say which way — write size:>={value} or size:<={value}"
                    ));
                    None
                }
            };
        }

        // `type:` and `is:` are the same filter. `is:` came first and is what
        // the Tasks search bar sends; `type:` is what the frontmatter field is
        // called, so it is what anyone writing a query — or an assistant
        // reading `list_schemas` — reaches for first.
        //
        // Any type, not a list of five: `node_type` is a free string in the
        // schema, and a list in the code deciding which of a person's own types
        // are real is the same mistake `NodeType::Other` exists to prevent.
        if let Some(stripped) = lower
            .strip_prefix("is:")
            .or_else(|| lower.strip_prefix("type:"))
        {
            return (!stripped.is_empty()).then(|| Expr::Term(Term::text(Field::Kind, stripped)));
        }
        if let Some(stripped) = lower.strip_prefix("status:") {
            // Same widening, and here it was not merely narrow but wrong: the
            // old list read `in-progress` while every task in every vault is
            // written `in_progress`, and `backlog` and `canceled` — both real
            // statuses the Tasks app writes — were not on it at all.
            return (!stripped.is_empty()).then(|| Expr::Term(Term::text(Field::Status, stripped)));
        }

        // `date:` used to be read here into a field no runner ever looked at,
        // so `date:today` quietly matched everything. It now reaches the
        // ordinary property filter below, where `date:2019-11-05` does what it
        // says on a daily note. A date that is not one day belongs to `when:`.

        if lower == "in:title" {
            q.title_only = true;
            return None;
        }

        if token.starts_with('#') && token.len() > 1 {
            return Some(Expr::Term(Term::text(Field::Tag, &token[1..])));
        }
        if lower.starts_with("tag:") && lower.len() > 4 {
            return Some(Expr::Term(Term::text(Field::Tag, strip_quotes(&lower[4..]))));
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
                q.sort = Some(SortOrder {
                    key: key.to_string(),
                    descending,
                });
            } else {
                q.refused
                    .push(format!("'{key}' is not something a query can sort by"));
            }
            return None;
        }

        if let Some(rest) = lower.strip_prefix("columns:") {
            q.columns = rest
                .split(',')
                .map(str::trim)
                .filter(|c| is_queryable_key(c))
                .map(str::to_string)
                .collect();
            if q.columns.is_empty() {
                q.refused
                    .push(format!("'{rest}' is not a column a query can show"));
            }
            return None;
        }

        if let Some(rest) = lower.strip_prefix("limit:") {
            match rest.trim().parse::<u32>() {
                Ok(n) => q.limit = Some(n.clamp(1, MAX_QUERY_LIMIT)),
                Err(_) => q
                    .refused
                    .push(format!("'{rest}' is not a number of rows")),
            }
            return None;
        }

        // Any other `key:value` is a filter on a frontmatter key of that name.
        if let Some(colon) = lower.find(':') {
            let key = &lower[..colon];
            let value = strip_quotes(&lower[colon + 1..]);
            if !key.is_empty() && !value.is_empty() {
                let value = match split_comparison(value) {
                    Some((op, rest)) => Value::Compare(op, rest.to_string()),
                    None => Value::Text(value.to_string()),
                };
                return Some(Expr::Term(Term {
                    field: Field::Prop(key.to_string()),
                    value,
                }));
            }
        }


            Some(Expr::Term(Term::text(Field::Text, as_phrase(&token))))
        }
    }

/// Read `| where …` into a test.
///
/// A second little reader rather than a reuse of the filter one, because they
/// are about different things and saying so is cheaper than a type that means
/// both. The filter asks about a **node or an event**; this asks about a
/// **row of the answer** — its columns, by name. `quiet > longest` compares
/// two columns of the same row, which is not a question the filter half can
/// even phrase.
///
/// Same precedence as everywhere else: `NOT` over `AND` over `OR`.
fn read_test(words: &[String]) -> Result<Test, String> {
    let mut at = 0usize;
    let test = read_any(words, &mut at)?;
    match words.get(at) {
        None => Ok(test),
        Some(extra) => Err(format!("'{extra}' has nothing to join onto in where")),
    }
}

fn read_any(words: &[String], at: &mut usize) -> Result<Test, String> {
    let mut branches = vec![read_all(words, at)?];
    while words.get(*at).map(String::as_str) == Some("or") {
        *at += 1;
        branches.push(read_all(words, at)?);
    }
    Ok(if branches.len() == 1 {
        branches.pop().expect("one branch")
    } else {
        Test::Any(branches)
    })
}

fn read_all(words: &[String], at: &mut usize) -> Result<Test, String> {
    let mut branches = vec![read_not(words, at)?];
    loop {
        match words.get(*at).map(String::as_str) {
            Some("and") => *at += 1,
            Some(word) if word != "or" && word != ")" => {}
            _ => break,
        }
        branches.push(read_not(words, at)?);
    }
    Ok(if branches.len() == 1 {
        branches.pop().expect("one branch")
    } else {
        Test::All(branches)
    })
}

fn read_not(words: &[String], at: &mut usize) -> Result<Test, String> {
    if words.get(*at).map(String::as_str) == Some("not") {
        *at += 1;
        return Ok(Test::Nope(Box::new(read_not(words, at)?)));
    }
    if words.get(*at).map(String::as_str) == Some("(") {
        *at += 1;
        let inner = read_any(words, at)?;
        if words.get(*at).map(String::as_str) != Some(")") {
            return Err("a bracket was opened and not closed in where".into());
        }
        *at += 1;
        return Ok(inner);
    }
    read_comparison(words, at)
}

fn read_comparison(words: &[String], at: &mut usize) -> Result<Test, String> {
    let left = words
        .get(*at)
        .ok_or_else(|| "where needs something to compare".to_string())?;
    let operator = words
        .get(*at + 1)
        .ok_or_else(|| format!("'{left}' is not a comparison — write `{left} > 5`"))?;
    let right = words
        .get(*at + 2)
        .ok_or_else(|| format!("'{left} {operator}' has nothing on the right of it"))?;
    let (left, right) = (Operand::of(left), Operand::of(right));
    *at += 3;
    Ok(match operator.as_str() {
        ">" => Test::Compare(left, Comparison::GreaterThan, right),
        ">=" => Test::Compare(left, Comparison::GreaterOrEqual, right),
        "<" => Test::Compare(left, Comparison::LessThan, right),
        "<=" => Test::Compare(left, Comparison::LessOrEqual, right),
        "=" | "==" | "is" => Test::Equals(left, right, true),
        "!=" | "<>" => Test::Equals(left, right, false),
        other => return Err(format!("'{other}' is not a way of comparing two things")),
    })
}

impl Operand {
    /// What one word on either side of a comparison is.
    ///
    /// A stretch of time is read here rather than left as text, because
    /// `quiet > 6mo` is the question the silence panel asks and `quiet` is
    /// counted in days. Months are 30 days and years 365: this is comparing
    /// stretches, not naming dates, and a calendar month would make the
    /// comparison depend on which month nobody is talking about.
    fn of(word: &str) -> Operand {
        if let Ok(number) = word.parse::<f64>() {
            return Operand::Number(number);
        }
        for (suffix, days) in [("mo", 30.0), ("y", 365.0), ("w", 7.0), ("d", 1.0)] {
            if let Some(count) = word.strip_suffix(suffix) {
                if let Ok(count) = count.parse::<f64>() {
                    return Operand::Days(count * days);
                }
            }
        }
        // A bare word is a column name; a quoted one is text somebody meant
        // literally, which is how a column called `done` is told from the word.
        match strip_quotes(word) {
            quoted if quoted != word => Operand::Text(quoted.to_string()),
            plain => Operand::Column(plain.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::search::{parse_query, Comparison, ParsedQuery};

    fn term(field: Field, value: &str) -> Expr {
        Expr::Term(Term::text(field, value))
    }

    #[test]
    fn a_plain_question_is_a_conjunction_of_conditions() {
        let q = parse("is:note #gia-đình báo");
        assert_eq!(
            q.filter,
            Expr::And(vec![
                term(Field::Kind, "note"),
                term(Field::Tag, "gia-đình"),
                term(Field::Text, "báo"),
            ])
        );
    }

    #[test]
    fn a_minus_is_a_negation_and_not_a_field_of_its_own() {
        // The flat struct had three separate exclusion lists because it had no
        // way to say "not". The tree says it once.
        assert_eq!(
            parse("-status:done -#gia-đình -báo").filter,
            Expr::And(vec![
                Expr::Not(Box::new(term(Field::Status, "done"))),
                Expr::Not(Box::new(term(Field::Tag, "gia-đình"))),
                Expr::Not(Box::new(term(Field::Text, "báo"))),
            ])
        );
    }

    /// `NOT` binds tighter than `AND`, which binds tighter than `OR` — §3,
    /// and everybody else's. Written out as a tree because that is the only
    /// place the precedence is visible.
    #[test]
    fn either_binds_loosest_and_not_binds_tightest() {
        assert_eq!(
            parse("#a #b OR #c").filter,
            Expr::Or(vec![
                Expr::And(vec![term(Field::Tag, "a"), term(Field::Tag, "b")]),
                term(Field::Tag, "c"),
            ])
        );
        assert_eq!(
            parse("(#a OR #b) #c").filter,
            Expr::And(vec![
                Expr::Or(vec![term(Field::Tag, "a"), term(Field::Tag, "b")]),
                term(Field::Tag, "c"),
            ]),
            "brackets beat both"
        );
        assert_eq!(parse("#a AND #b").filter, parse("#a #b").filter, "AND may be said");
    }

    /// Only shouted. A vault holds sentences, and somebody searching for the
    /// English word "or" must find it.
    #[test]
    fn the_operators_are_upper_case_or_they_are_words() {
        assert_eq!(
            parse("#a or #b").filter,
            Expr::And(vec![
                term(Field::Tag, "a"),
                term(Field::Text, "or"),
                term(Field::Tag, "b"),
            ])
        );
    }

    /// The debt step 0 could not pay. `-with:khánh` used to become *a note
    /// whose `with` frontmatter field is not khánh*, because the reader knew a
    /// small list of what could be negated and `with:` was not on it.
    #[test]
    fn anything_that_can_be_asked_can_be_un_asked() {
        for (written, field) in [
            ("-with:khánh", Field::With),
            ("-when:2019", Field::When),
            ("-shape:chore", Field::Shape),
            ("-#gia-đình", Field::Tag),
            ("-status:done", Field::Status),
        ] {
            let Expr::Not(inner) = parse(written).filter else {
                panic!("'{written}' did not read as a negation");
            };
            let Expr::Term(term) = *inner else {
                panic!("'{written}' negated something other than a condition");
            };
            assert_eq!(term.field, field, "{written}");
        }
        // And a question that negates one of the timeline's words is still a
        // question about the timeline.
        assert_eq!(parse("-with:khánh").source_of(), Source::Events);
    }

    /// A bracket in the wrong place changes what the rest of the question
    /// means, so it is said out loud rather than dropped.
    #[test]
    fn a_bracket_that_does_not_close_is_refused() {
        for broken in ["(#a", "#a)", "#a OR", "((#a)"] {
            assert!(!parse(broken).refused.is_empty(), "'{broken}' was let through");
        }
        assert!(parse("(#a)").refused.is_empty(), "and a closed one is fine");
    }

    /// A name with a bracket after it is one word, not a group — which is
    /// what lets `same-day-as(today)` be written at all.
    #[test]
    fn a_bracket_that_belongs_to_a_name_stays_with_it() {
        assert_eq!(tokenize("when:same-day-as(today)"), ["when:same-day-as(today)"]);
        assert_eq!(
            parse("when:same-day-as(today)").filter,
            term(Field::SameDay, "today")
        );
    }

    /// The word that names the table was renamed. Where it used to name one,
    /// it says so — silence there would turn `notes when:2019` from a question
    /// about nodes into a search for the word "notes", and then, with no node
    /// words left in it, into a question about events.
    #[test]
    fn the_old_name_for_the_table_says_what_it_is_now_called() {
        let renamed = parse("notes when:2019");
        assert_eq!(renamed.refused.len(), 1, "{renamed:?}");
        assert!(renamed.refused[0].contains("nodes"), "{renamed:?}");

        assert_eq!(parse("nodes when:2019").source_of(), Source::Nodes);
        assert_eq!(parse("nodes when:2019").refused.len(), 0);

        // On its own it is still the ordinary English word, the same as
        // `nodes` and `events` are.
        assert_eq!(parse("notes").filter, term(Field::Text, "notes"));
        assert!(parse("notes").refused.is_empty());
    }

    /// §6.2: this day in other years is **not** a span, so it is not a shape
    /// of `when:` — a span is two ends, and this is a day of the year with the
    /// year taken off.
    #[test]
    fn this_day_in_other_years_is_a_field_of_its_own() {
        assert_eq!(parse("when:same-day-as(2019-11-05)").filter, term(Field::SameDay, "2019-11-05"));
        assert_eq!(parse("when:same-day-as()").filter, Expr::And(vec![]));
        assert!(!parse("when:same-day-as()").refused.is_empty());
        // And it is still a question about the timeline.
        assert_eq!(parse("when:same-day-as(today)").source_of(), Source::Events);
    }

    /// `| where` reads the columns of the answer, which is a different
    /// question from the one the filter half asks — `quiet > longest`
    /// compares two columns of the same row.
    #[test]
    fn where_compares_the_columns_of_a_row() {
        let Stage::Where(test) = &parse("events | seq gaps by who | where quiet > longest").stages[1]
        else {
            panic!("the second stage is a where");
        };
        assert_eq!(
            *test,
            Test::Compare(
                Operand::Column("quiet".into()),
                Comparison::GreaterThan,
                Operand::Column("longest".into())
            )
        );
    }

    /// A stretch of time is read where it is written. `quiet` is counted in
    /// days, so `6mo` has to become days before anything can be compared.
    #[test]
    fn a_stretch_of_time_is_read_as_days() {
        assert_eq!(Operand::of("6mo"), Operand::Days(180.0));
        assert_eq!(Operand::of("2y"), Operand::Days(730.0));
        assert_eq!(Operand::of("3w"), Operand::Days(21.0));
        assert_eq!(Operand::of("90"), Operand::Number(90.0));
        assert_eq!(Operand::of("quiet"), Operand::Column("quiet".into()));
        // Quoted, so it is the word and not a column called `done`.
        assert_eq!(Operand::of("\"done\""), Operand::Text("done".into()));
    }

    #[test]
    fn a_where_that_is_not_a_comparison_says_so() {
        for broken in [
            "events | where",
            "events | where quiet",
            "events | where quiet >",
            "events | where quiet ~ longest",
        ] {
            assert!(!parse(broken).refused.is_empty(), "'{broken}' was let through");
        }
    }

    /// What the assistant does when requiring every word found nothing. It
    /// used to be a flag only the FTS path read; now it is the language.
    #[test]
    fn widening_to_any_word_keeps_every_other_filter_required() {
        let asked = parse("is:note decide pricing disagreed");
        assert_eq!(asked.word_count(), 3);
        assert_eq!(
            asked.any_word().filter,
            Expr::And(vec![
                term(Field::Kind, "note"),
                Expr::Or(vec![
                    term(Field::Text, "decide"),
                    term(Field::Text, "pricing"),
                    term(Field::Text, "disagreed"),
                ]),
            ])
        );
        // One word is already as wide as it gets.
        assert_eq!(parse("is:note decide").any_word().filter, parse("is:note decide").filter);
    }

    #[test]
    fn shaping_words_are_not_conditions() {
        // `sort:` says nothing about which rows match, so it is not in the
        // filter — otherwise every walker of the tree would have to skip it.
        let q = parse("is:task sort:-priority limit:5 columns:title");
        assert_eq!(q.filter, term(Field::Kind, "task"));
        assert_eq!(q.limit, Some(5));
        assert_eq!(q.columns, vec!["title"]);
        assert!(q.sort.is_some_and(|s| s.descending && s.key == "priority"));
    }

    /// The point of the whole step: when `OR` starts parsing in step 3, a
    /// caller still reading the flat view must complain rather than answer a
    /// smaller question than the one asked.
    #[test]
    fn the_flat_view_refuses_an_or_rather_than_keeping_one_branch() {
        let either = Query {
            filter: Expr::Or(vec![
                term(Field::Tag, "gia-đình"),
                term(Field::Tag, "công-việc"),
            ]),
            ..Default::default()
        };
        let flat = ParsedQuery::of(either);
        assert!(
            flat.tag_filters.is_empty(),
            "neither branch may be kept — half an OR is a different question"
        );
        assert_eq!(flat.refused.len(), 1, "and it has to say so: {flat:?}");
    }

    #[test]
    fn the_flat_view_refuses_a_negated_group_too() {
        let neither = Query {
            filter: Expr::Not(Box::new(Expr::And(vec![
                term(Field::Tag, "gia-đình"),
                term(Field::Kind, "note"),
            ]))),
            ..Default::default()
        };
        let flat = ParsedQuery::of(neither);
        assert!(flat.tag_exclusions.is_empty() && flat.type_filter.is_none());
        assert_eq!(flat.refused.len(), 1, "{flat:?}");
    }

    /// A phone turns `"` into `“` without being asked. Slicing bytes off a
    /// curly quote panicked the parser outright — every caller of
    /// `parse_query`, from the search bar to the assistant's own tool.
    #[test]
    fn a_smart_quote_does_not_crash_the_parser() {
        assert_eq!(parse_query("with:“Khánh”").with, vec!["Khánh"]);
        assert_eq!(parse_query("tag:“gia đình”").tag_filters, vec!["gia đình"]);
        assert_eq!(parse_query("-“báo cáo”").exclude_terms, vec!["báo cáo"]);
        assert_eq!(parse_query("“báo cáo”").fts_terms, vec!["\"báo cáo\""]);
    }
}