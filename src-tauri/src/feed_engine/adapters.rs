//! Sites that have feeds but do not advertise them.
//!
//! YouTube and Reddit both publish perfectly good Atom and RSS, and neither
//! puts a `<link rel="alternate">` on the page a person would paste. Pasting a
//! channel URL used to fall through discovery to the scraper, which produced a
//! worse version of a feed that was there all along.
//!
//! Everything here is a URL rewrite. No request is made to work out the feed
//! address unless the address genuinely cannot be derived from the URL, which
//! is the case only for the YouTube handle and vanity forms.

use url::Url;

/// A feed address derived from a page address.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdaptedFeed {
    pub url: String,
    /// `youtube` or `reddit`, so the rest of the app can tell what it is.
    pub feed_type: String,
    /// Set when the address could not be derived and the page must be read.
    pub needs_channel_lookup: bool,
}

/// Work out the feed for a URL, if this is a site we know how to translate.
pub fn adapt(raw_url: &str) -> Option<AdaptedFeed> {
    let url = Url::parse(raw_url).ok()?;
    let host = url.host_str()?.trim_start_matches("www.").to_ascii_lowercase();

    match host.as_str() {
        "youtube.com" | "m.youtube.com" | "youtu.be" => adapt_youtube(&url),
        "reddit.com" | "old.reddit.com" | "np.reddit.com" => adapt_reddit(&url),
        _ => None,
    }
}

fn adapt_youtube(url: &Url) -> Option<AdaptedFeed> {
    let segments: Vec<&str> = url
        .path_segments()
        .map(|s| s.filter(|p| !p.is_empty()).collect())
        .unwrap_or_default();

    // A playlist is a feed in its own right, whatever page it was linked from.
    if let Some(list) = url
        .query_pairs()
        .find(|(k, _)| k == "list")
        .map(|(_, v)| v.to_string())
    {
        return Some(AdaptedFeed {
            url: format!(
                "https://www.youtube.com/feeds/videos.xml?playlist_id={}",
                list
            ),
            feed_type: "youtube".to_string(),
            needs_channel_lookup: false,
        });
    }

    match segments.as_slice() {
        // The canonical form, and the only one that carries the id outright.
        ["channel", id, ..] => Some(AdaptedFeed {
            url: format!("https://www.youtube.com/feeds/videos.xml?channel_id={}", id),
            feed_type: "youtube".to_string(),
            needs_channel_lookup: false,
        }),
        // `@handle`, `/c/name`, `/user/name` — the id is not in the URL, so
        // the channel page has to be read for it.
        [handle, ..] if handle.starts_with('@') => Some(needs_lookup()),
        ["c", _, ..] | ["user", _, ..] => Some(needs_lookup()),
        _ => None,
    }
}

fn needs_lookup() -> AdaptedFeed {
    AdaptedFeed {
        url: String::new(),
        feed_type: "youtube".to_string(),
        needs_channel_lookup: true,
    }
}

fn adapt_reddit(url: &Url) -> Option<AdaptedFeed> {
    let segments: Vec<&str> = url
        .path_segments()
        .map(|s| s.filter(|p| !p.is_empty()).collect())
        .unwrap_or_default();

    match segments.as_slice() {
        ["r", subreddit, ..] => Some(AdaptedFeed {
            url: format!("https://www.reddit.com/r/{}/.rss", subreddit),
            feed_type: "reddit".to_string(),
            needs_channel_lookup: false,
        }),
        ["user", name, ..] | ["u", name, ..] => Some(AdaptedFeed {
            url: format!("https://www.reddit.com/user/{}/.rss", name),
            feed_type: "reddit".to_string(),
            needs_channel_lookup: false,
        }),
        _ => None,
    }
}

