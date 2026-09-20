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
    Notes,
    Events,
}

impl Source {
    /// The word a person writes, and the word shown back to them.
    pub fn word(self) -> &'static str {
        match self {
            Source::Notes => "notes",
            Source::Events => "events",
        }
    }

    /// The source a leading word names, if it names one.
    pub fn of(word: &str) -> Option<Source> {
        match word {
            "notes" => Some(Source::Notes),
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
    fn text(field: Field, value: impl Into<String>) -> Self {
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

/// A whole question: what to match, and how to lay out what matched.
///
/// The shaping words are not part of the filter tree on purpose. `sort:`,
/// `columns:` and `limit:` say nothing about *which* rows match — they are
/// about the table, not the question — so putting them in the tree would mean
/// every walker had to skip over them.
///
/// The pipeline (`| stats …`) of §1 is not here yet: step 5 adds it when there
/// is something to parse into it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Query {
    /// Which table, when the question said. `None` means it did not, and the
    /// words it used decide — see `ParsedQuery::source_of`.
    pub source: Option<Source>,
    pub filter: Vec<Expr>,
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
}

/// Words that used to mean something and now mean it under another name.
///
/// §11. They are listed rather than deleted because deleting a keyword does
/// not make it stop parsing — it makes it parse as *a frontmatter key of that
/// name*, which answers 0 and explains nothing.
const RENAMED: &[(&str, &str)] = &[("where:", "place:"), ("magnitude:", "size:")];

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
            while let Some(&c) = chars.peek() {
                if c == '"' || c == '\u{201c}' || c == '\u{201d}' {
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
    // it is not the source at all: `#work events` is about notes tagged work
    // and the word "events", and reading it the other way would take a word
    // out of somebody's search.
    //
    // And on its own it is not a question — it is the word. `notes` and
    // `events` are ordinary English, and the free-text boxes (`search_notes`,
    // `search_tasks`, …) hand whatever was typed straight to this parser.
    // Swallowing a lone word there would quietly turn a search into a listing
    // of everything, with nothing on the screen to say so. "The whole
    // timeline" is still sayable — `events sort:-when` or `events limit:200`
    // — and those say what they want anyway.
    let names_a_source = tokens.len() > 1
        && tokens
            .first()
            .and_then(|first| Source::of(&first.to_lowercase()))
            .is_some();
    if names_a_source {
        q.source = Source::of(&tokens.remove(0).to_lowercase());
    }

    for token in tokens {
        let lower = token.to_lowercase();

        // ── The timeline's own words ───────────────────────────────
        //
        // Read before `is:` and the property filters so they are never taken
        // for a property named `with` on a note. Each keeps the person's text
        // as written: a name becomes a node id only where the vault can be
        // read, and a date only where `timeline::when` can read it.
        if let Some(stripped) = lower.strip_prefix("when:") {
            let value = unquoted(stripped);
            if !value.is_empty() {
                q.filter.push(Expr::Term(Term::text(Field::When, value)));
            }
            continue;
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
            if !value.is_empty() {
                let field = match word {
                    "with:" => Field::With,
                    "place:" => Field::Place,
                    _ => Field::About,
                };
                q.filter.push(Expr::Term(Term::text(field, value)));
            }
            continue;
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
            continue;
        }
        if let Some(stripped) = lower.strip_prefix("shape:") {
            let value = unquoted(stripped);
            if !value.is_empty() {
                q.filter.push(Expr::Term(Term::text(Field::Shape, value)));
            }
            continue;
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
            match split_comparison(value) {
                Some((comparison, rest)) => match rest.trim().parse::<f64>() {
                    Ok(number) => q.filter.push(Expr::Term(Term {
                        field: Field::Size,
                        value: Value::Number(comparison, number),
                    })),
                    Err(_) => q
                        .refused
                        .push(format!("'{rest}' is not a size to compare against")),
                },
                None if value.is_empty() => {}
                None => q.refused.push(format!(
                    "'size:{value}' has to say which way — write size:>={value} or size:<={value}"
                )),
            }
            continue;
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
            if !stripped.is_empty() {
                q.filter.push(Expr::Term(Term::text(Field::Kind, stripped)));
            }
            continue;
        }
        if let Some(stripped) = lower.strip_prefix("status:") {
            // Same widening, and here it was not merely narrow but wrong: the
            // old list read `in-progress` while every task in every vault is
            // written `in_progress`, and `backlog` and `canceled` — both real
            // statuses the Tasks app writes — were not on it at all.
            if !stripped.is_empty() {
                q.filter.push(Expr::Term(Term::text(Field::Status, stripped)));
            }
            continue;
        }

        // `date:` used to be read here into a field no runner ever looked at,
        // so `date:today` quietly matched everything. It now reaches the
        // ordinary property filter below, where `date:2019-11-05` does what it
        // says on a daily note. A date that is not one day belongs to `when:`.

        if lower == "in:title" {
            q.title_only = true;
            continue;
        }

        if token.starts_with('#') && token.len() > 1 {
            q.filter
                .push(Expr::Term(Term::text(Field::Tag, &token[1..])));
            continue;
        }
        if lower.starts_with("tag:") && lower.len() > 4 {
            q.filter
                .push(Expr::Term(Term::text(Field::Tag, strip_quotes(&lower[4..]))));
            continue;
        }

        if token.starts_with('-') && token.len() > 1 && !token.starts_with("--") {
            q.filter.push(negated(&token[1..], &lower[1..]));
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
                q.sort = Some(SortOrder {
                    key: key.to_string(),
                    descending,
                });
            } else {
                q.refused
                    .push(format!("'{key}' is not something a query can sort by"));
            }
            continue;
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
            continue;
        }

        if let Some(rest) = lower.strip_prefix("limit:") {
            match rest.trim().parse::<u32>() {
                Ok(n) => q.limit = Some(n.clamp(1, MAX_QUERY_LIMIT)),
                Err(_) => q
                    .refused
                    .push(format!("'{rest}' is not a number of rows")),
            }
            continue;
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
                q.filter.push(Expr::Term(Term {
                    field: Field::Prop(key.to_string()),
                    value,
                }));
                continue;
            }
        }

        q.filter.push(Expr::Term(Term::text(Field::Text, token)));
    }

    q
}

/// What `-x` means, given the text after the minus.
///
/// `written` keeps its casing so `-#Gia-Đình` excludes the tag as spelled;
/// `lower` is what the key/value forms are read from, the way they are
/// everywhere else.
fn negated(written: &str, lower: &str) -> Expr {
    // `-#tag` excludes the tag, not the characters. It has to be read before
    // the key/value split, which would otherwise see no colon and fall through
    // to a word exclusion — asking for notes that do not contain the literal
    // text "#gia-đình", which is every note, tagged or not.
    if let Some(tag) = written.strip_prefix('#') {
        if !tag.is_empty() {
            return Expr::Not(Box::new(Expr::Term(Term::text(Field::Tag, tag))));
        }
    }
    // `-key:value` is a property exclusion, not a word to avoid. A bare
    // `-draft` means "no note whose text says draft"; `-status:done` means
    // "not finished", and a note that never had a status satisfies it.
    if let Some((key, value)) = lower.split_once(':') {
        if !key.is_empty() && !value.is_empty() && is_queryable_key(key) {
            return Expr::Not(Box::new(Expr::Term(Term::text(
                Field::Prop(key.to_string()),
                value,
            ))));
        }
    }
    Expr::Not(Box::new(Expr::Term(Term::text(
        Field::Text,
        strip_quotes(written),
    ))))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::search::{parse_query, ParsedQuery};

    fn term(field: Field, value: &str) -> Expr {
        Expr::Term(Term::text(field, value))
    }

    #[test]
    fn a_plain_question_is_a_conjunction_of_conditions() {
        let q = parse("is:note #gia-đình báo");
        assert_eq!(
            q.filter,
            vec![
                term(Field::Kind, "note"),
                term(Field::Tag, "gia-đình"),
                term(Field::Text, "báo"),
            ]
        );
    }

    #[test]
    fn a_minus_is_a_negation_and_not_a_field_of_its_own() {
        // The flat struct had three separate exclusion lists because it had no
        // way to say "not". The tree says it once.
        assert_eq!(
            parse("-status:done -#gia-đình -báo").filter,
            vec![
                Expr::Not(Box::new(term(Field::Prop("status".into()), "done"))),
                Expr::Not(Box::new(term(Field::Tag, "gia-đình"))),
                Expr::Not(Box::new(term(Field::Text, "báo"))),
            ]
        );
    }

    #[test]
    fn shaping_words_are_not_conditions() {
        // `sort:` says nothing about which rows match, so it is not in the
        // filter — otherwise every walker of the tree would have to skip it.
        let q = parse("is:task sort:-priority limit:5 columns:title");
        assert_eq!(q.filter, vec![term(Field::Kind, "task")]);
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
            filter: vec![Expr::Or(vec![
                term(Field::Tag, "gia-đình"),
                term(Field::Tag, "công-việc"),
            ])],
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
            filter: vec![Expr::Not(Box::new(Expr::And(vec![
                term(Field::Tag, "gia-đình"),
                term(Field::Kind, "note"),
            ])))],
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
    }
}
