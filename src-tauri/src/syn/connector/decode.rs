//! What an answer is, before `pick` reads it.
//!
//! The decoder axis of `docs/syn-connectors-2026-10-08.md` §3.1: every format
//! is turned into the same JSON shape `pick` already reads — a list of records
//! — so `each`, `pick`, `zip` and `explode` work on a CSV, a calendar or a feed
//! exactly as on JSON. Nothing here runs anything the answer says; a format
//! that could (HTML's scripts, XML's entities) is read as text.
//!
//! - `csv`: rows by the header row; the delimiter is the one the header uses
//!   (`,` `;` or tab — Vietnamese bank statements use `;`).
//! - `xml`: elements by local name (`D:response` and `d:response` are one),
//!   attributes as `@name`, text as `#text` or as the value when that is all.
//! - `ics` / `vcard`: one record per `VEVENT`/`VTODO` or `VCARD`; dates as ISO.
//!   A WebDAV `multistatus` (CalDAV `REPORT`, CardDAV) is read for the
//!   `calendar-data` / `address-data` in it, each record with its `_href` and
//!   `_etag`.
//! - `rss`: Atom, RSS and JSON Feed, by `feed-rs`, one record per entry.
//! - `text` / `html`: `{ "text": … }`, tags removed from HTML.

use serde_json::{json, Map, Value};

use super::manifest::Decoder;

/// Read `body` as `decoder` says.
pub fn decode(decoder: Decoder, body: &[u8]) -> Result<Value, String> {
    let text = || String::from_utf8_lossy(body).trim_start_matches('\u{feff}').to_string();
    match decoder {
        Decoder::Json => serde_json::from_slice(body).map_err(|_| {
            format!("did not answer with JSON: {}", text().chars().take(300).collect::<String>())
        }),
        Decoder::Csv => Ok(csv(&text())),
        Decoder::Xml => xml(&text()),
        Decoder::Ics => Ok(Value::Array(cards(&dav_or_raw(&text(), "calendar-data")?, &["VEVENT", "VTODO"]))),
        Decoder::Vcard => Ok(Value::Array(cards(&dav_or_raw(&text(), "address-data")?, &["VCARD"]))),
        Decoder::Rss => rss(body),
        Decoder::Text => Ok(json!({ "text": text() })),
        Decoder::Html => Ok(json!({ "text": html_text(&text()) })),
    }
}

// ── CSV ─────────────────────────────────────────────────────────

fn split_csv(text: &str, delimiter: char) -> Vec<Vec<String>> {
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' if quoted && chars.peek() == Some(&'"') => {
                field.push('"');
                chars.next();
            }
            '"' if quoted => quoted = false,
            '"' if field.is_empty() => quoted = true,
            c if c == delimiter && !quoted => row.push(std::mem::take(&mut field)),
            '\r' if !quoted => {}
            '\n' if !quoted => {
                row.push(std::mem::take(&mut field));
                if row.iter().any(|f| !f.trim().is_empty()) {
                    rows.push(std::mem::take(&mut row));
                } else {
                    row.clear();
                }
            }
            c => field.push(c),
        }
    }
    row.push(field);
    if row.iter().any(|f| !f.trim().is_empty()) {
        rows.push(row);
    }
    rows
}

/// Rows of a CSV as records by its header. A header cell that is empty or
/// repeated gets its position, so no column is lost.
pub fn csv(text: &str) -> Value {
    let first = text.lines().next().unwrap_or_default();
    let delimiter = [',', ';', '\t'].into_iter().max_by_key(|d| first.matches(*d).count()).unwrap_or(',');
    let rows = split_csv(text, delimiter);
    let Some((header, rest)) = rows.split_first() else { return json!([]) };
    let mut names: Vec<String> = Vec::new();
    for (i, h) in header.iter().enumerate() {
        let h = h.trim();
        let name = if h.is_empty() || names.iter().any(|n| n == h) { format!("column_{}", i + 1) } else { h.to_string() };
        names.push(name);
    }
    Value::Array(
        rest.iter()
            .map(|r| {
                let mut o = Map::new();
                for (i, name) in names.iter().enumerate() {
                    o.insert(name.clone(), Value::String(r.get(i).map(|s| s.trim().to_string()).unwrap_or_default()));
                }
                Value::Object(o)
            })
            .collect(),
    )
}

