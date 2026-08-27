//! The line grammar iCalendar and vCard share.
//!
//! Both formats are built on the same shape — `NAME;PARAM=value:the value`,
//! folded at 75 octets, with `\,` `\;` `\\` `\n` escapes — because both come
//! from the same directory-services family (RFC 2425, restated by RFC 5545 for
//! calendars and RFC 6350 for cards). Parsing one is parsing the other.
//!
//! This module knows nothing about events or people. It turns bytes into
//! `(name, params, value)` and back, and leaves the meaning to the caller.
//!
//! # What is deliberately not here
//!
//! **`QUOTED-PRINTABLE`.** Only vCard 2.1 uses it, and it brings its own
//! continuation rule that contradicts the folding below — a line ending in `=`
//! continues on the next line with no leading space. Decoding it in here would
//! make every calendar pay for a shape no calendar has. See
//! `crate::people::vcard`.

const LINE_LIMIT: usize = 75;

/// A property name, its parameters, and its value.
pub type Line = (String, Vec<(String, String)>, String);

// ─── Writing ────────────────────────────────────────────────

/// Escape a text value. Order matters: backslashes first, or the escapes
/// this adds would be escaped again.
pub fn escape(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace(';', "\\;")
        .replace(',', "\\,")
        .replace("\r\n", "\\n")
        .replace(['\n', '\r'], "\\n")
}

/// Break a line at 75 octets, continuing with a leading space.
///
/// Counted in octets, not characters: a line split through the middle of a
/// multi-byte character is not valid UTF-8 on the other side, which for a
/// file full of accented names is most of them.
pub fn fold(line: &str, out: &mut String) {
    let bytes = line.as_bytes();
    if bytes.len() <= LINE_LIMIT {
        out.push_str(line);
        out.push_str("\r\n");
        return;
    }

    let mut start = 0;
    let mut limit = LINE_LIMIT;
    while start < bytes.len() {
        let mut end = (start + limit).min(bytes.len());
        while end > start && !line.is_char_boundary(end) {
            end -= 1;
        }
        if end == start {
            break;
        }
        if start > 0 {
            out.push(' ');
        }
        out.push_str(&line[start..end]);
        out.push_str("\r\n");
        start = end;
        // A continuation line spends one octet on its leading space.
        limit = LINE_LIMIT - 1;
    }
}

/// Write `NAME:value`, escaped and folded. An empty value writes nothing.
pub fn prop(name: &str, value: &str, out: &mut String) {
    if value.is_empty() {
        return;
    }
    fold(&format!("{}:{}", name, escape(value)), out);
}

// ─── Reading ────────────────────────────────────────────────

pub fn unescape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut chars = value.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') | Some('N') => out.push('\n'),
            Some(other) => out.push(other),
            None => out.push('\\'),
        }
    }
    out
}

/// Join continuation lines back onto the line they belong to.
pub fn unfold(text: &str) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    for raw in text.split('\n') {
        let line = raw.strip_suffix('\r').unwrap_or(raw);
        if line.starts_with(' ') || line.starts_with('\t') {
            if let Some(last) = lines.last_mut() {
                last.push_str(&line[1..]);
                continue;
            }
        }
        lines.push(line.to_string());
    }
    lines
}

/// `DTSTART;TZID=Asia/Tokyo:20260310T090000` → name, params, value.
///
/// A parameter written without a name — `TEL;HOME;VOICE:...`, which is how
/// vCard 2.1 writes types — is recorded as a `TYPE`. That is what it means,
/// and the alternative is dropping it: every phone number exported by an
/// older device would arrive with no idea whether it was a mobile or a fax.
pub fn split_line(line: &str) -> Option<Line> {
    let mut in_quotes = false;
    let mut colon = None;
    for (i, c) in line.char_indices() {
        match c {
            '"' => in_quotes = !in_quotes,
            ':' if !in_quotes => {
                colon = Some(i);
                break;
            }
            _ => {}
        }
    }
    let colon = colon?;
    let (head, value) = line.split_at(colon);
    let value = &value[1..];

    let mut parts = head.split(';');
    let name = parts.next()?.trim().to_ascii_uppercase();
    let params = parts
        .filter_map(|p| {
            let p = p.trim();
            if p.is_empty() {
                return None;
            }
            match p.split_once('=') {
                Some((k, v)) => Some((
                    k.trim().to_ascii_uppercase(),
                    v.trim().trim_matches('"').to_string(),
                )),
                None => Some(("TYPE".to_string(), p.to_string())),
            }
        })
        .collect();
    Some((name, params, value.to_string()))
}

