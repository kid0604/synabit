//! What an answer is allowed to claim, checked against what was read.
//!
//! # The failure this exists for
//!
//! Asked for the addresses of the technical articles in *This Week in Rust*
//! #667, Syn gave nineteen. Seven of the ten checked were 404, and the wrong
//! ones were not even on the right host: Cargo's scheduler went to
//! `kobzol.github.io`, which is `spirali.github.io`; the safety-certified
//! product to `ferrous-systems.com`, which is `sonair.com`. They were assembled
//! from the titles on the page and a guess at who publishes that sort of thing.
//!
//! The answer was marked `Grounded`, and by the definition in `footing` it was:
//! a tool that only reads had come back with a real page. **`footing` measures
//! whether a source was opened. It has never measured whether the answer stands
//! on it.**
//!
//! # Why a string check and not a model
//!
//! Because this particular claim is decidable. An address is either one that
//! appeared in something this run read, or it is one nobody has seen — and that
//! is `contains`, not judgement. It costs nothing, it cannot be talked out of
//! its answer by the page it is checking, and it catches all nineteen.
//!
//! It is the sixth time in this codebase that arithmetic on the transcript has
//! beaten asking the model, and the reasoning is the same every time.
//!
//! # What it deliberately does not do
//!
//! It does not check that a *fact* came from the page — that is not decidable
//! and pretending otherwise would put a stamp on something nobody verified. It
//! checks addresses, because an address is the one claim in an answer that is
//! exact, checkable, and silently wrong in a way the reader only discovers by
//! clicking.
//!
//! Citations are the second such claim. Retrieved context reaches the model
//! numbered, the model cites `[n]`, and whether a source numbered `n` was handed
//! to it is `n <= sources.len()` — decidable, so decided. Whether the sentence
//! before `[2]` is what source 2 says is not, and is not claimed.

/// Every http address in a piece of text.
///
/// A scan rather than a parse: this wants the addresses in the order they
/// appear and must not fail on the markdown, punctuation and Vietnamese around
/// them.
pub fn addresses_in(text: &str) -> Vec<String> {
    let mut found = Vec::new();
    let bytes = text.as_bytes();
    let mut from = 0;

    while let Some(at) = text[from..].find("http") {
        let start = from + at;
        let rest = &text[start..];
        if !(rest.starts_with("http://") || rest.starts_with("https://")) {
            from = start + 4;
            continue;
        }

        // Up to the first character that cannot be in a URL. The trailing
        // trims below take care of the ones that can be but usually are not.
        let end = rest
            .find(|c: char| c.is_whitespace() || matches!(c, '"' | '<' | '>' | '`' | '\\' | '|'))
            .unwrap_or(rest.len());
        let mut url = &rest[..end];

        // `](url)` in markdown, `(url).` in a sentence, `url,` in a list. A
        // closing bracket only ends the address if nothing opened it inside.
        while let Some(last) = url.chars().last() {
            let bare = match last {
                '.' | ',' | ';' | ':' | '!' | '?' | '\'' | '”' | '’' => true,
                // Markdown emphasis closing around a link: `**[text](url)**`.
                // Without this the address read as `…/668/)**`, matched
                // nothing that had been read, and the answer carried a warning
                // that the one real link in it might not exist.
                '*' => true,
                ')' => !url.contains('('),
                ']' => !url.contains('['),
                _ => false,
            };
            if !bare {
                break;
            }
            url = &url[..url.len() - last.len_utf8()];
        }

        if url.len() > "https://".len() {
            found.push(url.to_string());
        }
        from = start + end.max(1);
        let _ = bytes;
    }

    found
}

/// Whether an address appears in something that was read.
///
/// Compared on the address as written, and on the address without a trailing
/// slash, because a model quoting a link back will add or drop one and that is
/// not a fabrication. Nothing looser than that: `example.com/a` and
/// `example.com/b` are different pages however similar they look, and the whole
/// point of this is that a nearly-right address is the dangerous kind.
pub fn was_read(url: &str, read: &[String]) -> bool {
    let bare = url.strip_suffix('/').unwrap_or(url);
    read.iter().any(|seen| seen.contains(url) || seen.contains(bare))
}

