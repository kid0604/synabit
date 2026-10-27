//! What the model wrote, looked at before it is kept.
//!
//! The frame is what holds a visual in: no network, no forms, no way to its
//! parent but `postMessage`. This is not a second wall. It is the model being
//! told, before anything is kept, that something it wrote cannot work there —
//! a script from a CDN, an image from the web, a form — so it fixes the visual
//! rather than the person meeting a blank one. Google's generative UI does the
//! same after the fact (arXiv 2604.09577, its post-processors); here the model
//! fixes it itself, up to twice a visual (plan, PR 4).
//!
//! A **problem** keeps the visual from being kept. A **warning** is kept with
//! it and told to the model: something the frame will refuse, which the
//! visual should not rely on.

use scraper::{Html, Selector};

/// How long a visual may be, in characters. The ceiling of what a model writes
/// in one call, not a share of a context window: a visual this long is a page,
/// and a page is not what a visual is for.
pub const MAX_CHARS: usize = 120_000;

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Checked {
    pub problems: Vec<String>,
    pub warnings: Vec<String>,
}

impl Checked {
    pub fn passed(&self) -> bool {
        self.problems.is_empty()
    }
}

/// Elements a visual has no use for and the frame would not run.
const NOT_IN_A_VISUAL: &[(&str, &str)] = &[
    ("form", "a form cannot be sent from the frame; use inputs and buttons with script"),
    ("iframe", "a frame cannot hold another frame"),
    ("frame", "a frame cannot hold another frame"),
    ("object", "embedded objects do not load in the frame"),
    ("embed", "embedded objects do not load in the frame"),
    ("base", "a <base> would change where every path points"),
    ("meta[http-equiv]", "the page's policy is set by the app"),
];

/// Attributes that point somewhere.
const POINTERS: &[&str] = &["src", "href", "xlink:href", "srcset", "poster", "action", "formaction", "data", "background", "ping"];

/// Script that the frame refuses, worth a word to the model.
const REFUSED_IN_SCRIPT: &[(&str, &str)] = &[
    ("window.parent", "the parent page is out of reach"),
    ("window.top", "the parent page is out of reach"),
    ("parent.document", "the parent page is out of reach"),
    ("localStorage", "the frame keeps nothing between visits"),
    ("sessionStorage", "the frame keeps nothing between visits"),
    ("indexedDB", "the frame keeps nothing between visits"),
    ("document.cookie", "the frame keeps nothing between visits"),
    ("fetch(", "the frame has no network; put the data in the visual"),
    ("XMLHttpRequest", "the frame has no network; put the data in the visual"),
    ("WebSocket", "the frame has no network"),
    ("eval(", "eval is refused"),
    ("new Function", "eval is refused"),
];

/// Where a pointer may point: the libraries shipped with the app, a place in
/// the visual itself, or data written into it.
fn may_point_to(value: &str) -> bool {
    let v = value.trim();
    v.is_empty() || v.starts_with('#') || v.starts_with("/lib/") || v.starts_with("data:") || v.starts_with("blob:")
}

/// Every `url(…)` in a stylesheet or a style attribute that points elsewhere.
fn urls_elsewhere(css: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = css;
    while let Some(at) = rest.find("url(") {
        let after = &rest[at + 4..];
        let end = after.find(')').unwrap_or(after.len());
        let target = after[..end].trim().trim_matches(|c| c == '\'' || c == '"');
        if !may_point_to(target) {
            out.push(target.to_string());
        }
        rest = &after[end..];
    }
    if css.contains("@import") {
        out.push("@import".into());
    }
    out
}

