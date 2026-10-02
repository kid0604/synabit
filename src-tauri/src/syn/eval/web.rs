//! The web, as a task supplies it.
//!
//! `browse` opens a visible WebView to search, which cannot run headless and
//! would make every run depend on what a search engine says today. So while a
//! task is running, `engine::browse` asks here first: a search is answered
//! from the task's pages, an address with the page the task gave for it, and
//! an address the task did not give is an error — never the real internet.
//!
//! What comes back goes through the product's own `web::reduce` and
//! `web::wrap`, so the model reads a fixture page exactly as it would read a
//! real one, untrusted-content framing included. The guards that run before
//! `browse` reaches here (`taint::Destinations`, the gate) are untouched.

use std::cell::RefCell;

use crate::error::{AppError, AppResult};
use crate::models::syn::SourceRef;

/// One page a task puts on its web.
#[derive(Debug, Clone)]
pub struct Page {
    pub url: &'static str,
    /// Words a search for this page would use, lower-cased.
    pub topics: &'static [&'static str],
    pub html: String,
}

#[derive(Default)]
struct Web {
    pages: Vec<Page>,
    /// Every search and address asked for, in order: `search:<words>` or the URL.
    visited: Vec<String>,
}

thread_local! {
    static WEB: RefCell<Option<Web>> = const { RefCell::new(None) };
}

/// Serve `pages` to this thread's runs until the guard drops.
pub fn serve(pages: Vec<Page>) -> WebGuard {
    WEB.with(|w| *w.borrow_mut() = Some(Web { pages, visited: Vec::new() }));
    WebGuard
}

pub struct WebGuard;

impl Drop for WebGuard {
    fn drop(&mut self) {
        WEB.with(|w| *w.borrow_mut() = None);
    }
}

/// Everything the runs on this thread asked the web for.
pub fn visited() -> Vec<String> {
    WEB.with(|w| w.borrow().as_ref().map(|w| w.visited.clone()).unwrap_or_default())
}

fn same(a: &str, b: &str) -> bool {
    a.trim_end_matches('/').eq_ignore_ascii_case(b.trim_end_matches('/'))
}

/// A search, answered from the task's pages. `None` when no task is serving.
pub(crate) fn search(what: &str) -> Option<AppResult<(String, Vec<SourceRef>)>> {
    WEB.with(|w| {
        let mut slot = w.borrow_mut();
        let web = slot.as_mut()?;
        web.visited.push(format!("search:{what}"));
        let words: Vec<String> = what
            .split_whitespace()
            .map(|w| crate::syn::rag::fold(&w.to_lowercase()))
            .filter(|w| w.chars().count() > 2)
            .collect();
        let mut scored: Vec<(usize, &Page)> = web
            .pages
            .iter()
            .map(|p| {
                // Either way round, the way a search engine is forgiving:
                // "pricing" answers "price", and "vendor-a.example" names "vendor".
                let topics: Vec<String> = p.topics.iter().map(|t| crate::syn::rag::fold(t)).collect();
                let mut hits = words
                    .iter()
                    .filter(|w| topics.iter().any(|t| t.contains(w.as_str()) || w.contains(t.as_str())))
                    .count();
                // A query that names the page's own site finds it first.
                let host = crate::syn::taint::host_of(p.url).unwrap_or_default();
                if !host.is_empty() && what.to_lowercase().contains(&host) {
                    hits += 3;
                }
                (hits, p)
            })
            .filter(|(hits, _)| *hits > 0)
            .collect();
        scored.sort_by(|a, b| b.0.cmp(&a.0));
        let opened: Vec<crate::syn::web::Page> = scored
            .iter()
            .take(2)
            .map(|(_, p)| crate::syn::web::reduce(&p.html, p.url))
            .collect();
        if opened.is_empty() {
            return Some(Ok((format!("No results for \"{what}\"."), Vec::new())));
        }
        let mut body = String::new();
        if opened.len() > 1 {
            body.push_str(crate::syn::web::TWO_SOURCES);
            body.push_str("\n\n");
        }
        body.push_str(&opened.iter().map(crate::syn::web::wrap).collect::<Vec<_>>().join("\n\n"));
        Some(Ok((body, opened.iter().map(crate::syn::web::citation).collect())))
    })
}

/// A page at an address. `None` when no task is serving; an error when one is
/// and did not give this address — the eval never reaches the real internet.
pub(crate) fn page(address: &str) -> Option<AppResult<(String, Vec<SourceRef>)>> {
    WEB.with(|w| {
        let mut slot = w.borrow_mut();
        let web = slot.as_mut()?;
        web.visited.push(address.to_string());
        Some(match web.pages.iter().find(|p| same(p.url, address)) {
            Some(p) => {
                let page = crate::syn::web::reduce(&p.html, p.url);
                Ok((crate::syn::web::wrap(&page), vec![crate::syn::web::citation(&page)]))
            }
            None => Err(AppError::General(format!("Could not open {address}: no such page."))),
        })
    })
}