// ── XML ─────────────────────────────────────────────────────────

fn local(name: &[u8]) -> String {
    let name = String::from_utf8_lossy(name);
    name.rsplit(':').next().unwrap_or_default().to_string()
}

fn put(o: &mut Map<String, Value>, key: String, v: Value) {
    match o.get_mut(&key) {
        Some(Value::Array(a)) => a.push(v),
        Some(existing) => {
            let first = existing.take();
            *existing = Value::Array(vec![first, v]);
        }
        None => {
            o.insert(key, v);
        }
    }
}

/// An element with only text is its text; otherwise an object.
fn settle(o: Map<String, Value>, text: String) -> Value {
    let text = text.trim().to_string();
    if o.is_empty() {
        return Value::String(text);
    }
    let mut o = o;
    if !text.is_empty() {
        o.insert("#text".into(), Value::String(text));
    }
    Value::Object(o)
}

/// XML as JSON, by local names. Entities other than the five XML defines are
/// not expanded, and no DTD is read.
pub fn xml(text: &str) -> Result<Value, String> {
    use quick_xml::events::Event;
    let mut reader = quick_xml::Reader::from_str(text);
    reader.config_mut().trim_text(false);
    // (name, children, text) for each open element.
    let mut stack: Vec<(String, Map<String, Value>, String)> = vec![(String::new(), Map::new(), String::new())];
    loop {
        match reader.read_event().map_err(|e| format!("not readable XML: {e}"))? {
            Event::Start(e) => {
                let mut attrs = Map::new();
                for a in e.attributes().flatten() {
                    let v = a.decode_and_unescape_value(reader.decoder()).map(|v| v.into_owned()).unwrap_or_default();
                    attrs.insert(format!("@{}", local(a.key.as_ref())), Value::String(v));
                }
                stack.push((local(e.name().as_ref()), attrs, String::new()));
            }
            Event::Empty(e) => {
                let mut attrs = Map::new();
                for a in e.attributes().flatten() {
                    let v = a.decode_and_unescape_value(reader.decoder()).map(|v| v.into_owned()).unwrap_or_default();
                    attrs.insert(format!("@{}", local(a.key.as_ref())), Value::String(v));
                }
                let top = stack.last_mut().expect("root");
                put(&mut top.1, local(e.name().as_ref()), settle(attrs, String::new()));
            }
            Event::Text(t) => {
                let s = t.unescape().map(|s| s.into_owned()).unwrap_or_else(|_| String::from_utf8_lossy(&t).into_owned());
                stack.last_mut().expect("root").2.push_str(&s);
            }
            Event::CData(t) => stack.last_mut().expect("root").2.push_str(&String::from_utf8_lossy(&t)),
            Event::End(_) => {
                let (name, o, text) = stack.pop().ok_or("an end with no start")?;
                let parent = stack.last_mut().ok_or("an end with no start")?;
                put(&mut parent.1, name, settle(o, text));
            }
            Event::Eof => break,
            _ => {}
        }
    }
    let (_, root, _) = stack.into_iter().next().ok_or("empty")?;
    Ok(Value::Object(root))
}

// ── iCalendar and vCard ─────────────────────────────────────────

/// The `calendar-data` (or `address-data`) of each response in a WebDAV
/// multistatus, with its address and etag; or the text itself when it is not
/// one.
fn dav_or_raw(text: &str, data: &str) -> Result<Vec<(String, Option<String>, Option<String>)>, String> {
    if !text.trim_start().starts_with('<') {
        return Ok(vec![(text.to_string(), None, None)]);
    }
    let doc = xml(text)?;
    let responses = doc.get("multistatus").and_then(|m| m.get("response")).cloned().unwrap_or(Value::Null);
    let responses = match responses {
        Value::Array(a) => a,
        Value::Null => Vec::new(),
        one => vec![one],
    };
    let mut out = Vec::new();
    for r in responses {
        let href = r.get("href").and_then(Value::as_str).map(str::to_string);
        let propstats = match r.get("propstat").cloned() {
            Some(Value::Array(a)) => a,
            Some(one) => vec![one],
            None => Vec::new(),
        };
        for ps in propstats {
            let Some(prop) = ps.get("prop") else { continue };
            let etag = prop.get("getetag").and_then(Value::as_str).map(|s| s.trim_matches('"').to_string());
            let body = match prop.get(data) {
                Some(Value::String(s)) => s.clone(),
                Some(Value::Object(o)) => o.get("#text").and_then(Value::as_str).unwrap_or_default().to_string(),
                _ => continue,
            };
            if !body.trim().is_empty() {
                out.push((body, href.clone(), etag));
            }
        }
    }
    Ok(out)
}