/// Look at a fragment before it is kept.
pub fn check(html: &str) -> Checked {
    let mut checked = Checked::default();
    let chars = html.chars().count();
    if html.trim().is_empty() {
        checked.problems.push("The visual is empty.".into());
        return checked;
    }
    if chars > MAX_CHARS {
        checked.problems.push(format!(
            "The visual is {chars} characters; at most {MAX_CHARS}. Draw one compact visual, not a page — the explanation goes in the answer."
        ));
    }

    let lower = html.to_ascii_lowercase();
    if lower.contains("<!doctype") || lower.contains("<html") || lower.contains("<head") || lower.contains("<body") {
        checked.warnings.push("Write a fragment: the app supplies <html>, <head> and <body>, with the kit already loaded.".into());
    }

    let fragment = Html::parse_fragment(html);
    for (selector, why) in NOT_IN_A_VISUAL {
        let Ok(found) = Selector::parse(selector) else { continue };
        if fragment.select(&found).next().is_some() {
            checked.problems.push(format!("<{}> is not allowed: {why}.", selector.split('[').next().unwrap_or(selector)));
        }
    }

    let mut elsewhere: Vec<String> = Vec::new();
    for element in fragment.root_element().descendants().filter_map(scraper::ElementRef::wrap) {
        for (name, value) in element.value().attrs() {
            let name = name.to_ascii_lowercase();
            if POINTERS.contains(&name.as_str()) {
                let targets: Vec<&str> = if name == "srcset" { value.split(',').filter_map(|c| c.split_whitespace().next()).collect() } else { vec![value] };
                elsewhere.extend(targets.into_iter().filter(|t| !may_point_to(t)).map(str::to_string));
            }
            if name == "style" {
                elsewhere.extend(urls_elsewhere(value));
            }
        }
    }
    if let Ok(styles) = Selector::parse("style") {
        for style in fragment.select(&styles) {
            elsewhere.extend(urls_elsewhere(&style.text().collect::<String>()));
        }
    }
    elsewhere.sort();
    elsewhere.dedup();
    if !elsewhere.is_empty() {
        let shown: Vec<String> = elsewhere.iter().take(5).map(|t| t.chars().take(80).collect()).collect();
        checked.problems.push(format!(
            "The visual points outside itself: {}. It has no network; only /lib/d3.v7.min.js, /lib/katex.min.js and /lib/katex.min.css can be loaded, and anything else must be written into the visual.",
            shown.join(", ")
        ));
    }

    if let Ok(scripts) = Selector::parse("script") {
        let code: String = fragment.select(&scripts).flat_map(|s| s.text()).collect();
        let mut said: Vec<&str> = Vec::new();
        for (needle, why) in REFUSED_IN_SCRIPT {
            if code.contains(needle) && !said.contains(why) {
                said.push(why);
                checked.warnings.push(format!("`{needle}` will not work: {why}."));
            }
        }
    }
    checked
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_visual_using_the_kit_and_the_libraries_passes() {
        let html = r##"<link rel="stylesheet" href="/lib/katex.min.css">
<div class="row"><input type="range" id="l"><a href="#notes">notes</a></div>
<img src="data:image/png;base64,iVBORw0KGgo=" alt="">
<svg><use xlink:href="#dot"/></svg>
<style>.dot { background: url(data:image/svg+xml;utf8,<svg/>); }</style>
<script src="/lib/d3.v7.min.js"></script>
<script>synabit.ready.then(() => d3.select('svg'));</script>"##;
        let checked = check(html);
        assert_eq!(checked, Checked::default(), "{checked:?}");
    }

    #[test]
    fn anything_from_elsewhere_is_a_problem() {
        for html in [
            r#"<script src="https://cdnjs.cloudflare.com/ajax/libs/Chart.js/4.4.1/chart.umd.js"></script>"#,
            r#"<img src="https://example.com/x.png?d=vault">"#,
            r#"<img srcset="/lib/a.png 1x, https://example.com/b.png 2x">"#,
            r#"<link rel="stylesheet" href="https://fonts.googleapis.com/css2?family=Inter">"#,
            r#"<div style="background:url('https://example.com/bg.png')"></div>"#,
            r#"<style>@import url(https://example.com/a.css);</style>"#,
            r#"<a href="javascript:alert(1)">x</a>"#,
            r#"<video poster="//example.com/p.jpg"></video>"#,
        ] {
            let checked = check(html);
            assert!(!checked.passed(), "{html}: {checked:?}");
            assert!(checked.problems.iter().any(|p| p.contains("points outside")), "{html}: {checked:?}");
        }
    }

    #[test]
    fn elements_the_frame_will_not_run_are_problems() {
        for (html, tag) in [
            ("<form action=\"/x\"><input></form>", "<form>"),
            ("<iframe src=\"/lib/x\"></iframe>", "<iframe>"),
            ("<object data=\"/lib/x\"></object>", "<object>"),
            ("<embed src=\"/lib/x\">", "<embed>"),
            ("<base href=\"/lib/\">", "<base>"),
            ("<meta http-equiv=\"refresh\" content=\"0\">", "<meta>"),
        ] {
            let checked = check(html);
            assert!(checked.problems.iter().any(|p| p.starts_with(tag)), "{html}: {checked:?}");
        }
    }

    #[test]
    fn what_the_frame_refuses_in_script_is_a_warning_not_a_problem() {
        let checked = check("<script>localStorage.setItem('a', 1); fetch('/x'); window.top.location = '#';</script>");
        assert!(checked.passed());
        assert_eq!(checked.warnings.len(), 3, "{:?}", checked.warnings);
    }

    #[test]
    fn size_and_shape() {
        assert!(!check("   ").passed());
        assert!(!check(&"<p>x</p>".repeat(MAX_CHARS / 8 + 1)).passed());
        let page = check("<!doctype html><html><body><p>x</p></body></html>");
        assert!(page.passed(), "a page still works inside the body");
        assert!(page.warnings.iter().any(|w| w.contains("fragment")));
    }
}
