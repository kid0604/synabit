use futures::StreamExt;
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Maximum response body size.
///
/// Two megabytes used to be the cap, which is under the size of a full-text
/// feed from any blog with a few years of archive behind it — and the failure
/// was permanent, because a feed too large to read never becomes smaller.
const MAX_RESPONSE_SIZE: usize = 16 * 1024 * 1024;

/// Maximum size of an HTML page fetched for scraping or article extraction.
const MAX_PAGE_SIZE: usize = 8 * 1024 * 1024;

/// The user agent every request in this module sends.
///
/// It names the app rather than impersonating Chrome. Pretending to be a
/// browser is how a client gets an entire product blocked once anyone notices,
/// and a publisher who wants to rate-limit a reader should be able to.
const USER_AGENT: &str = concat!(
    "Synabit/",
    env!("CARGO_PKG_VERSION"),
    " (+https://synabit.app; feed reader)"
);

/// An HTTP client configured the way every fetch in this module wants it.
pub fn build_client(timeout: Duration) -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .timeout(timeout)
        .user_agent(USER_AGENT)
        .default_headers({
            let mut h = reqwest::header::HeaderMap::new();
            h.insert(
                reqwest::header::ACCEPT,
                "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8"
                    .parse()
                    .expect("a static header value"),
            );
            h
        })
        .redirect(reqwest::redirect::Policy::limited(5))
        .build()
        .map_err(|e| format!("HTTP client error: {}", e))
}

/// Read a response body, giving up once it exceeds `limit`.
///
/// The limit is enforced as the bytes arrive rather than after the fact.
/// `Content-Length` is a claim, not a promise, and three of the four fetch
/// paths in this module used to call `.text()` with no ceiling at all — one
/// hostile URL was enough to grow the process until it died.
async fn read_capped(response: reqwest::Response, limit: usize) -> Result<Vec<u8>, String> {
    if let Some(len) = response.content_length() {
        if len as usize > limit {
            return Err(format!("Response too large: {} bytes (max {})", len, limit));
        }
    }

    let mut body = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| format!("Failed to read response body: {}", e))?;
        if body.len() + chunk.len() > limit {
            return Err(format!("Response body too large (max {} bytes)", limit));
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

/// Fetch an HTML page as text, with a size ceiling.
pub async fn fetch_page(url: &str) -> Result<String, String> {
    guard_url(url)?;
    let client = build_client(Duration::from_secs(20))?;
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("Failed to fetch {}: {}", url, e))?;

    if !response.status().is_success() {
        return Err(format!("HTTP {} from {}", response.status(), url));
    }

    let body = read_capped(response, MAX_PAGE_SIZE).await?;
    Ok(String::from_utf8_lossy(&body).into_owned())
}

/// Result of a conditional HTTP fetch.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "status")]
pub enum FetchResult {
    /// Server returned 304 Not Modified — nothing new.
    NotModified,
    /// Feed content was fetched successfully.
    Updated {
        body: Vec<u8>,
        etag: Option<String>,
        last_modified: Option<String>,
    },
    /// An error occurred during fetch.
    Error {
        message: String,
        /// Seconds the server asked us to wait, from `Retry-After`.
        retry_after: Option<i64>,
    },
}

/// Reject a URL this app should not be fetching on someone's behalf.
///
/// A feed URL is typed by a person, or comes out of an OPML file they were
/// given, and the fetch runs in the privileged Rust process — so "http://
/// localhost:8080/admin" or the cloud metadata address are worth refusing
/// before they are dialled.
///
/// This checks the host as written. It is not a defence against DNS rebinding
/// or a public name that resolves to a private address; stopping those means
/// resolving the name here and pinning the result, which reqwest does not let
/// us hand back. What it does stop is the whole class of mistakes people
/// actually make, and it costs one parse.
pub fn guard_url(url: &str) -> Result<(), String> {
    let parsed = url::Url::parse(url).map_err(|e| format!("Not a usable URL: {}", e))?;

    if !matches!(parsed.scheme(), "http" | "https") {
        return Err(format!("Refusing to fetch a {} URL", parsed.scheme()));
    }

    let Some(host) = parsed.host() else {
        return Err("URL has no host".to_string());
    };

    let private = match host {
        url::Host::Ipv4(ip) => {
            ip.is_loopback()
                || ip.is_private()
                || ip.is_link_local()
                || ip.is_unspecified()
                || ip.is_broadcast()
                || ip.is_multicast()
        }
        url::Host::Ipv6(ip) => ip.is_loopback() || ip.is_unspecified() || ip.is_multicast(),
        url::Host::Domain(name) => {
            let name = name.to_ascii_lowercase();
            name == "localhost"
                || name.ends_with(".localhost")
                || name.ends_with(".local")
                || name.ends_with(".internal")
        }
    };

    if private {
        return Err(format!(
            "Refusing to fetch {} — that address is on this machine or its private network",
            parsed.host_str().unwrap_or_default()
        ));
    }

    Ok(())
}