/// The addresses in an answer that nobody ever read.
pub fn invented(answer: &str, read: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for url in addresses_in(answer) {
        if !was_read(&url, read) && !out.contains(&url) {
            out.push(url);
        }
    }
    out
}

/// What to say when an answer carries addresses nobody read.
///
/// # Why the answer is not rewritten
///
/// Because an address inside a sentence is load-bearing — "the article is
/// [here](…)" with the link cut out is a sentence that means nothing — and
/// because editing a model's answer to make it look correct is the failure
/// this whole module exists to catch, done by hand.
///
/// So the answer stands and the truth is added under it. The person sees what
/// was said, sees which parts of it were made up, and is not left to find out
/// by clicking.
pub fn warning(invented: &[String]) -> String {
    if invented.is_empty() {
        return String::new();
    }

    let listed = invented
        .iter()
        .map(|u| format!("- {u}"))
        .collect::<Vec<_>>()
        .join("\n");

    format!(
        "\n\n---\n\n⚠️ **{count} address{es} above {were} not on any page I read**, so {they} \
         may not exist. I have left {them} in place rather than quietly editing what I said, \
         but do not trust {them}:\n\n{listed}",
        count = invented.len(),
        es = if invented.len() == 1 { "" } else { "es" },
        were = if invented.len() == 1 { "was" } else { "were" },
        they = if invented.len() == 1 { "it" } else { "they" },
        them = if invented.len() == 1 { "it" } else { "them" },
    )
}

// ═══════════════════════════════════════════════════════════════
//  CITATIONS
// ═══════════════════════════════════════════════════════════════

/// One citation as written: where it is in the answer, and the numbers in it.
///
/// A list because models write `[1, 3]` as often as `[1][3]`, and both are
/// asking for the same two sources.
#[derive(Debug, PartialEq)]
struct Mark {
    at: std::ops::Range<usize>,
    numbers: Vec<usize>,
}

/// Every `[n]` and `[n, m]` in an answer, outside code.
///
/// # What is not a citation
///
/// - `[[Title]]`, which is a link to a note and the other way this app cites.
/// - `[2](https://…)`, which is a link whose text happens to be a number.
/// - Anything in a code block or a code span: `v[1]` is an index, not a source.
/// - A number of more than three digits, which is a year or an amount in
///   brackets rather than the thousandth thing retrieval found.
///
/// A scan rather than a regex for the same reason as `addresses_in`: it has to
/// know where the code is, and a regex that knows that is harder to read than
/// the loop.
fn marks(text: &str) -> Vec<Mark> {
    let bytes = text.as_bytes();
    let mut found = Vec::new();
    let mut i = 0;
    let mut in_fence = false;
    let mut in_span = false;

    while i < bytes.len() {
        if bytes[i..].starts_with(b"```") {
            in_fence = !in_fence;
            i += 3;
            continue;
        }
        if bytes[i] == b'`' && !in_fence {
            in_span = !in_span;
            i += 1;
            continue;
        }
        if bytes[i] != b'[' || in_fence || in_span || (i > 0 && bytes[i - 1] == b'[') {
            i += 1;
            continue;
        }
        let Some(close) = text[i + 1..].find(']').map(|c| i + 1 + c) else {
            break;
        };
        let inside = &text[i + 1..close];
        let numbers: Option<Vec<usize>> = inside
            .split(',')
            .map(|n| {
                let n = n.trim();
                (!n.is_empty() && n.len() <= 3 && n.bytes().all(|b| b.is_ascii_digit()))
                    .then(|| n.parse().ok())
                    .flatten()
            })
            .collect();
        let after = bytes.get(close + 1).copied();
        match numbers {
            Some(numbers) if after != Some(b']') && after != Some(b'(') => {
                found.push(Mark { at: i..close + 1, numbers });
                i = close + 1;
            }
            _ => i += 1,
        }
    }
    found
}

