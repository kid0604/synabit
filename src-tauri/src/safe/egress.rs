//! The one place a secret's value meets something Syn asked for.
//!
//! Syn never holds a value. It writes a placeholder — `{{safe:github-token}}`,
//! or `{{safe:github-token.password}}` for one field — in the arguments of a
//! tool that reaches outside, and this module swaps it for the value on a
//! **copy** of those arguments, just before they leave the process. The
//! original arguments, which the conversation records and the model sees
//! again, keep the placeholder.
//!
//! # What decides, and what does not
//!
//! The item decides where it may go (`AiPolicy::destinations`), and this
//! module checks it. The model is not consulted: a page that talked Syn into
//! sending `{{safe:github-token}}` to a connector the item was not given to
//! gets the placeholder sent as text, and an error back. This is section 8.4 of
//! the design, and the reason the design exists.
//!
//! # What comes back
//!
//! A service may echo what it was sent — an error that quotes the request, a
//! debug field, a token in a URL. [`Injected::scrub`] replaces every value this
//! call put in, in every encoding it could come back in, before the result goes
//! anywhere the model or the log can see it; and reports that it did, because a
//! service that returns your token is worth knowing about.

use std::collections::BTreeMap;

use serde_json::Value;

use super::item::SecretString;

/// Where a value is about to go.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Destination {
    /// One of Syn's connectors, by id.
    Connector(String),
    /// An HTTPS host, exactly.
    Host(String),
}

impl Destination {
    /// The form an item's `destinations` list stores.
    pub fn key(&self) -> String {
        match self {
            Destination::Connector(id) => format!("connector:{id}"),
            Destination::Host(h) => format!("host:{}", h.to_ascii_lowercase()),
        }
    }
}

/// A placeholder found in the arguments: `{{safe:<handle>}}` or
/// `{{safe:<handle>.<field label>}}`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Placeholder {
    pub handle: String,
    pub field: Option<String>,
}

impl Placeholder {
    pub fn render(&self) -> String {
        match &self.field {
            Some(f) => format!("{{{{safe:{}.{}}}}}", self.handle, f),
            None => format!("{{{{safe:{}}}}}", self.handle),
        }
    }
}

/// Handles are lower-case letters, digits and dashes — what `safe_request`
/// suggests and the editor enforces. A field label after a dot may be any
/// text without braces.
fn placeholders_in(text: &str) -> Vec<(std::ops::Range<usize>, Placeholder)> {
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(start) = text[from..].find("{{safe:").map(|i| i + from) {
        let rest = &text[start + 7..];
        let Some(end) = rest.find("}}") else { break };
        let inner = &rest[..end];
        let (handle, field) = match inner.split_once('.') {
            Some((h, f)) => (h, Some(f.trim().to_string()).filter(|f| !f.is_empty())),
            None => (inner, None),
        };
        let valid = !handle.is_empty() && handle.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
        if valid && !inner.contains('{') {
            out.push((start..start + 7 + end + 2, Placeholder { handle: handle.to_string(), field }));
        }
        from = start + 7;
    }
    out
}

/// Every placeholder anywhere in `args`, strings nested at any depth included.
pub fn find(args: &Value) -> Vec<Placeholder> {
    let mut found = Vec::new();
    walk(args, &mut |s| found.extend(placeholders_in(s).into_iter().map(|(_, p)| p)));
    found.sort();
    found.dedup();
    found
}

fn walk(v: &Value, f: &mut impl FnMut(&str)) {
    match v {
        Value::String(s) => f(s),
        Value::Array(a) => a.iter().for_each(|x| walk(x, f)),
        Value::Object(o) => o.iter().for_each(|(k, x)| {
            f(k);
            walk(x, f)
        }),
        _ => {}
    }
}

