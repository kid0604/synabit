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

#[cfg(test)]
mod tests {
    use super::*;

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