/// The source numbers an answer cites, each once, in the order first cited.
pub fn citations_in(text: &str) -> Vec<usize> {
    let mut out = Vec::new();
    for n in marks(text).into_iter().flat_map(|m| m.numbers) {
        if !out.contains(&n) {
            out.push(n);
        }
    }
    out
}

/// The numbers an answer cites that no source carries.
///
/// The same failure as an invented address, one level down: `[4]` under an
/// answer that was handed three sources reads as a reference and points at
/// nothing. `0` is in here too — nothing is numbered from zero.
pub fn uncited_numbers(text: &str, sources: usize) -> Vec<usize> {
    citations_in(text)
        .into_iter()
        .filter(|&n| n == 0 || n > sources)
        .collect()
}

/// The sources in the order they should stand under an answer, and how the
/// numbers in the answer move to match.
///
/// # Why cited first
///
/// Because the chips under an answer are read as *what it stood on*, and ten
/// retrieved notes of which the answer used one make the other nine look like
/// evidence. Cited sources lead, in the order the answer first cites them; the
/// rest follow, as retrieved, when `keep_uncited` says they should be shown at
/// all.
///
/// # Why the numbers move
///
/// A citation opens the source at its place under the answer — `[2]` is the
/// second chip. Reorder the chips and leave the text alone, and every `[3]`
/// that used to be right now opens something else. So the answer's numbers are
/// rewritten to the new places (`renumbered`), and nothing else in it is: no
/// sentence is removed or reworded, and a number that matched no source is
/// left exactly as it was written, to be flagged.
pub fn cited_first<T>(sources: Vec<T>, cited: &[usize], keep_uncited: bool) -> (Vec<T>, Vec<(usize, usize)>) {
    let count = sources.len();
    let valid: Vec<usize> = cited.iter().copied().filter(|&n| n >= 1 && n <= count).collect();

    let mut slots: Vec<Option<T>> = sources.into_iter().map(Some).collect();
    let mut ordered = Vec::new();
    let mut moved = Vec::new();

    for &n in &valid {
        if let Some(source) = slots[n - 1].take() {
            ordered.push(source);
            moved.push((n, ordered.len()));
        }
    }
    if keep_uncited {
        for (i, slot) in slots.into_iter().enumerate() {
            if let Some(source) = slot {
                ordered.push(source);
                moved.push((i + 1, ordered.len()));
            }
        }
    }
    (ordered, moved)
}

/// The answer with each cited number moved to where its source now stands.
///
/// Only the digits inside a citation change. A number with nowhere to go is
/// written as it was.
pub fn renumbered(text: &str, moved: &[(usize, usize)]) -> String {
    let to = |n: usize| moved.iter().find(|(from, _)| *from == n).map(|(_, to)| *to).unwrap_or(n);
    let mut out = String::with_capacity(text.len());
    let mut last = 0;
    for mark in marks(text) {
        out.push_str(&text[last..mark.at.start]);
        let numbers: Vec<String> = mark.numbers.iter().map(|&n| to(n).to_string()).collect();
        out.push('[');
        out.push_str(&numbers.join(", "));
        out.push(']');
        last = mark.at.end;
    }
    out.push_str(&text[last..]);
    out
}

