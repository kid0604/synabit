//! Getting the words out of a document, so they can be searched.
//!
//! Until now a file was findable by its name, its tags and its extension, and
//! by nothing else. The search index even recorded the string `"pdf"` in the
//! column reserved for a document's contents — so a contract you remembered a
//! sentence from was findable only if you also remembered what you had called
//! it.
//!
//! # What this is not
//!
//! It is not a document parser. Nothing here reconstructs a layout, a style or
//! a table; the output is a bag of words destined for FTS5, and the only
//! question worth asking of it is whether a person searching for a phrase they
//! remember will find the file that contains it.
//!
//! That is why the office formats are handled by stripping tags rather than by
//! walking a schema. A `.docx` is a zip of XML, and the words live between the
//! tags whatever the tags happen to be; a parser that understood `w:t` would be
//! more correct and no more useful, and would need a dependency this project
//! does not have. `regex` and `zip` are already here.
//!
//! # Pages
//!
//! PDFs are extracted a page at a time and kept that way. It costs nothing at
//! index time and it is the difference between "this file mentions it" and
//! "page 34 mentions it" — which is what a reader actually wants from a
//! four-hundred-page manual.

use std::path::Path;

/// The most text kept for one file.
///
/// A search index is for finding a document, not for holding it. Some files —
/// a database dump saved as `.txt`, a minified bundle — are megabytes of words
/// nobody will ever search for, and indexing them in full costs disk and
/// dilutes ranking for everything else. Two megabytes is roughly a
/// three-hundred-page book.
const MAX_TEXT: usize = 2 * 1024 * 1024;

/// Files larger than this are not opened at all.
///
/// Separate from `MAX_TEXT` because it applies before any work happens: a 4GB
/// disk image has no text in it and should not be read to discover that.
const MAX_FILE: u64 = 256 * 1024 * 1024;

/// What came out of a file, and whether anything did.
#[derive(Debug, Clone, PartialEq)]
pub enum Extraction {
    /// Text, split the way the format splits it. One entry per page for a PDF;
    /// a single entry for everything else.
    Text(Vec<String>),
    /// A format with no text to give — an image, a video, an archive. Recorded
    /// as an answer rather than a failure so it is never attempted twice.
    Unsupported,
    /// Something went wrong reading a file we should have been able to read.
    Failed(String),
}

/// Which route a given extension takes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Kind {
    Pdf,
    /// Zipped XML: docx, pptx, xlsx, epub.
    ZippedXml,
    Markup,
    Plain,
    None,
}

pub fn kind_of(extension: &str) -> Kind {
    match extension.to_lowercase().as_str() {
        "pdf" => Kind::Pdf,
        "docx" | "pptx" | "xlsx" | "epub" | "odt" | "ods" | "odp" => Kind::ZippedXml,
        "html" | "htm" | "xml" | "svg" | "xhtml" => Kind::Markup,
        "md" | "txt" | "csv" | "tsv" | "json" | "yaml" | "yml" | "toml" | "ini" | "conf"
        | "log" | "rs" | "ts" | "js" | "vue" | "py" | "java" | "c" | "h" | "cpp" | "go"
        | "rb" | "sh" | "bash" | "sql" | "css" | "scss" | "graphql" | "env" | "srt" | "vtt" => {
            Kind::Plain
        }
        _ => Kind::None,
    }
}

/// Read whatever words a file has.
pub fn extract(path: &Path, extension: &str) -> Extraction {
    let kind = kind_of(extension);
    if kind == Kind::None {
        return Extraction::Unsupported;
    }

    match std::fs::metadata(path) {
        Ok(meta) if meta.len() > MAX_FILE => {
            return Extraction::Unsupported;
        }
        Err(e) => return Extraction::Failed(e.to_string()),
        _ => {}
    }

    match kind {
        Kind::Pdf => from_pdf(path),
        Kind::ZippedXml => from_zipped_xml(path),
        Kind::Markup => match std::fs::read_to_string(path) {
            Ok(raw) => Extraction::Text(vec![capped(strip_markup(&raw))]),
            Err(e) => Extraction::Failed(e.to_string()),
        },
        Kind::Plain => match std::fs::read_to_string(path) {
            Ok(raw) => Extraction::Text(vec![capped(normalise(&raw))]),
            Err(e) => Extraction::Failed(e.to_string()),
        },
        Kind::None => Extraction::Unsupported,
    }
}