/// Pull a YouTube channel id out of a channel page.
///
/// The id appears in a canonical link and in the page's own metadata; either
/// will do, and looking for both costs nothing over looking for one.
pub fn youtube_channel_id(html: &str) -> Option<String> {
    use scraper::{Html, Selector};

    let document = Html::parse_document(html);

    if let Ok(selector) = Selector::parse(r#"meta[itemprop="identifier"][content]"#) {
        if let Some(id) = document
            .select(&selector)
            .next()
            .and_then(|el| el.value().attr("content"))
            .filter(|id| id.starts_with("UC"))
        {
            return Some(id.to_string());
        }
    }

    for selector in [r#"link[rel="canonical"][href]"#, r#"meta[property="og:url"][content]"#] {
        let Ok(parsed) = Selector::parse(selector) else {
            continue;
        };
        for element in document.select(&parsed) {
            let value = element
                .value()
                .attr("href")
                .or_else(|| element.value().attr("content"))
                .unwrap_or_default();
            if let Some(id) = value.split("/channel/").nth(1) {
                let id = id.split(['/', '?']).next().unwrap_or_default();
                if id.starts_with("UC") {
                    return Some(id.to_string());
                }
            }
        }
    }

    None
}

/// The Atom feed for a YouTube channel id.
pub fn youtube_feed_for_channel(channel_id: &str) -> String {
    format!(
        "https://www.youtube.com/feeds/videos.xml?channel_id={}",
        channel_id
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn adapted(url: &str) -> AdaptedFeed {
        adapt(url).unwrap_or_else(|| panic!("{url} should be adapted"))
    }

    #[test]
    fn a_youtube_channel_url_becomes_its_atom_feed() {
        for url in [
            "https://www.youtube.com/channel/UCabcdef123/videos",
            "https://youtube.com/channel/UCabcdef123",
            "https://m.youtube.com/channel/UCabcdef123/featured",
        ] {
            assert_eq!(
                adapted(url).url,
                "https://www.youtube.com/feeds/videos.xml?channel_id=UCabcdef123"
            );
        }
    }

    #[test]
    fn a_handle_or_vanity_url_says_it_needs_the_page_read() {
        for url in [
            "https://www.youtube.com/@someone",
            "https://www.youtube.com/@someone/videos",
            "https://www.youtube.com/c/SomeChannel",
            "https://www.youtube.com/user/SomeChannel",
        ] {
            let feed = adapted(url);
            assert!(feed.needs_channel_lookup, "{url} carries no channel id");
            assert!(feed.url.is_empty());
        }
    }

    #[test]
    fn a_playlist_is_a_feed_of_its_own() {
        let feed = adapted("https://www.youtube.com/playlist?list=PL123abc");
        assert_eq!(
            feed.url,
            "https://www.youtube.com/feeds/videos.xml?playlist_id=PL123abc"
        );
    }

    #[test]
    fn a_subreddit_becomes_its_rss() {
        for url in [
            "https://www.reddit.com/r/rust/",
            "https://old.reddit.com/r/rust/top/",
            "https://reddit.com/r/rust",
        ] {
            assert_eq!(adapted(url).url, "https://www.reddit.com/r/rust/.rss");
        }
    }

    #[test]
    fn a_reddit_user_becomes_their_rss() {
        assert_eq!(
            adapted("https://www.reddit.com/user/someone/").url,
            "https://www.reddit.com/user/someone/.rss"
        );
        assert_eq!(
            adapted("https://www.reddit.com/u/someone").url,
            "https://www.reddit.com/user/someone/.rss"
        );
    }

    #[test]
    fn ordinary_sites_are_left_to_ordinary_discovery() {
        for url in [
            "https://example.com/blog",
            "https://notyoutube.com/channel/UC123",
            "https://www.reddit.com/",
            "https://www.youtube.com/watch?v=abc",
        ] {
            assert!(adapt(url).is_none(), "{url} should not be adapted");
        }
    }

    #[test]
    fn a_channel_id_is_found_however_the_page_states_it() {
        assert_eq!(
            youtube_channel_id(r#"<meta itemprop="identifier" content="UCfromMeta">"#),
            Some("UCfromMeta".to_string())
        );
        assert_eq!(
            youtube_channel_id(
                r#"<link rel="canonical" href="https://www.youtube.com/channel/UCfromCanonical">"#
            ),
            Some("UCfromCanonical".to_string())
        );
        assert_eq!(
            youtube_channel_id(
                r#"<meta property="og:url" content="https://www.youtube.com/channel/UCfromOg/videos">"#
            ),
            Some("UCfromOg".to_string())
        );
        assert_eq!(youtube_channel_id("<p>nothing here</p>"), None);
    }
}
