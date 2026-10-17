//! Where a Google Maps short link leads.
//!
//! The Share button in Google Maps hands out `maps.app.goo.gl/…`, and a link
//! like that carries no coordinates — they are in the address it redirects
//! to. The webview cannot find that out: the redirect is cross-origin and
//! Google sends no CORS headers, so a `fetch` from script sees only an opaque
//! failure. Here the redirect is read, not followed, and the address it names
//! is handed back for the editor to parse.
//!
//! Only Google's short-link hosts are asked, and only over https, so this
//! cannot be pointed at anything else on the network.

use crate::error::{AppError, AppResult};

const SHORT_HOSTS: &[&str] = &["maps.app.goo.gl", "goo.gl"];
const TIMEOUT: std::time::Duration = std::time::Duration::from_secs(8);
/// `goo.gl/maps/…` has been seen to go through `maps.app.goo.gl` first.
const MAX_HOPS: usize = 3;

fn is_short(url: &reqwest::Url) -> bool {
    url.scheme() == "https" && url.host_str().is_some_and(|h| SHORT_HOSTS.contains(&h))
}

#[tauri::command]
pub async fn resolve_map_link(url: String) -> AppResult<String> {
    let mut current = reqwest::Url::parse(url.trim())
        .map_err(|e| AppError::General(format!("Not a link: {e}")))?;
    if !is_short(&current) {
        return Err(AppError::General("Not a Google Maps short link".into()));
    }

    let client = reqwest::Client::builder()
        .timeout(TIMEOUT)
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| AppError::General(format!("Could not build an HTTP client: {e}")))?;

    for _ in 0..MAX_HOPS {
        let response = client
            .get(current.clone())
            .send()
            .await
            .map_err(|e| AppError::General(format!("Could not open the link: {e}")))?;
        let Some(location) = response
            .headers()
            .get(reqwest::header::LOCATION)
            .and_then(|v| v.to_str().ok())
        else {
            return Err(AppError::General(format!(
                "The link did not lead anywhere (HTTP {})",
                response.status()
            )));
        };
        let next = current
            .join(location)
            .map_err(|e| AppError::General(format!("The link led to a bad address: {e}")))?;
        if !is_short(&next) {
            return Ok(next.to_string());
        }
        current = next;
    }
    Err(AppError::General("The link redirected too many times".into()))
}