/// One entry per page, so a hit can say where it is.
///
/// A page that yields nothing still takes a slot: dropping empties would shift
/// every page after a scanned insert, and a page number that points at the
/// wrong page is worse than no page number.
fn from_pdf(path: &Path) -> Extraction {
    let doc = match lopdf::Document::load(path) {
        Ok(doc) => doc,
        Err(e) => return Extraction::Failed(format!("cannot open PDF: {e}")),
    };

    let mut pages = Vec::new();
    let mut budget = MAX_TEXT;
    for number in doc.get_pages().keys() {
        if budget == 0 {
            break;
        }
        // A single malformed page should cost that page, not the document.
        let text = doc
            .extract_text(&[*number])
            .map(|t| normalise(&t))
            .unwrap_or_default();
        let text = if text.len() > budget {
            truncate_on_boundary(&text, budget)
        } else {
            text
        };
        budget = budget.saturating_sub(text.len());
        pages.push(text);
    }

    if pages.is_empty() {
        // Loaded, but no pages came back. Almost always an image-only scan,
        // which needs OCR rather than a retry.
        return Extraction::Unsupported;
    }
    Extraction::Text(pages)
}

/// docx, pptx, xlsx, epub: a zip of XML with the words between the tags.
fn from_zipped_xml(path: &Path) -> Extraction {
    let file = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(e) => return Extraction::Failed(e.to_string()),
    };
    let mut archive = match zip::ZipArchive::new(file) {
        Ok(a) => a,
        Err(e) => return Extraction::Failed(format!("not a readable archive: {e}")),
    };

    let mut collected = String::new();
    // Sorted so a document reads in its own order rather than in whatever order
    // the archive happens to store its parts.
    let mut names: Vec<String> = (0..archive.len())
        .filter_map(|i| archive.by_index(i).ok().map(|f| f.name().to_string()))
        .filter(|name| carries_text(name))
        .collect();
    names.sort();

    for name in names {
        if collected.len() >= MAX_TEXT {
            break;
        }
        let Ok(mut entry) = archive.by_name(&name) else {
            continue;
        };
        let mut raw = String::new();
        use std::io::Read;
        if entry.read_to_string(&mut raw).is_err() {
            continue;
        }
        let text = strip_markup(&raw);
        if text.is_empty() {
            continue;
        }
        if !collected.is_empty() {
            collected.push(' ');
        }
        collected.push_str(&text);
    }

    if collected.is_empty() {
        return Extraction::Unsupported;
    }
    Extraction::Text(vec![capped(collected)])
}

/// Which parts of an office archive hold prose.
///
/// Named rather than "everything that ends in .xml" because the rest of the
/// archive is styling, relationships and settings — thousands of words of
/// machine noise that would swamp the ranking of the actual document.
fn carries_text(name: &str) -> bool {
    let lower = name.to_lowercase();
    if lower.contains("/_rels/") || lower.ends_with(".rels") {
        return false;
    }
    // Word, PowerPoint, Excel, OpenDocument, EPUB, in that order.
    lower == "word/document.xml"
        || lower.starts_with("word/footnotes")
        || lower.starts_with("word/endnotes")
        || (lower.starts_with("ppt/slides/") && lower.ends_with(".xml"))
        || (lower.starts_with("ppt/notesslides/") && lower.ends_with(".xml"))
        || lower == "xl/sharedstrings.xml"
        || lower == "content.xml"
        || lower.ends_with(".xhtml")
        || lower.ends_with(".html")
}