fn walk_mut(v: &mut Value, f: &mut impl FnMut(&mut String)) {
    match v {
        Value::String(s) => f(s),
        Value::Array(a) => a.iter_mut().for_each(|x| walk_mut(x, f)),
        Value::Object(o) => o.values_mut().for_each(|x| walk_mut(x, f)),
        _ => {}
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum EgressError {
    /// The Safe is locked; nothing can be looked up.
    #[error("the Safe is locked")]
    Locked,
    /// No item Syn may use has this handle — it does not exist, is hidden from
    /// Syn, or is only listed. Deliberately one answer for all three.
    #[error("there is no Safe item “{0}” that Syn may use")]
    Unknown(String),
    #[error("“{handle}” may not be sent to {destination}")]
    NotAllowed { handle: String, destination: String },
    #[error("“{0}” has no field by that name")]
    NoSuchField(String),
}

/// The values one call put into its arguments, kept only to scrub its result.
/// Dropping it wipes them.
#[derive(Default, Clone)]
pub struct Injected {
    values: Vec<(Placeholder, SecretString)>,
}

/// How a value is found, given a placeholder and where it is going: from the
/// open Safe in the app, from a table in the tests.
pub trait Lookup {
    fn value(&self, placeholder: &Placeholder, destination: &Destination) -> Result<SecretString, EgressError>;
}

/// A copy of `args` with every placeholder replaced by its value — or the
/// first reason one could not be. All or nothing: a call is never sent with
/// some placeholders filled and others left as text.
pub fn fill(args: &Value, destination: &Destination, lookup: &dyn Lookup) -> Result<(Value, Injected), EgressError> {
    let mut values: BTreeMap<Placeholder, SecretString> = BTreeMap::new();
    for p in find(args) {
        let v = lookup.value(&p, destination)?;
        values.insert(p, v);
    }
    let mut filled = args.clone();
    if !values.is_empty() {
        walk_mut(&mut filled, &mut |s| {
            let spans = placeholders_in(s);
            if spans.is_empty() {
                return;
            }
            let mut out = String::with_capacity(s.len());
            let mut at = 0;
            for (range, p) in spans {
                out.push_str(&s[at..range.start]);
                match values.get(&p) {
                    Some(v) => out.push_str(v.expose()),
                    None => out.push_str(&s[range.clone()]),
                }
                at = range.end;
            }
            out.push_str(&s[at..]);
            zeroize::Zeroize::zeroize(s);
            *s = out;
        });
    }
    Ok((filled, Injected { values: values.into_iter().collect() }))
}

impl Injected {
    /// Hold `other`'s values too: one scrub for everything sent to a server.
    pub fn absorb(&mut self, other: Injected) {
        for (p, v) in other.values {
            if !self.values.iter().any(|(q, _)| *q == p) {
                self.values.push((p, v));
            }
        }
    }

    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    /// The handles this call used, for the audit log and the consent card.
    pub fn handles(&self) -> Vec<String> {
        let mut h: Vec<String> = self.values.iter().map(|(p, _)| p.handle.clone()).collect();
        h.dedup();
        h
    }

    /// Replace every value this call sent, in every form it could come back
    /// in, with its placeholder. Returns how many were found.
    pub fn scrub(&self, text: &mut String) -> usize {
        let mut found = 0;
        for (placeholder, value) in &self.values {
            let v = value.expose();
            // Too short to be told apart from ordinary text; a four-digit PIN
            // coming back is not worth mangling every "2024" in the answer.
            if v.chars().count() < 6 {
                continue;
            }
            let marker = format!("‹{}›", placeholder.render().trim_start_matches("{{").trim_end_matches("}}"));
            for form in encodings(v) {
                if form.is_empty() {
                    continue;
                }
                let n = text.matches(form.as_str()).count();
                if n > 0 {
                    found += n;
                    *text = text.replace(form.as_str(), &marker);
                }
            }
            found += hide_in_base64(text, v.as_bytes(), &marker);
        }
        found
    }

    /// [`Self::scrub`] for a JSON result: every string in it.
    pub fn scrub_value(&self, value: &mut Value) -> usize {
        let mut found = 0;
        walk_mut(value, &mut |s| found += self.scrub(s));
        found
    }
}

/// Base64 the value sits *inside* rather than being all of: `user:password`
/// in a reflected `Authorization: Basic` header, a token inside an encoded
/// JSON blob. Base64 of a value only appears in base64 of a longer text when
/// the value happens to start on a three-byte boundary, so each run of
/// base64-looking text is decoded and looked in instead. The whole run goes.
fn hide_in_base64(text: &mut String, value: &[u8], marker: &str) -> usize {
    use base64::Engine;
    static RUN: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let run = RUN.get_or_init(|| regex::Regex::new(r"[A-Za-z0-9+/_\-]{8,}={0,2}").expect("the pattern compiles"));
    let engines = [
        base64::engine::general_purpose::STANDARD,
        base64::engine::general_purpose::STANDARD_NO_PAD,
        base64::engine::general_purpose::URL_SAFE,
        base64::engine::general_purpose::URL_SAFE_NO_PAD,
    ];
    let holds = |candidate: &str| {
        engines.iter().any(|e| {
            e.decode(candidate)
                .map(|decoded| zeroize::Zeroizing::new(decoded).windows(value.len()).any(|w| w == value))
                .unwrap_or(false)
        })
    };
    let spans: Vec<std::ops::Range<usize>> =
        run.find_iter(text).filter(|m| m.as_str().len() * 3 / 4 >= value.len() && holds(m.as_str())).map(|m| m.range()).collect();
    for r in spans.iter().rev() {
        text.replace_range(r.clone(), marker);
    }
    spans.len()
}

/// The forms a value may come back in: itself, base64 (standard and URL-safe,
/// padded or not), percent-encoded (either case, and as a form with `+` for a
/// space), hex, escaped inside a JSON string (with non-ASCII as `\uXXXX` or
/// not), and escaped for HTML (named or numbered entities).
fn encodings(v: &str) -> Vec<String> {
    use base64::Engine;
    let b = v.as_bytes();
    let mut forms = vec![
        v.to_string(),
        base64::engine::general_purpose::STANDARD.encode(b),
        base64::engine::general_purpose::STANDARD_NO_PAD.encode(b),
        base64::engine::general_purpose::URL_SAFE.encode(b),
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(b),
        urlencoding::encode(v).into_owned(),
        hex::encode(b),
        hex::encode_upper(b),
    ];
    if let Ok(json) = serde_json::to_string(v) {
        let json = json.trim_matches('"').to_string();
        let mut ascii = String::new();
        for c in json.chars() {
            if c.is_ascii() {
                ascii.push(c);
            } else {
                let mut units = [0u16; 2];
                for u in c.encode_utf16(&mut units) {
                    ascii.push_str(&format!("\\u{:04x}", u));
                }
            }
        }
        forms.push(ascii.replace('/', "\\/"));
        forms.push(ascii.to_uppercase().replace("\\U", "\\u"));
        forms.push(ascii);
        forms.push(json);
    }
    let percent = urlencoding::encode(v).into_owned();
    forms.push(lower_percent(&percent));
    forms.push(percent.replace("%20", "+"));
    forms.push(lower_percent(&percent.replace("%20", "+")));
    for (quote, apostrophe) in [("&quot;", "&#39;"), ("&#34;", "&#x27;"), ("&quot;", "&apos;")] {
        forms.push(
            v.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', quote).replace('\'', apostrophe),
        );
    }
    // Longest first, so a form that contains another is replaced whole.
    forms.sort_by_key(|f| std::cmp::Reverse(f.len()));
    forms.dedup();
    forms
}

/// `%2F` as `%2f`: servers write either.
fn lower_percent(encoded: &str) -> String {
    let mut out = String::with_capacity(encoded.len());
    let mut chars = encoded.chars();
    while let Some(c) = chars.next() {
        out.push(c);
        if c == '%' {
            out.extend(chars.by_ref().take(2).map(|h| h.to_ascii_lowercase()));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// A Safe with one item, `github-token`, allowed to go to the `github`
    /// connector only.
    struct Table;

    impl Lookup for Table {
        fn value(&self, p: &Placeholder, d: &Destination) -> Result<SecretString, EgressError> {
            if p.handle != "github-token" {
                return Err(EgressError::Unknown(p.handle.clone()));
            }
            if *d != Destination::Connector("github".into()) {
                return Err(EgressError::NotAllowed { handle: p.handle.clone(), destination: d.key() });
            }
            match p.field.as_deref() {
                None | Some("token") => Ok(SecretString::new("ghp_CANARY_1234567890abcdef".into())),
                Some(_) => Err(EgressError::NoSuchField(p.handle.clone())),
            }
        }
    }

    const GITHUB: fn() -> Destination = || Destination::Connector("github".into());

    #[test]
    fn placeholders_are_found_at_any_depth_and_only_when_well_formed() {
        let args = json!({
            "headers": { "Authorization": "Bearer {{safe:github-token}}" },
            "list": ["{{safe:other.password}}", "{{safe:Bad Handle}}", "{{safe:}}", "{{safe:x"],
        });
        let found = find(&args);
        assert_eq!(
            found,
            vec![
                Placeholder { handle: "github-token".into(), field: None },
                Placeholder { handle: "other".into(), field: Some("password".into()) },
            ]
        );
    }

    #[test]
    fn a_copy_is_filled_and_the_original_keeps_its_placeholder() {
        let args = json!({ "auth": "Bearer {{safe:github-token}}", "n": 3 });
        let (filled, injected) = fill(&args, &GITHUB(), &Table).unwrap();
        assert_eq!(filled["auth"], "Bearer ghp_CANARY_1234567890abcdef");
        assert_eq!(args["auth"], "Bearer {{safe:github-token}}", "the recorded arguments changed");
        assert_eq!(injected.handles(), vec!["github-token"]);
    }

    /// The attack: a page convinced Syn to send the token somewhere else.
    #[test]
    fn a_destination_the_item_was_not_given_to_gets_nothing() {
        let args = json!({ "body": "{{safe:github-token}}" });
        let err = fill(&args, &Destination::Connector("evil".into()), &Table).err().unwrap();
        assert!(matches!(err, EgressError::NotAllowed { .. }));
    }

    #[test]
    fn one_unknown_placeholder_sends_nothing_at_all() {
        let args = json!({ "a": "{{safe:github-token}}", "b": "{{safe:missing}}" });
        assert_eq!(fill(&args, &GITHUB(), &Table).err().unwrap(), EgressError::Unknown("missing".into()));
    }

    #[test]
    fn args_without_placeholders_pass_untouched() {
        let args = json!({ "q": "hello {{not a placeholder}}" });
        let (filled, injected) = fill(&args, &GITHUB(), &Table).unwrap();
        assert_eq!(filled, args);
        assert!(injected.is_empty());
    }

    /// A service that echoes the token back, in each form it might.
    #[test]
    fn every_echo_of_the_value_is_scrubbed() {
        use base64::Engine;
        let (_, injected) = fill(&json!({ "a": "{{safe:github-token}}" }), &GITHUB(), &Table).unwrap();
        let v = "ghp_CANARY_1234567890abcdef";
        let mut result = json!({
            "echo": format!("you sent {v}"),
            "b64": base64::engine::general_purpose::STANDARD.encode(v),
            "url": format!("https://x/?t={}", urlencoding::encode(v)),
            "hex": hex::encode(v),
            "nested": [{ "deep": format!("token={v}") }],
        });
        let n = injected.scrub_value(&mut result);
        let text = result.to_string();
        assert!(!text.contains("CANARY"), "{text}");
        assert!(!text.contains(&hex::encode(v)), "{text}");
        assert!(n >= 5, "found {n}");
        assert!(text.contains("‹safe:github-token›"));
    }

    /// The echoes a real server makes of a password with symbols in it: inside
    /// a Basic header, in an HTML error page, in a form, in JSON with its
    /// non-ASCII escaped.
    #[test]
    fn echoes_inside_other_text_and_other_escapings_are_scrubbed() {
        use base64::Engine;
        let v = "p@ss w/rd&<é>\"'42";
        let injected = Injected {
            values: vec![(Placeholder { handle: "pw".into(), field: None }, SecretString::new(v.into()))],
        };
        let basic = base64::engine::general_purpose::STANDARD.encode(format!("anh:{v}"));
        let blob = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(format!("{{\"user\":\"anh\",\"pass\":\"{v}\"}}"));
        for echo in [
            format!("Authorization: Basic {basic}"),
            format!("state={blob}&x=1"),
            "<p>Bad password: p@ss w/rd&amp;&lt;é&gt;&quot;&#39;42</p>".to_string(),
            "<p>p@ss w/rd&amp;&lt;é&gt;&#34;&#x27;42</p>".to_string(),
            "password=p%40ss+w%2frd%26%3c%c3%a9%3e%22%2742".to_string(),
            "{\"pw\":\"p@ss w\\/rd&<\\u00e9>\\\"'42\"}".to_string(),
        ] {
            let mut text = echo.clone();
            assert!(injected.scrub(&mut text) > 0, "missed: {echo}");
            assert!(text.contains("‹safe:pw›"), "{text}");
        }
        let mut plain = "nothing to see: aGVsbG8gd29ybGQ=".to_string();
        assert_eq!(injected.scrub(&mut plain), 0, "{plain}");
    }

}