/// How long a server asked us to wait, from a `Retry-After` header.
///
/// The header comes in two spellings: a number of seconds, or an HTTP date.
/// Both are worth honouring — a feed that answers 429 and is asked again on
/// our own schedule is a feed we are about to be blocked by.
fn retry_after_seconds(headers: &reqwest::header::HeaderMap) -> Option<i64> {
    let raw = headers.get(reqwest::header::RETRY_AFTER)?.to_str().ok()?;

    if let Ok(seconds) = raw.trim().parse::<i64>() {
        return Some(seconds.max(0));
    }

    let when = chrono::DateTime::parse_from_rfc2822(raw.trim()).ok()?;
    let seconds = when
        .with_timezone(&chrono::Utc)
        .signed_duration_since(chrono::Utc::now())
        .num_seconds();
    Some(seconds.max(0))
}

/// Fetch a feed URL with conditional request headers (ETag / If-Modified-Since).
pub async fn fetch_feed(url: &str, etag: Option<&str>, last_modified: Option<&str>) -> FetchResult {
    if let Err(message) = guard_url(url) {
        return FetchResult::Error {
            message,
            retry_after: None,
        };
    }

    let client = match build_client(Duration::from_secs(30)) {
        Ok(c) => c,
        Err(message) => {
            return FetchResult::Error {
                message,
                retry_after: None,
            }
        }
    };

    let mut req = client.get(url);

    // Conditional headers
    if let Some(etag_val) = etag {
        req = req.header("If-None-Match", etag_val);
    }
    if let Some(lm_val) = last_modified {
        req = req.header("If-Modified-Since", lm_val);
    }

    let response = match req.send().await {
        Ok(r) => r,
        Err(e) => {
            return FetchResult::Error {
                message: format!("HTTP request failed: {}", e),
                retry_after: None,
            }
        }
    };

    // 304 Not Modified
    if response.status() == reqwest::StatusCode::NOT_MODIFIED {
        return FetchResult::NotModified;
    }

    // Non-success status
    if !response.status().is_success() {
        return FetchResult::Error {
            message: format!("HTTP {} from {}", response.status(), url),
            retry_after: retry_after_seconds(response.headers()),
        };
    }

    // Extract caching headers before consuming the body
    let new_etag = response
        .headers()
        .get("etag")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    let new_last_modified = response
        .headers()
        .get("last-modified")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    let body = match read_capped(response, MAX_RESPONSE_SIZE).await {
        Ok(body) => body,
        Err(message) => {
            return FetchResult::Error {
                message,
                retry_after: None,
            }
        }
    };

    FetchResult::Updated {
        body,
        etag: new_etag,
        last_modified: new_last_modified,
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn addresses_on_this_machine_are_refused() {
        for url in [
            "http://localhost:8080/admin",
            "http://127.0.0.1/",
            "http://[::1]:3000/",
            "http://0.0.0.0/",
            "http://printer.local/feed",
            "http://vault.internal/rss",
        ] {
            assert!(guard_url(url).is_err(), "{url} should be refused");
        }
    }

    #[test]
    fn private_networks_and_metadata_services_are_refused() {
        for url in [
            "http://192.168.1.1/feed",
            "http://10.0.0.5/rss",
            "http://172.16.4.4/",
            "http://169.254.169.254/latest/meta-data/",
        ] {
            assert!(guard_url(url).is_err(), "{url} should be refused");
        }
    }

    #[test]
    fn only_http_is_fetched() {
        assert!(guard_url("file:///etc/passwd").is_err());
        assert!(guard_url("ftp://example.com/feed.xml").is_err());
        assert!(guard_url("not a url at all").is_err());
    }

    #[test]
    fn ordinary_feeds_are_left_alone() {
        for url in [
            "https://example.com/feed.xml",
            "http://blog.example.co.uk/index.xml",
            "https://192.0.2.1/feed", // documentation range, but public
        ] {
            assert!(guard_url(url).is_ok(), "{url} should be allowed");
        }
    }

    #[test]
    fn retry_after_is_read_in_both_of_its_spellings() {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(reqwest::header::RETRY_AFTER, "120".parse().unwrap());
        assert_eq!(retry_after_seconds(&headers), Some(120));

        // An HTTP date, far enough ahead that the arithmetic is unambiguous.
        let when = chrono::Utc::now() + chrono::Duration::hours(2);
        headers.insert(
            reqwest::header::RETRY_AFTER,
            when.to_rfc2822().parse().unwrap(),
        );
        let seconds = retry_after_seconds(&headers).expect("a date is a valid Retry-After");
        assert!(
            (7000..=7200).contains(&seconds),
            "about two hours, got {seconds}"
        );
    }

    #[test]
    fn a_date_already_past_asks_for_no_wait_rather_than_a_negative_one() {
        let mut headers = reqwest::header::HeaderMap::new();
        let when = chrono::Utc::now() - chrono::Duration::hours(1);
        headers.insert(
            reqwest::header::RETRY_AFTER,
            when.to_rfc2822().parse().unwrap(),
        );
        assert_eq!(retry_after_seconds(&headers), Some(0));
    }

    #[test]
    fn nonsense_in_the_header_is_simply_not_an_instruction() {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(reqwest::header::RETRY_AFTER, "soon".parse().unwrap());
        assert_eq!(retry_after_seconds(&headers), None);
        assert_eq!(retry_after_seconds(&reqwest::header::HeaderMap::new()), None);
    }
}
