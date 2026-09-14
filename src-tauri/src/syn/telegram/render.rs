//! Syn's answer, as Telegram can show it.
//!
//! Two layers, as Hermes has: the prompt asks for answers that suit a phone
//! (`Surface::prompt_block`), and this makes sure of it. The second layer
//! exists because the first is a request, not a promise — a model told not to
//! draw mermaid will sometimes draw mermaid.
//!
//! What cannot be shown becomes something that can:
//!
//! - `[[Title]]`, a link that only means something inside the app → **Title**
//! - a mermaid block → a line saying there is a chart on the computer
//! - an image, which is a path into the vault → a line saying so
//! - a table → one line per row
//!
//! Output is Telegram's HTML subset rather than MarkdownV2, whose escaping
//! rules are the most common reason a message a model wrote is refused.
//!
//! # Splitting
//!
//! Telegram takes 4,096 characters a message. The answer is cut between
//! top-level blocks, so no tag is ever left open across two messages; a single
//! block longer than that is split on its own — a code block into several code
//! blocks, anything else into plain text.

use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag, TagEnd};

/// Telegram's ceiling for one message, in UTF-16 code units.
pub const LIMIT: usize = 4096;

/// What stands in for what a chat cannot show.
pub struct Placeholders<'a> {
    pub chart: &'a str,
    pub image: &'a str,
}

/// Escape text for Telegram's HTML.
pub fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(c),
        }
    }
    out
}

/// The words of some HTML, without the markup.
pub fn plain(html: &str) -> String {
    let tags = regex::Regex::new(r"<[^>]*>").expect("a valid pattern");
    tags.replace_all(html, "")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&amp;", "&")
}

/// An answer, as the messages to send.
pub fn to_html(markdown: &str, say: &Placeholders<'_>) -> Vec<String> {
    pack(blocks(&unlink(markdown), say))
}

fn units(text: &str) -> usize {
    text.encode_utf16().count()
}

/// `[[Title]]` and `[[Title|shown]]`, as bold text.
fn unlink(markdown: &str) -> String {
    let link = regex::Regex::new(r"\[\[([^\]\|\n]+)(?:\|([^\]\n]+))?\]\]").expect("a valid pattern");
    link.replace_all(markdown, |c: &regex::Captures<'_>| {
        let shown = c.get(2).or_else(|| c.get(1)).map(|m| m.as_str().trim()).unwrap_or_default();
        format!("**{shown}**")
    })
    .into_owned()
}

fn is_web(url: &str) -> bool {
    let url = url.trim().to_ascii_lowercase();
    url.starts_with("https://") || url.starts_with("http://") || url.starts_with("mailto:")
}

fn pre(language: &str, code: &str) -> String {
    let named = !language.is_empty()
        && language.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '+'));
    let class = if named { format!(" class=\"language-{language}\"") } else { String::new() };
    format!("<pre><code{class}>{}</code></pre>", escape(code))
}

/// Room left in a message for one code block's markup.
const PRE_ROOM: usize = LIMIT - 64;

/// A code block too long for one message, as several.
fn pre_pieces(language: &str, code: &str) -> Vec<String> {
    let mut pieces = Vec::new();
    let mut current = String::new();
    for line in code.lines() {
        let candidate = if current.is_empty() { line.to_string() } else { format!("{current}\n{line}") };
        if units(&escape(&candidate)) > PRE_ROOM && !current.is_empty() {
            pieces.push(pre(language, &current));
            current = line.to_string();
        } else {
            current = candidate;
        }
        // One line that will not fit anywhere is cut where it has to be.
        while units(&escape(&current)) > PRE_ROOM {
            let (head, tail) = split_at_units(&current, PRE_ROOM / 5);
            pieces.push(pre(language, &head));
            current = tail;
        }
    }
    if !current.is_empty() {
        pieces.push(pre(language, &current));
    }
    pieces
}

/// Cut text so the first part is at most `max` UTF-16 units, preferring a space.
fn split_at_units(text: &str, max: usize) -> (String, String) {
    let mut used = 0;
    let mut cut = text.len();
    let mut last_space = None;
    for (index, c) in text.char_indices() {
        if used + c.len_utf16() > max {
            cut = index;
            break;
        }
        used += c.len_utf16();
        if c.is_whitespace() {
            last_space = Some(index);
        }
    }
    if cut < text.len() {
        if let Some(space) = last_space.filter(|&s| s > cut / 2) {
            cut = space;
        }
    }
    let (head, tail) = text.split_at(cut);
    (head.to_string(), tail.trim_start().to_string())
}