/// Split a structured value on its separators, then unescape each piece.
///
/// `ORG:Acme\, Ltd;Engineering` is two components, and the comma in the first
/// belongs to the company's name. Unescaping the whole value first and
/// splitting afterwards cannot tell those apart — it would read a company
/// called "Acme, Ltd" as two.
pub fn split_values(raw: &str, sep: char) -> Vec<String> {
    let mut out = Vec::new();
    let mut piece = String::new();
    let mut chars = raw.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            piece.push(c);
            if let Some(next) = chars.next() {
                piece.push(next);
            }
        } else if c == sep {
            out.push(unescape(&piece));
            piece.clear();
        } else {
            piece.push(c);
        }
    }
    out.push(unescape(&piece));
    out
}

/// The first parameter with this name.
pub fn param<'a>(params: &'a [(String, String)], key: &str) -> Option<&'a str> {
    params
        .iter()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.as_str())
}

/// Every parameter with this name, which is how a card carries more than one
/// type: `TYPE=work,voice` in one parameter, or `;WORK;VOICE` in several.
pub fn params_all<'a>(params: &'a [(String, String)], key: &str) -> Vec<&'a str> {
    params
        .iter()
        .filter(|(k, _)| k == key)
        .flat_map(|(_, v)| v.split(','))
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_value_survives_being_escaped_and_read_back() {
        for original in [
            "plain",
            "a, b; c",
            "back\\slash",
            "line\nbreak",
            "Nguyễn Văn A",
            "everything: \\ ; , \n done",
        ] {
            assert_eq!(unescape(&escape(original)), original.replace("\r\n", "\n"));
        }
    }

    #[test]
    fn a_long_line_is_folded_and_unfolds_to_itself() {
        let value = "x".repeat(300);
        let line = format!("NOTE:{}", value);
        let mut out = String::new();
        fold(&line, &mut out);

        assert!(out.lines().all(|l| l.trim_end().len() <= 75), "each line fits");
        assert_eq!(unfold(&out).first().map(String::as_str), Some(line.as_str()));
    }

    #[test]
    fn folding_never_splits_a_character_in_half() {
        // A line of accented characters is the case that breaks a naive
        // byte-count split, and a name file is mostly those.
        let line = format!("FN:{}", "é".repeat(80));
        let mut out = String::new();
        fold(&line, &mut out);
        // Round-tripping through String proves every piece was valid UTF-8.
        assert_eq!(unfold(&out).first().map(String::as_str), Some(line.as_str()));
    }

    #[test]
    fn a_tab_continues_a_line_too() {
        assert_eq!(unfold("NOTE:one\r\n\ttwo"), vec!["NOTE:onetwo"]);
    }

    #[test]
    fn a_line_splits_into_name_params_and_value() {
        let (name, params, value) = split_line("DTSTART;TZID=Asia/Tokyo:20260310T090000").unwrap();
        assert_eq!(name, "DTSTART");
        assert_eq!(param(&params, "TZID"), Some("Asia/Tokyo"));
        assert_eq!(value, "20260310T090000");
    }

    #[test]
    fn a_colon_inside_quotes_is_not_the_separator() {
        let (name, params, value) = split_line(r#"ATTENDEE;CN="Smith:Jane":mailto:j@example.com"#).unwrap();
        assert_eq!(name, "ATTENDEE");
        assert_eq!(param(&params, "CN"), Some("Smith:Jane"));
        assert_eq!(value, "mailto:j@example.com");
    }

    #[test]
    fn a_parameter_with_no_name_is_a_type() {
        // vCard 2.1 writes `TEL;HOME;VOICE:...`. Dropping those left every
        // number from an older phone with no label at all.
        let (name, params, value) = split_line("TEL;HOME;VOICE:+84 90 000 0000").unwrap();
        assert_eq!(name, "TEL");
        assert_eq!(params_all(&params, "TYPE"), ["HOME", "VOICE"]);
        assert_eq!(value, "+84 90 000 0000");
    }

    #[test]
    fn one_parameter_may_carry_several_types() {
        // vCard 4.0 writes them as one comma-separated, often quoted, value.
        let (_, params, _) = split_line(r#"TEL;TYPE="work,voice":+84 90 000 0000"#).unwrap();
        assert_eq!(params_all(&params, "TYPE"), ["work", "voice"]);
    }

    #[test]
    fn a_separator_inside_a_value_is_not_a_separator() {
        // The comma belongs to the company's name, not to the grammar.
        assert_eq!(
            split_values(r"Acme\, Ltd;Engineering", ';'),
            ["Acme, Ltd", "Engineering"]
        );
        assert_eq!(split_values(r"work\,home,friend", ','), ["work,home", "friend"]);
        assert_eq!(split_values("", ';'), [""]);
        assert_eq!(split_values(";;street;city", ';'), ["", "", "street", "city"]);
    }

    #[test]
    fn a_line_with_no_colon_is_not_a_line() {
        assert!(split_line("BEGIN").is_none());
        assert!(split_line("").is_none());
    }
}