/// Lines put back together: a line starting with a space or tab continues
/// the one before (RFC 5545 §3.1).
fn unfold(text: &str) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    for raw in text.split('\n') {
        let line = raw.strip_suffix('\r').unwrap_or(raw);
        if let Some(rest) = line.strip_prefix([' ', '\t']) {
            if let Some(last) = lines.last_mut() {
                last.push_str(rest);
                continue;
            }
        }
        lines.push(line.to_string());
    }
    lines
}

fn unescape(v: &str) -> String {
    let mut out = String::new();
    let mut chars = v.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n') | Some('N') => out.push('\n'),
                Some(o) => out.push(o),
                None => {}
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// `20260930T090000Z` → `2026-09-30T09:00:00Z`; `20260930` → `2026-09-30`.
fn iso(v: &str) -> Option<String> {
    let v = v.trim();
    let digits = |s: &str| s.chars().all(|c| c.is_ascii_digit());
    match v.len() {
        8 if digits(v) => Some(format!("{}-{}-{}", &v[..4], &v[4..6], &v[6..8])),
        15 | 16 if digits(&v[..8]) && &v[8..9] == "T" && digits(&v[9..15]) => Some(format!(
            "{}-{}-{}T{}:{}:{}{}",
            &v[..4],
            &v[4..6],
            &v[6..8],
            &v[9..11],
            &v[11..13],
            &v[13..15],
            if v.ends_with('Z') { "Z" } else { "" }
        )),
        _ => None,
    }
}

const DATES: &[&str] = &["dtstart", "dtend", "due", "dtstamp", "created", "last-modified", "recurrence-id", "completed", "bday", "rev"];
/// Properties that may come more than once and are kept as a list.
const MANY: &[&str] = &["attendee", "email", "tel", "adr", "categories", "exdate", "url", "impp", "related"];

/// One record per component named in `kinds`, from each text.
fn cards(texts: &[(String, Option<String>, Option<String>)], kinds: &[&str]) -> Vec<Value> {
    let mut out = Vec::new();
    for (text, href, etag) in texts {
        let mut depth: Vec<String> = Vec::new();
        let mut current: Option<Map<String, Value>> = None;
        for line in unfold(text) {
            let Some((head, value)) = line.split_once(':') else { continue };
            let mut parts = head.split(';');
            let name = parts.next().unwrap_or_default().trim().to_ascii_uppercase();
            // `item1.EMAIL` in a vCard is an EMAIL.
            let name = name.rsplit('.').next().unwrap_or_default().to_string();
            let params: Vec<(String, String)> = parts
                .filter_map(|p| p.split_once('=').map(|(k, v)| (k.to_ascii_lowercase(), v.trim_matches('"').to_string())))
                .collect();
            match name.as_str() {
                "BEGIN" => {
                    let kind = value.trim().to_ascii_uppercase();
                    if kinds.contains(&kind.as_str()) && current.is_none() {
                        let mut o = Map::new();
                        o.insert("_kind".into(), json!(kind.to_ascii_lowercase()));
                        if let Some(h) = href {
                            o.insert("_href".into(), json!(h));
                        }
                        if let Some(e) = etag {
                            o.insert("_etag".into(), json!(e));
                        }
                        current = Some(o);
                    }
                    depth.push(kind);
                }
                "END" => {
                    let kind = value.trim().to_ascii_uppercase();
                    depth.pop();
                    if kinds.contains(&kind.as_str()) && !depth.iter().any(|d| kinds.contains(&d.as_str())) {
                        if let Some(o) = current.take() {
                            out.push(Value::Object(o));
                        }
                    }
                }
                _ => {
                    // Only the component's own lines: an alarm inside an event is not the event.
                    let inner = depth.last().is_some_and(|d| kinds.contains(&d.as_str()));
                    let Some(o) = current.as_mut().filter(|_| inner) else { continue };
                    let key = name.to_ascii_lowercase();
                    let text = unescape(value);
                    let v = if DATES.contains(&key.as_str()) {
                        if let Some(tz) = params.iter().find(|(k, _)| k == "tzid") {
                            o.insert(format!("{key}_tz"), json!(tz.1));
                        }
                        json!(iso(&text).unwrap_or(text))
                    } else if key == "attendee" || key == "organizer" {
                        json!(text.trim_start_matches("mailto:").trim_start_matches("MAILTO:"))
                    } else {
                        json!(text)
                    };
                    if MANY.contains(&key.as_str()) {
                        match o.entry(key).or_insert_with(|| json!([])) {
                            Value::Array(a) => a.push(v),
                            other => *other = json!([other.take(), v]),
                        }
                    } else {
                        o.entry(key).or_insert(v);
                    }
                }
            }
        }
    }
    out
}

// ── Feeds, text ─────────────────────────────────────────────────

fn rss(body: &[u8]) -> Result<Value, String> {
    let feed = feed_rs::parser::parse(body).map_err(|e| format!("not a feed: {e}"))?;
    Ok(Value::Array(
        feed.entries
            .into_iter()
            .map(|e| {
                json!({
                    "id": e.id,
                    "title": e.title.map(|t| t.content).unwrap_or_default(),
                    "link": e.links.first().map(|l| l.href.clone()).unwrap_or_default(),
                    "published": e.published.map(|d| d.to_rfc3339()),
                    "updated": e.updated.map(|d| d.to_rfc3339()),
                    "author": e.authors.first().map(|a| a.name.clone()),
                    "summary": e.summary.map(|s| s.content).or_else(|| e.content.and_then(|c| c.body)).unwrap_or_default(),
                })
            })
            .collect(),
    ))
}

/// The words of a page: scripts and styles gone, tags gone, whitespace kept short.
fn html_text(html: &str) -> String {
    let no_code = regex::Regex::new(r"(?is)<(script|style|noscript)\b.*?</(script|style|noscript)\s*>").expect("pattern").replace_all(html, " ");
    let no_tags = regex::Regex::new(r"(?s)<[^>]*>").expect("pattern").replace_all(&no_code, " ");
    let text = no_tags.replace("&nbsp;", " ").replace("&amp;", "&").replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&#39;", "'");
    regex::Regex::new(r"\s+").expect("pattern").replace_all(&text, " ").trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bank_statement_csv_reads_by_its_header() {
        let text = "\u{feff}Ngày;Mô tả;Số tiền\r\n01/09/2026;\"Highlands; Q1\";-55.000\r\n02/09/2026;Lương;25.000.000\r\n\r\n";
        let v = decode(Decoder::Csv, text.as_bytes()).expect("reads");
        assert_eq!(v[0]["Mô tả"], "Highlands; Q1", "a quoted delimiter stays in its field");
        assert_eq!(v[1]["Số tiền"], "25.000.000");
        assert_eq!(v.as_array().map(Vec::len), Some(2));
        let quoted = csv("a,b\n\"say \"\"hi\"\"\",\"x\ny\"\n");
        assert_eq!(quoted[0]["a"], "say \"hi\"");
        assert_eq!(quoted[0]["b"], "x\ny");
    }

    #[test]
    fn xml_reads_by_local_names() {
        let v = xml(r#"<D:multistatus xmlns:D="DAV:"><D:response><D:href>/a</D:href></D:response><D:response><D:href>/b</D:href><D:x id="7"/></D:response></D:multistatus>"#).expect("reads");
        assert_eq!(v["multistatus"]["response"][1]["href"], "/b");
        assert_eq!(v["multistatus"]["response"][1]["x"]["@id"], "7");
    }

    /// An entity declared in the document is not expanded: no billion laughs,
    /// no file read through an external entity.
    #[test]
    fn xml_does_not_expand_what_the_document_declares() {
        let v = xml(r#"<!DOCTYPE x [<!ENTITY e SYSTEM "file:///etc/passwd">]><x>&e;</x>"#);
        let shown = format!("{v:?}");
        assert!(!shown.contains("root:"), "{shown}");
    }

    const CAL: &str = "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nBEGIN:VEVENT\r\nUID:ev-1@example\r\nDTSTART;TZID=Asia/Ho_Chi_Minh:20261012T090000\r\nDTEND;TZID=Asia/Ho_Chi_Minh:20261012T100000\r\nSUMMARY:Họp team\\, sprint\r\nDESCRIPTION:Dòng một\\nDòng\r\n  hai\r\nATTENDEE;CN=Linh:mailto:linh@example.com\r\nATTENDEE:mailto:minh@example.com\r\nBEGIN:VALARM\r\nSUMMARY:Nhắc\r\nEND:VALARM\r\nEND:VEVENT\r\nBEGIN:VEVENT\r\nUID:ev-2\r\nDTSTART;VALUE=DATE:20261013\r\nSUMMARY:Nghỉ\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";

    #[test]
    fn a_calendar_reads_as_its_events() {
        let v = decode(Decoder::Ics, CAL.as_bytes()).expect("reads");
        let events = v.as_array().expect("list");
        assert_eq!(events.len(), 2);
        assert_eq!(events[0]["uid"], "ev-1@example");
        assert_eq!(events[0]["summary"], "Họp team, sprint");
        assert_eq!(events[0]["description"], "Dòng một\nDòng hai", "unfolded and unescaped");
        assert_eq!(events[0]["dtstart"], "2026-10-12T09:00:00");
        assert_eq!(events[0]["dtstart_tz"], "Asia/Ho_Chi_Minh");
        assert_eq!(events[0]["attendee"], json!(["linh@example.com", "minh@example.com"]));
        assert_eq!(events[1]["dtstart"], "2026-10-13");
    }

    /// A CalDAV REPORT answers a multistatus; each event keeps where it is.
    #[test]
    fn a_caldav_report_reads_as_its_events_with_their_address() {
        let ics = CAL.replace('&', "&amp;").replace('<', "&lt;");
        let body = format!(
            r#"<?xml version="1.0"?><d:multistatus xmlns:d="DAV:" xmlns:c="urn:ietf:params:xml:ns:caldav"><d:response><d:href>/cal/ev-1.ics</d:href><d:propstat><d:prop><d:getetag>"e1"</d:getetag><c:calendar-data>{ics}</c:calendar-data></d:prop><d:status>HTTP/1.1 200 OK</d:status></d:propstat></d:response></d:multistatus>"#
        );
        let v = decode(Decoder::Ics, body.as_bytes()).expect("reads");
        assert_eq!(v[0]["_href"], "/cal/ev-1.ics");
        assert_eq!(v[0]["_etag"], "e1");
        assert_eq!(v[1]["uid"], "ev-2");
    }

    #[test]
    fn contacts_read_as_cards() {
        let vcf = "BEGIN:VCARD\r\nVERSION:3.0\r\nUID:c-1\r\nFN:Nguyễn Linh\r\nitem1.EMAIL;TYPE=work:linh@example.com\r\nTEL;TYPE=cell:+84 90 000 0000\r\nBDAY:19900115\r\nEND:VCARD\r\n";
        let v = decode(Decoder::Vcard, vcf.as_bytes()).expect("reads");
        assert_eq!(v[0]["fn"], "Nguyễn Linh");
        assert_eq!(v[0]["email"], json!(["linh@example.com"]));
        assert_eq!(v[0]["bday"], "1990-01-15");
    }

    #[test]
    fn a_feed_reads_as_its_entries() {
        let rss = r#"<?xml version="1.0"?><rss version="2.0"><channel><title>T</title><item><title>Bài 1</title><link>https://a.example/1</link><guid>1</guid></item></channel></rss>"#;
        let v = decode(Decoder::Rss, rss.as_bytes()).expect("reads");
        assert_eq!(v[0]["title"], "Bài 1");
        assert_eq!(v[0]["link"], "https://a.example/1");
    }

    #[test]
    fn a_page_is_its_words() {
        let v = decode(Decoder::Html, b"<html><script>alert(1)</script><p>Xin&nbsp;ch\xc3\xa0o</p></html>").expect("reads");
        assert_eq!(v["text"], "Xin chào");
    }
}