struct Writer<'a> {
    say: &'a Placeholders<'a>,
    blocks: Vec<String>,
    current: String,
    depth: usize,
    lists: Vec<Option<u64>>,
    code: Option<(String, String)>,
    links: Vec<bool>,
    image: usize,
    cell: Option<String>,
    row: Vec<String>,
}

impl Writer<'_> {
    fn write(&mut self, text: &str) {
        match self.cell.as_mut() {
            Some(cell) => cell.push_str(text),
            None => self.current.push_str(text),
        }
    }

    fn open(&mut self) {
        self.depth += 1;
    }

    fn close(&mut self) {
        self.depth = self.depth.saturating_sub(1);
        if self.depth == 0 {
            let block = self.current.trim().to_string();
            self.current.clear();
            if !block.is_empty() {
                self.blocks.push(block);
            }
        }
    }

    fn row_done(&mut self, bold: bool) {
        let row = self.row.join(" · ");
        self.row.clear();
        if bold {
            self.write(&format!("<b>{row}</b>\n"));
        } else {
            self.write(&row);
            self.write("\n");
        }
    }

    fn event(&mut self, event: Event<'_>) {
        match event {
            Event::Start(tag) => self.start(tag),
            Event::End(tag) => self.end(tag),
            Event::Text(text) => {
                if let Some((_, code)) = self.code.as_mut() {
                    code.push_str(&text);
                } else if self.image == 0 {
                    self.write(&escape(&text));
                }
            }
            Event::Code(text) | Event::InlineMath(text) | Event::DisplayMath(text) => {
                self.write(&format!("<code>{}</code>", escape(&text)));
            }
            Event::Html(text) | Event::InlineHtml(text) => {
                if let Some((_, code)) = self.code.as_mut() {
                    code.push_str(&text);
                } else {
                    self.write(&escape(&text));
                }
            }
            Event::SoftBreak | Event::HardBreak => {
                if self.image == 0 {
                    self.write("\n");
                }
            }
            Event::Rule => {
                if self.depth == 0 {
                    self.blocks.push("———".to_string());
                } else {
                    self.write("———\n");
                }
            }
            Event::TaskListMarker(done) => self.write(if done { "☑ " } else { "☐ " }),
            Event::FootnoteReference(label) => self.write(&format!("[{}]", escape(&label))),
        }
    }

    fn start(&mut self, tag: Tag<'_>) {
        match tag {
            Tag::Paragraph | Tag::HtmlBlock | Tag::Table(_) | Tag::FootnoteDefinition(_) => self.open(),
            Tag::Heading { .. } => {
                self.open();
                self.write("<b>");
            }
            Tag::BlockQuote(_) => {
                self.open();
                self.write("<blockquote>");
            }
            Tag::CodeBlock(kind) => {
                self.open();
                let language = match kind {
                    CodeBlockKind::Fenced(info) => info.split_whitespace().next().unwrap_or("").to_string(),
                    CodeBlockKind::Indented => String::new(),
                };
                self.code = Some((language, String::new()));
            }
            Tag::List(start) => {
                if !self.lists.is_empty() && !self.current.ends_with('\n') {
                    self.write("\n");
                }
                self.open();
                self.lists.push(start);
            }
            Tag::Item => {
                self.open();
                let indent = "  ".repeat(self.lists.len().saturating_sub(1));
                let marker = match self.lists.last_mut() {
                    Some(Some(n)) => {
                        let marker = format!("{n}. ");
                        *n += 1;
                        marker
                    }
                    _ => "• ".to_string(),
                };
                self.write(&indent);
                self.write(&marker);
            }
            Tag::TableHead | Tag::TableRow => self.row.clear(),
            Tag::TableCell => self.cell = Some(String::new()),
            Tag::Emphasis => self.write("<i>"),
            Tag::Strong => self.write("<b>"),
            Tag::Strikethrough => self.write("<s>"),
            Tag::Link { dest_url, .. } => {
                let web = is_web(&dest_url);
                if web {
                    self.write(&format!("<a href=\"{}\">", escape(&dest_url)));
                }
                self.links.push(web);
            }
            Tag::Image { .. } => {
                if self.image == 0 {
                    let image = format!("<i>{}</i>", escape(self.say.image));
                    self.write(&image);
                }
                self.image += 1;
            }
            _ => {}
        }
    }

    fn end(&mut self, tag: TagEnd) {
        match tag {
            TagEnd::Paragraph => {
                self.write(if self.lists.is_empty() { "\n\n" } else { "\n" });
                self.close();
            }
            TagEnd::Heading { .. } => {
                self.write("</b>\n\n");
                self.close();
            }
            TagEnd::BlockQuote { .. } => {
                let kept = self.current.trim_end().len();
                self.current.truncate(kept);
                self.write("</blockquote>\n\n");
                self.close();
            }
            TagEnd::CodeBlock => {
                if let Some((language, code)) = self.code.take() {
                    let code = code.trim_end_matches('\n');
                    if language.eq_ignore_ascii_case("mermaid") {
                        let chart = format!("<i>{}</i>", escape(self.say.chart));
                        self.write(&chart);
                    } else if self.depth == 1 && units(&escape(code)) > PRE_ROOM {
                        // Top-level and too long for one message: several code
                        // blocks, each a block of its own.
                        let mut pieces = pre_pieces(&language, code);
                        let last = pieces.pop().unwrap_or_default();
                        self.blocks.extend(pieces);
                        self.write(&last);
                    } else {
                        self.write(&pre(&language, code));
                    }
                    self.write("\n\n");
                }
                self.close();
            }
            TagEnd::List { .. } => {
                self.lists.pop();
                if self.lists.is_empty() {
                    self.write("\n");
                }
                self.close();
            }
            TagEnd::Item => {
                if !self.current.ends_with('\n') {
                    self.write("\n");
                }
                self.close();
            }
            TagEnd::Table => {
                self.write("\n");
                self.close();
            }
            TagEnd::TableHead => self.row_done(true),
            TagEnd::TableRow => self.row_done(false),
            TagEnd::TableCell => {
                if let Some(cell) = self.cell.take() {
                    self.row.push(cell.trim().to_string());
                }
            }
            TagEnd::Emphasis => self.write("</i>"),
            TagEnd::Strong => self.write("</b>"),
            TagEnd::Strikethrough => self.write("</s>"),
            TagEnd::Link => {
                if self.links.pop() == Some(true) {
                    self.write("</a>");
                }
            }
            TagEnd::Image => self.image = self.image.saturating_sub(1),
            TagEnd::HtmlBlock | TagEnd::FootnoteDefinition => self.close(),
            _ => {}
        }
    }
}