/// Words with the markup taken out.
///
/// Every tag becomes a space, which is what keeps `<t>Hello</t><t>World</t>`
/// from reading as `HelloWorld`. Script and style bodies go first: they are
/// not prose, and a page's jQuery would otherwise be the bulk of what gets
/// indexed.
pub fn strip_markup(raw: &str) -> String {
    static SCRIPTS: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    static TAGS: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();

    let scripts = SCRIPTS.get_or_init(|| {
        regex::Regex::new(r"(?is)<(script|style)\b[^>]*>.*?</\s*(script|style)\s*>").unwrap()
    });
    let tags = TAGS.get_or_init(|| regex::Regex::new(r"(?s)<[^>]*>").unwrap());

    let without_scripts = scripts.replace_all(raw, " ");
    let without_tags = tags.replace_all(&without_scripts, " ");
    normalise(&decode_entities(&without_tags))
}

/// The five entities XML defines, plus numeric escapes.
fn decode_entities(raw: &str) -> String {
    static NUMERIC: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let numeric = NUMERIC.get_or_init(|| regex::Regex::new(r"&#(x?)([0-9A-Fa-f]+);").unwrap());

    let expanded = numeric.replace_all(raw, |caps: &regex::Captures| {
        let radix = if caps[1].is_empty() { 10 } else { 16 };
        u32::from_str_radix(&caps[2], radix)
            .ok()
            .and_then(char::from_u32)
            .map(String::from)
            .unwrap_or_default()
    });

    expanded
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        // Last, or an `&amp;lt;` would decode twice.
        .replace("&amp;", "&")
}