/// What to say when an answer cites a source that does not exist.
///
/// Added under the answer rather than cut out of it, for the reason `warning`
/// gives: the sentence carrying `[4]` may be true, and only the reference is
/// known to be wrong. The reader is told which reference, and can weigh the
/// sentence for themselves.
pub fn citation_warning(unknown: &[usize], sources: usize) -> String {
    if unknown.is_empty() {
        return String::new();
    }
    let listed = unknown.iter().map(|n| format!("[{n}]")).collect::<Vec<_>>().join(", ");
    format!(
        "\n\n---\n\n⚠️ **{listed} above {points} to no source.** I was given {sources} \
         source{s}, so {what} may not be backed by anything I read.",
        points = if unknown.len() == 1 { "points" } else { "point" },
        s = if sources == 1 { "" } else { "s" },
        what = if unknown.len() == 1 { "the sentence it follows" } else { "the sentences they follow" },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn citations_are_read_in_the_order_they_are_first_made() {
        let answer = "Mật khẩu là ha-noi-2026 [2]. Giá theo ghế [1][3], và Mai phản đối [3, 1].";
        assert_eq!(citations_in(answer), vec![2, 1, 3]);
    }

    /// A wiki-link, a link whose text is a number, an index in code and a year
    /// in brackets are not citations.
    #[test]
    fn what_only_looks_like_a_citation_is_left_alone() {
        let answer = "See [[1]] and [2](https://a.test/2). In code `v[3]` and\n\
                      ```\nlet x = a[4];\n```\nThe report [2026] said so.";
        assert!(citations_in(answer).is_empty(), "{:?}", citations_in(answer));
    }

    #[test]
    fn a_number_past_the_sources_is_caught_and_zero_with_it() {
        let answer = "One [1], four [4], nothing [0].";
        assert_eq!(uncited_numbers(answer, 3), vec![4, 0]);
        assert!(uncited_numbers("One [1], three [3].", 3).is_empty());
    }

    #[test]
    fn cited_sources_stand_first_and_the_answer_follows_them() {
        let sources = vec!["a", "b", "c", "d"];
        let answer = "Per-seat [3]. Mai disagreed [3][1]. Also [9].";

        let (ordered, moved) = cited_first(sources, &citations_in(answer), true);
        assert_eq!(ordered, vec!["c", "a", "b", "d"]);

        let rewritten = renumbered(answer, &moved);
        assert_eq!(rewritten, "Per-seat [1]. Mai disagreed [1][2]. Also [9].");
        // And every number still opens what it opened before.
        assert_eq!(ordered[0], "c");
        assert_eq!(ordered[1], "a");
    }

    /// When tools were used only the cited sources stand under the answer; the
    /// rest of what retrieval found is not what it stood on.
    #[test]
    fn without_the_uncited_only_what_was_cited_is_kept() {
        let (ordered, moved) = cited_first(vec!["a", "b", "c"], &[2], false);
        assert_eq!(ordered, vec!["b"]);
        assert_eq!(renumbered("It was b [2].", &moved), "It was b [1].");
    }

    /// Only the digits in the brackets move. Every sentence stays, word for
    /// word — rewriting an answer to look right is what this module exists
    /// to catch.
    #[test]
    fn renumbering_changes_nothing_but_the_numbers() {
        let answer = "Câu một [2]. `a[2]` là code. [[Ghi chú]] vẫn là link.";
        let rewritten = renumbered(answer, &[(2, 1)]);
        assert_eq!(rewritten, "Câu một [1]. `a[2]` là code. [[Ghi chú]] vẫn là link.");
    }

    #[test]
    fn a_citation_to_nothing_is_named_under_the_answer() {
        assert!(citation_warning(&[], 3).is_empty());

        let one = citation_warning(&[4], 3);
        assert!(one.contains("[4] above points to no source"), "{one}");
        assert!(one.contains("given 3 sources"), "{one}");

        let two = citation_warning(&[4, 7], 1);
        assert!(two.contains("[4], [7] above point to no source"), "{two}");
        assert!(two.contains("given 1 source,"), "{two}");
    }

    #[test]
    fn addresses_are_found_through_the_punctuation_around_them() {
        let text = "Đây là [bài viết](https://genk.vn/poco-f9.chn) và \
                    <https://vnexpress.net/a.html>, xem thêm https://lwn.net/x/ nhé. \
                    Nguồn: `https://a.test/b`.";

        assert_eq!(
            addresses_in(text),
            [
                "https://genk.vn/poco-f9.chn",
                "https://vnexpress.net/a.html",
                "https://lwn.net/x/",
                "https://a.test/b",
            ]
        );
    }

    #[test]
    fn a_bracket_that_belongs_to_the_address_is_kept() {
        let text = "see https://en.wikipedia.org/wiki/Rust_(programming_language) for more";
        assert_eq!(
            addresses_in(text),
            ["https://en.wikipedia.org/wiki/Rust_(programming_language)"]
        );
    }

    /// A bold link is still a link.
    ///
    /// The answer said `👉 **[This Week in Rust 668](https://…/668/)**`, and the
    /// address came out as `https://…/668/)**`. Nothing read had been at that
    /// address — nothing ever is — so the one real link in the answer was
    /// handed to the person with a warning that it might not exist.
    #[test]
    fn markdown_emphasis_is_not_part_of_the_address() {
        let bold = "👉 **[This Week in Rust 668](https://this-week-in-rust.org/blog/668/)**";
        assert_eq!(addresses_in(bold), ["https://this-week-in-rust.org/blog/668/"]);

        let read = ["đã đọc https://this-week-in-rust.org/blog/668/ hôm nay".to_string()];
        assert!(invented(bold, &read).is_empty(), "and it counts as read");
    }

    #[test]
    fn text_with_no_addresses_has_none() {
        assert!(addresses_in("không có link nào ở đây, kể cả http hay https").is_empty());
    }

    /// The nineteen, in miniature. Every one of these was on the right page and
    /// none of them was the right address.
    #[test]
    fn an_address_nobody_read_is_caught() {
        let read = vec![
            "…offers… https://wasmi-labs.github.io/blog/posts/wasmi-v2.0/ …".to_string(),
            "…https://spirali.github.io/blog/cargo-scheduler/…".to_string(),
        ];

        let answer = "- [Wasmi 2.0](https://wasmi-labs.github.io/blog/wasmi-2.0/)\n\
                      - [Cargo scheduler](https://spirali.github.io/blog/cargo-scheduler/)";

        assert_eq!(
            invented(answer, &read),
            ["https://wasmi-labs.github.io/blog/wasmi-2.0/"],
            "the near-miss is the dangerous one, and the real one passes"
        );
    }

    /// A trailing slash is not a fabrication.
    #[test]
    fn a_slash_one_way_or_the_other_is_the_same_page() {
        let read = vec!["https://genk.vn/poco-f9.chn".to_string()];
        assert!(invented("[bài](https://genk.vn/poco-f9.chn/)", &read).is_empty());

        let read = vec!["https://genk.vn/".to_string()];
        assert!(invented("xem https://genk.vn", &read).is_empty());
    }

    /// And nothing looser than that: a different path is a different page,
    /// however alike they look.
    #[test]
    fn a_different_path_on_the_same_site_is_still_invented() {
        let read = vec!["https://blog.cloudflare.com/dns-cache-memory-optimization-1111/".to_string()];
        let answer = "https://blog.cloudflare.com/how-we-saved-100-terabytes-of-memory/";

        assert_eq!(invented(answer, &read).len(), 1);
    }

    #[test]
    fn an_answer_that_invented_nothing_gets_no_warning() {
        assert!(warning(&[]).is_empty());
    }

    /// The answer is left alone and the truth is added under it. Editing what a
    /// model said to make it look right is the failure this module exists to
    /// catch, done by hand.
    #[test]
    fn the_warning_names_them_and_does_not_hide_the_answer() {
        let said = warning(&["https://a.test/one".to_string(), "https://a.test/two".to_string()]);

        assert!(said.contains("2 addresses above were not on any page I read"), "{said}");
        assert!(said.contains("https://a.test/one"));
        assert!(said.contains("https://a.test/two"));

        let one = warning(&["https://a.test/one".to_string()]);
        assert!(one.contains("1 address above was not on any page I read"), "{one}");
    }
}