fn blocks(markdown: &str, say: &Placeholders<'_>) -> Vec<String> {
    let options = Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS;
    let mut writer = Writer {
        say,
        blocks: Vec::new(),
        current: String::new(),
        depth: 0,
        lists: Vec::new(),
        code: None,
        links: Vec::new(),
        image: 0,
        cell: None,
        row: Vec::new(),
    };
    for event in Parser::new_ext(markdown, options) {
        writer.event(event);
    }
    let rest = writer.current.trim().to_string();
    if !rest.is_empty() {
        writer.blocks.push(rest);
    }
    writer.blocks
}

/// A block too long for a message on its own: its words, in pieces, without
/// its formatting. Losing the bold is better than losing the paragraph.
fn fit(block: String) -> Vec<String> {
    if units(&block) <= LIMIT {
        return vec![block];
    }
    let mut pieces = Vec::new();
    let mut rest = plain(&block);
    while !rest.is_empty() {
        // A fifth of the limit before escaping, so that even text made entirely
        // of characters that escape to five cannot overflow.
        let (head, tail) = split_at_units(&rest, LIMIT / 5);
        pieces.push(escape(&head));
        rest = tail;
    }
    pieces
}

fn pack(blocks: Vec<String>) -> Vec<String> {
    let mut messages = Vec::new();
    let mut current = String::new();
    for block in blocks.into_iter().flat_map(fit) {
        if !current.is_empty() && units(&current) + 2 + units(&block) > LIMIT {
            messages.push(std::mem::take(&mut current));
        }
        if !current.is_empty() {
            current.push_str("\n\n");
        }
        current.push_str(&block);
    }
    if !current.trim().is_empty() {
        messages.push(current);
    }
    messages
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAY: Placeholders<'static> = Placeholders {
        chart: "(chart — open on the computer)",
        image: "(image — open on the computer)",
    };

    fn one(markdown: &str) -> String {
        let messages = to_html(markdown, &SAY);
        assert_eq!(messages.len(), 1, "{messages:?}");
        messages.into_iter().next().unwrap_or_default()
    }

    /// A link into the vault is a name, not a link nobody can follow.
    #[test]
    fn a_wikilink_becomes_its_title() {
        assert_eq!(one("Xem [[Kế hoạch Q4]] nhé."), "Xem <b>Kế hoạch Q4</b> nhé.");
        assert_eq!(one("[[Notes/q4.md|kế hoạch]]"), "<b>kế hoạch</b>");
    }

    /// Nothing the chat cannot draw reaches it as source.
    #[test]
    fn charts_and_vault_images_are_named_not_sent() {
        let chart = one("Đây:\n\n```mermaid\npie\n  \"A\": 1\n```\n");
        assert!(chart.contains("(chart — open on the computer)"), "{chart}");
        assert!(!chart.contains("mermaid") && !chart.contains("pie"), "{chart}");

        let image = one("![hoá đơn](assets/abc123.png)");
        assert!(image.contains("(image — open on the computer)"), "{image}");
        assert!(!image.contains("assets/") && !image.contains("hoá đơn"), "{image}");
    }

    /// Text a model wrote cannot become markup.
    #[test]
    fn what_looks_like_markup_is_escaped() {
        let out = one("a < b && c > d <script>alert(1)</script>");
        assert!(!out.contains("<script>"), "{out}");
        assert!(out.contains("a &lt; b &amp;&amp; c &gt; d"), "{out}");
    }

    #[test]
    fn inline_formatting_survives() {
        assert_eq!(
            one("**đậm** *nghiêng* `mã` ~~gạch~~ [web](https://example.com) [vault](Notes/a.md)"),
            "<b>đậm</b> <i>nghiêng</i> <code>mã</code> <s>gạch</s> <a href=\"https://example.com\">web</a> vault"
        );
    }

    #[test]
    fn lists_and_tables_read_as_lines() {
        let list = one("- sữa\n- trứng\n\n1. một\n2. hai\n");
        assert!(list.contains("• sữa\n• trứng"), "{list}");
        assert!(list.contains("1. một\n2. hai"), "{list}");

        let table = one("| Tên | Giá |\n|---|---|\n| Cà phê | 30k |\n| Bánh | 20k |\n");
        assert!(table.contains("<b>Tên · Giá</b>"), "{table}");
        assert!(table.contains("Cà phê · 30k\nBánh · 20k"), "{table}");
    }

    /// Long answers become several messages, each within the limit, and no
    /// message carries a tag it does not close.
    #[test]
    fn a_long_answer_is_several_whole_messages() {
        let paragraph = "Một câu **khá dài** về kế hoạch quý bốn, có *nhấn mạnh* và `mã`. ".repeat(40);
        let markdown = vec![paragraph; 6].join("\n\n");
        let messages = to_html(&markdown, &SAY);

        assert!(messages.len() > 1);
        for message in &messages {
            assert!(units(message) <= LIMIT, "{} units", units(message));
            for tag in ["b", "i", "code"] {
                assert_eq!(
                    message.matches(&format!("<{tag}>")).count(),
                    message.matches(&format!("</{tag}>")).count(),
                    "unbalanced <{tag}>"
                );
            }
        }
    }

    /// A code block too long for one message is several code blocks.
    #[test]
    fn a_long_code_block_is_several_code_blocks() {
        let code = (0..400).map(|i| format!("let value_{i} = compute({i}) + 1;")).collect::<Vec<_>>().join("\n");
        let messages = to_html(&format!("```rust\n{code}\n```"), &SAY);

        assert!(messages.len() > 1);
        for message in &messages {
            assert!(units(message) <= LIMIT);
            assert!(message.starts_with("<pre><code class=\"language-rust\">"), "{}", &message[..40]);
            assert!(message.ends_with("</code></pre>"));
        }
    }

    /// One paragraph longer than a message, with nothing to break on but spaces.
    #[test]
    fn a_paragraph_longer_than_a_message_is_still_sent() {
        let messages = to_html(&"chữ ".repeat(3000), &SAY);
        assert!(messages.len() > 1);
        assert!(messages.iter().all(|m| units(m) <= LIMIT));
    }

    #[test]
    fn plain_takes_the_markup_off() {
        assert_eq!(plain("<b>a &lt; b</b> &amp; <i>c</i>"), "a < b & c");
    }
}