/// Collapse whitespace. FTS5 tokenises on it, so runs of it are pure size.
fn normalise(raw: &str) -> String {
    raw.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn capped(text: String) -> String {
    if text.len() <= MAX_TEXT {
        return text;
    }
    truncate_on_boundary(&text, MAX_TEXT)
}

/// Cut to at most `limit` bytes without splitting a character in half.
fn truncate_on_boundary(text: &str, limit: usize) -> String {
    if text.len() <= limit {
        return text.to_string();
    }
    let mut end = limit;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    text[..end].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Which formats are even attempted ──────────────────────

    #[test]
    fn formats_are_routed_by_extension() {
        assert_eq!(kind_of("PDF"), Kind::Pdf);
        assert_eq!(kind_of("docx"), Kind::ZippedXml);
        assert_eq!(kind_of("epub"), Kind::ZippedXml);
        assert_eq!(kind_of("html"), Kind::Markup);
        assert_eq!(kind_of("md"), Kind::Plain);
        assert_eq!(kind_of("rs"), Kind::Plain);
    }

    /// An image has no words, and saying so is an answer — it stops the file
    /// being reopened on every pass for the rest of its life.
    #[test]
    fn a_format_with_no_text_is_answered_not_retried() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("anh.jpg");
        std::fs::write(&path, [0xFF, 0xD8, 0xFF]).unwrap();

        assert_eq!(extract(&path, "jpg"), Extraction::Unsupported);
    }

    #[test]
    fn a_plain_file_gives_up_its_words() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ghi-chu.md");
        std::fs::write(&path, "# Hợp đồng\n\nGiá trị:  120 triệu\n").unwrap();

        let Extraction::Text(pages) = extract(&path, "md") else {
            panic!("markdown must extract");
        };
        assert_eq!(pages, vec!["# Hợp đồng Giá trị: 120 triệu"]);
    }

    // ── Markup ────────────────────────────────────────────────

    /// Two adjacent runs of text are two words. Office XML splits a sentence
    /// across `<w:t>` elements wherever formatting changes, so a tag that
    /// vanished without leaving a space would glue words together and make the
    /// sentence unsearchable.
    #[test]
    fn adjacent_runs_do_not_run_together() {
        assert_eq!(strip_markup("<w:t>Hợp</w:t><w:t>đồng</w:t>"), "Hợp đồng");
    }

    #[test]
    fn entities_come_back_as_characters() {
        assert_eq!(
            strip_markup("<p>Ti&#7873;n &amp; b&#x1EA1;c &lt;quan tr&#7885;ng&gt;</p>"),
            "Tiền & bạc <quan trọng>"
        );
    }

    /// A page's scripts are not its prose. Left in, a site's bundled JavaScript
    /// is most of what gets indexed and swamps the ranking of every real word
    /// on the page.
    #[test]
    fn scripts_and_styles_are_not_indexed_as_prose() {
        let html = "<html><head><style>body{color:red}</style>\
                    <script>var x = 'khong phai noi dung';</script></head>\
                    <body><p>Nội dung thật</p></body></html>";
        assert_eq!(strip_markup(html), "Nội dung thật");
    }

    // ── Zipped XML ────────────────────────────────────────────

    fn write_docx(path: &Path, body: &str) {
        use std::io::Write;
        let file = std::fs::File::create(path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let options: zip::write::FileOptions<'_, ()> = zip::write::FileOptions::default();

        zip.start_file("word/document.xml", options).unwrap();
        zip.write_all(body.as_bytes()).unwrap();
        // Styling and relationships, which must not be indexed.
        zip.start_file("word/styles.xml", options).unwrap();
        zip.write_all(b"<styles><name val='KhongPhaiNoiDung'/></styles>")
            .unwrap();
        zip.start_file("word/_rels/document.xml.rels", options)
            .unwrap();
        zip.write_all(b"<Relationships><Relationship Target='khongphai'/></Relationships>")
            .unwrap();
        zip.finish().unwrap();
    }

    #[test]
    fn a_docx_gives_up_the_words_in_its_body() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("hop-dong.docx");
        write_docx(
            &path,
            "<w:document><w:body><w:p><w:r><w:t>Điều khoản thanh toán</w:t></w:r></w:p></w:body></w:document>",
        );

        let Extraction::Text(parts) = extract(&path, "docx") else {
            panic!("a docx must extract");
        };
        assert_eq!(parts, vec!["Điều khoản thanh toán"]);
    }

    /// The rest of an office archive is settings, styles and relationships —
    /// thousands of machine words that would outweigh the document itself.
    #[test]
    fn the_machinery_around_a_document_is_not_indexed() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("hop-dong.docx");
        write_docx(&path, "<w:t>Chỉ nội dung này</w:t>");

        let Extraction::Text(parts) = extract(&path, "docx") else {
            panic!("a docx must extract");
        };
        assert!(!parts[0].contains("KhongPhaiNoiDung"));
        assert!(!parts[0].contains("khongphai"));
    }

    #[test]
    fn a_file_that_is_not_really_an_archive_fails_rather_than_panicking() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("hong.docx");
        std::fs::write(&path, "day khong phai zip").unwrap();

        assert!(matches!(extract(&path, "docx"), Extraction::Failed(_)));
    }

    // ── Limits ────────────────────────────────────────────────

    /// A search index is for finding a document, not holding it.
    #[test]
    fn a_wall_of_text_is_capped() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("khong-lo.txt");
        std::fs::write(&path, "x ".repeat(MAX_TEXT)).unwrap();

        let Extraction::Text(parts) = extract(&path, "txt") else {
            panic!("must extract");
        };
        assert!(parts[0].len() <= MAX_TEXT);
    }

    /// Cutting mid-character would produce invalid UTF-8, which in a Vietnamese
    /// vault is the common case rather than the exotic one.
    #[test]
    fn a_cut_never_lands_inside_a_character() {
        let text = "đđđđđđđđđđ";
        for limit in 0..text.len() {
            let cut = truncate_on_boundary(text, limit);
            assert!(text.starts_with(&cut));
        }
    }
}
