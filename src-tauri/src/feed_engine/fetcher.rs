use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Maximum response body size: 2 MB
const MAX_RESPONSE_SIZE: usize = 2 * 1024 * 1024;

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
    Error { message: String },
}

/// Fetch a feed URL with conditional request headers (ETag / If-Modified-Since).
pub async fn fetch_feed(
    url: &str,
    etag: Option<&str>,
    last_modified: Option<&str>,
) -> FetchResult {
    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .user_agent("Synabit/1.0 Feed Reader")
        .build()
    {
        Ok(c) => c,
        Err(e) => return FetchResult::Error { message: format!("Failed to build HTTP client: {}", e) },
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
        Err(e) => return FetchResult::Error { message: format!("HTTP request failed: {}", e) },
    };

    // 304 Not Modified
    if response.status() == reqwest::StatusCode::NOT_MODIFIED {
        return FetchResult::NotModified;
    }

    // Non-success status
    if !response.status().is_success() {
        return FetchResult::Error {
            message: format!("HTTP {} from {}", response.status(), url),
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

    // Check content length if provided
    if let Some(len) = response.content_length() {
        if len as usize > MAX_RESPONSE_SIZE {
            return FetchResult::Error {
                message: format!("Response too large: {} bytes (max {})", len, MAX_RESPONSE_SIZE),
            };
        }
    }

    // Read body with size limit
    let body = match response.bytes().await {
        Ok(b) => {
            if b.len() > MAX_RESPONSE_SIZE {
                return FetchResult::Error {
                    message: format!(
                        "Response body too large: {} bytes (max {})",
                        b.len(),
                        MAX_RESPONSE_SIZE
                    ),
                };
            }
            b.to_vec()
        }
        Err(e) => {
            return FetchResult::Error {
                message: format!("Failed to read response body: {}", e),
            }
        }
    };

    FetchResult::Updated {
        body,
        etag: new_etag,
        last_modified: new_last_modified,
    }
}
