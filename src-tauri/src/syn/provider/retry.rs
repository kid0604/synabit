//! Asking again when a provider stumbles, and never when it has already spoken.
//!
//! # Why this exists
//!
//! Until now a single 429 ended the run. Every provider sent its request once
//! and handed any failure straight up through `?`, so a rate limit that would
//! have cleared in two seconds, a 529 while Anthropic was busy, a 503 from a
//! Gemini region having a bad minute, or a connection reset on a laptop that
//! just changed Wi-Fi all looked, to the person asking, exactly like Syn being
//! broken. Hosted APIs say plainly that these are transient and that the
//! client is expected to try again; the official SDKs all do, twice, with
//! backoff. Syn did not, anywhere.
//!
//! # What is retried, and what never is
//!
//! **Only the request, never the stream.** A retry happens while nothing has
//! reached the sink: a status or a connection failure arrives before any of
//! the body is read, so nobody has seen a word. Once text is flowing, a broken
//! stream is *not* retried — asking again would put the first half of the
//! answer on screen twice, and a stream that dies halfway is better reported
//! than silently duplicated. That boundary is structural rather than a flag
//! somebody has to remember: this module wraps the send, and the providers read
//! the body only after it returns.
//!
//! **Only what the server calls temporary.** 408, 429, 500, 502, 503, 504 and
//! Anthropic's 529 "overloaded" — plus a request that never got an answer at
//! all: refused or timed-out connection, reset. Everything else in the 4xx range
//! is the request's own fault — a bad key, a model that does not exist, a
//! schema the server will not take — and sending the same bytes again gets
//! the same answer three times slower. Several callers depend on that: the
//! timeline reader and the OpenAI provider both react to a 400 by changing the
//! request, and they need to hear about it at once.
//!
//! **Three attempts, about 1s then 2s apart, each ±25%.** The jitter is what
//! stops two runs that hit the same rate limit together from retrying together
//! and hitting it again. When the server says how long to wait (`Retry-After`,
//! or OpenAI's `retry-after-ms`) its word wins over the schedule — it knows
//! when the window reopens and the schedule is a guess — up to twenty seconds,
//! because a person is watching a spinner and a minute of silence reads as a
//! hang.
//!
//! # Stopping
//!
//! Pressing stop has to work during the wait, and during the request itself.
//! The wait is slept in 100 ms slices with the stop flag read between them, and
//! each attempt races the same flag, so a request that is taking its time is
//! dropped — which closes the connection — rather than waited out. A stopped
//! send answers `Ok(None)`: not an error, because the person asked for it, and
//! the engine already treats an empty reply after a stop as a clean
//! cancellation.

use std::future::Future;
use std::time::Duration;

use crate::error::AppError;

/// How many times one request is sent, the first included.
pub const ATTEMPTS: u32 = 3;

/// The first wait. Each one after doubles it.
pub const FIRST_WAIT: Duration = Duration::from_secs(1);

/// The longest this will wait between two attempts, whatever the server says.
pub const LONGEST_WAIT: Duration = Duration::from_secs(20);

/// How much a wait may stray either side of the schedule, as a fraction.
pub const JITTER: f64 = 0.25;

/// How often the stop flag is read while waiting.
const SLICE: Duration = Duration::from_millis(100);

/// The stop flag for a call that has none to consult.
///
/// `chat` is not given one, so it retries without being stoppable. See
/// `ChatProvider::chat_stoppable`.
pub fn never() -> bool {
    false
}

/// Whether a status is the server saying "not now" rather than "no".
pub fn worth_retrying(status: u16) -> bool {
    matches!(status, 408 | 429 | 500 | 502 | 503 | 504 | 529)
}

/// Whether a request that got no answer at all is worth sending again.
///
/// A failed connect is the obvious one, a connect that timed out included.
/// `is_request` is the rest of "the bytes did not make it" — a reset mid-send
/// is reported that way. A builder error or a redirect loop is not here: those
/// fail the same way every time.
///
/// **A timeout on the whole request is not retried.** The chat client waits
/// five minutes, because a large local model can genuinely take that long;
/// running out of five minutes means the model is too slow for the question,
/// and asking twice more would turn a five-minute failure into a fifteen-minute
/// one with the same ending.
pub fn transport_worth_retrying(e: &reqwest::Error) -> bool {
    e.is_connect() || (e.is_request() && !e.is_timeout())
}

/// The wait after `failed` attempts have failed, by the schedule alone.
///
/// `jitter` is in `[-1, 1]` and moves the wait by up to `JITTER` of itself
/// either way; a caller passes a random one, a test passes the edges.
pub fn backoff(failed: u32, jitter: f64) -> Duration {
    let doublings = failed.saturating_sub(1).min(16);
    let base = FIRST_WAIT.as_secs_f64() * f64::from(1u32 << doublings);
    let spread = 1.0 + JITTER * jitter.clamp(-1.0, 1.0);
    Duration::from_secs_f64(base * spread).min(LONGEST_WAIT)
}

/// The wait before the next attempt: the server's word if it gave one, else
/// the schedule — and never longer than `LONGEST_WAIT`.
pub fn wait_before_retry(failed: u32, told: Option<Duration>, jitter: f64) -> Duration {
    match told {
        Some(told) => told.min(LONGEST_WAIT),
        None => backoff(failed, jitter),
    }
}

/// A random jitter in `[-1, 1]`, for `backoff`.
pub fn jitter() -> f64 {
    rand::random::<f64>() * 2.0 - 1.0
}

/// How long the server asked for, from `Retry-After` or `retry-after-ms`.
///
/// `Retry-After` is either whole seconds or an HTTP date; both are read. The
/// millisecond header is OpenAI's, and more precise when both are sent.
pub fn told_to_wait(headers: &reqwest::header::HeaderMap) -> Option<Duration> {
    let read = |name: &str| headers.get(name).and_then(|v| v.to_str().ok());
    if let Some(ms) = read("retry-after-ms").and_then(|v| v.trim().parse::<f64>().ok()) {
        if ms.is_finite() && ms >= 0.0 {
            return Some(Duration::from_secs_f64(ms / 1000.0));
        }
    }
    read("retry-after").and_then(|v| parse_retry_after(v, chrono::Utc::now()))
}

/// One `Retry-After` value, read against `now`.
pub fn parse_retry_after(value: &str, now: chrono::DateTime<chrono::Utc>) -> Option<Duration> {
    let value = value.trim();
    if let Ok(secs) = value.parse::<f64>() {
        return (secs.is_finite() && secs >= 0.0).then(|| Duration::from_secs_f64(secs));
    }
    let when = chrono::DateTime::parse_from_rfc2822(value).ok()?;
    // A date already past means "now", not "never".
    Some((when.with_timezone(&chrono::Utc) - now).to_std().unwrap_or(Duration::ZERO))
}

/// What one attempt came to.
pub enum Outcome<T> {
    /// It worked.
    Done(T),
    /// It failed in a way that may not happen again. `told` is how long the
    /// server asked to be left alone, if it said.
    Again { error: AppError, told: Option<Duration> },
    /// It failed in a way that will.
    Fail(AppError),
}

/// Run `fut`, unless the stop flag goes up first.
///
/// Dropping the future is the cancellation: an in-flight reqwest request that
/// is dropped closes its connection, so the provider stops generating — and
/// charging — for an answer nobody is waiting for.
pub async fn unless_stopped<F: Future>(stop: &(dyn Fn() -> bool + Send + Sync), fut: F) -> Option<F::Output> {
    if stop() {
        return None;
    }
    tokio::select! {
        out = fut => Some(out),
        _ = stopped(stop) => None,
    }
}

async fn stopped(stop: &(dyn Fn() -> bool + Send + Sync)) {
    loop {
        tokio::time::sleep(SLICE).await;
        if stop() {
            return;
        }
    }
}

/// Sleep for `wait`, a slice at a time. False if stopped before the end.
pub async fn pause(wait: Duration, stop: &(dyn Fn() -> bool + Send + Sync)) -> bool {
    let until = tokio::time::Instant::now() + wait;
    loop {
        if stop() {
            return false;
        }
        let now = tokio::time::Instant::now();
        if now >= until {
            return true;
        }
        tokio::time::sleep(SLICE.min(until - now)).await;
    }
}

/// Make attempts until one is final, the attempts run out, or stop is pressed.
///
/// `Ok(None)` is stopped. The last error is the one reported when every
/// attempt failed, because it is the most recent thing the server said.
pub async fn with_retry<T, F, Fut>(
    what: &str,
    stop: &(dyn Fn() -> bool + Send + Sync),
    mut attempt: F,
) -> Result<Option<T>, AppError>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Outcome<T>>,
{
    let mut failed = 0;
    loop {
        let Some(outcome) = unless_stopped(stop, attempt()).await else {
            return Ok(None);
        };
        match outcome {
            Outcome::Done(value) => return Ok(Some(value)),
            Outcome::Fail(error) => return Err(error),
            Outcome::Again { error, told } => {
                failed += 1;
                if failed >= ATTEMPTS {
                    return Err(error);
                }
                let jitter = jitter();
                let wait = wait_before_retry(failed, told, jitter);
                log::info!(
                    "[Syn] {what} failed ({error}); trying again in {:.1}s ({} of {ATTEMPTS})",
                    wait.as_secs_f64(),
                    failed + 1
                );
                if !pause(wait, stop).await {
                    return Ok(None);
                }
            }
        }
    }
}

/// What one plain HTTP send came to, sorted for `with_retry`.
///
/// For the providers whose only reaction to a failure is to report it —
/// Gemini, Anthropic, Ollama. `unreachable` words a request that got no answer
/// and `refused` words a non-2xx, each in the provider's own terms.
pub async fn classify(
    sent: Result<reqwest::Response, reqwest::Error>,
    unreachable: &(dyn Fn(reqwest::Error) -> AppError + Send + Sync),
    refused: &(dyn Fn(reqwest::StatusCode, &str) -> AppError + Send + Sync),
) -> Outcome<reqwest::Response> {
    match sent {
        Ok(resp) if resp.status().is_success() => Outcome::Done(resp),
        Ok(resp) => {
            let status = resp.status();
            let told = told_to_wait(resp.headers());
            let body = resp.text().await.unwrap_or_default();
            let error = refused(status, &body);
            if worth_retrying(status.as_u16()) {
                Outcome::Again { error, told }
            } else {
                Outcome::Fail(error)
            }
        }
        Err(e) if transport_worth_retrying(&e) => Outcome::Again { error: unreachable(e), told: None },
        Err(e) => Outcome::Fail(unreachable(e)),
    }
}

/// Send what `build` makes, retrying as this module describes.
///
/// `build` is called once per attempt because a `RequestBuilder` is spent by
/// sending it.
pub async fn send(
    what: &str,
    stop: &(dyn Fn() -> bool + Send + Sync),
    build: &(dyn Fn() -> reqwest::RequestBuilder + Send + Sync),
    unreachable: &(dyn Fn(reqwest::Error) -> AppError + Send + Sync),
    refused: &(dyn Fn(reqwest::StatusCode, &str) -> AppError + Send + Sync),
) -> Result<Option<reqwest::Response>, AppError> {
    with_retry(what, stop, || {
        let request = build();
        async move { classify(request.send().await, unreachable, refused).await }
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    /// The temporary statuses are retried, and the request's own mistakes are
    /// not: a bad key sent three times is three 401s and a slower error.
    #[test]
    fn only_a_server_saying_not_now_is_asked_again() {
        for status in [408, 429, 500, 502, 503, 504, 529] {
            assert!(worth_retrying(status), "{status} is temporary");
        }
        for status in [200, 400, 401, 402, 403, 404, 409, 413, 422, 501] {
            assert!(!worth_retrying(status), "{status} will say the same again");
        }
    }

    /// One second, then two, then four — each within a quarter either way.
    #[test]
    fn the_schedule_doubles_and_strays_a_quarter_either_way() {
        assert_eq!(backoff(1, 0.0), Duration::from_secs(1));
        assert_eq!(backoff(2, 0.0), Duration::from_secs(2));
        assert_eq!(backoff(3, 0.0), Duration::from_secs(4));

        assert_eq!(backoff(1, -1.0), Duration::from_millis(750));
        assert_eq!(backoff(1, 1.0), Duration::from_millis(1250));
        assert_eq!(backoff(2, 1.0), Duration::from_millis(2500));

        // A jitter out of range is held to the edge, not multiplied through.
        assert_eq!(backoff(1, 9.0), Duration::from_millis(1250));
    }

    /// However many failures, the wait stays something a person will sit
    /// through.
    #[test]
    fn the_schedule_never_outgrows_the_ceiling() {
        assert_eq!(backoff(40, 1.0), LONGEST_WAIT);
    }

    /// The server knows when its window reopens; the schedule is a guess. But
    /// a server asking for a minute gets twenty seconds, not a frozen screen.
    #[test]
    fn the_servers_word_wins_up_to_the_ceiling() {
        assert_eq!(wait_before_retry(1, Some(Duration::from_secs(7)), 1.0), Duration::from_secs(7));
        assert_eq!(wait_before_retry(1, Some(Duration::from_secs(90)), 0.0), LONGEST_WAIT);
        assert_eq!(wait_before_retry(2, None, 0.0), Duration::from_secs(2));
    }

    #[test]
    fn retry_after_is_read_as_seconds_or_as_a_date() {
        let now = chrono::DateTime::parse_from_rfc3339("2026-09-26T10:00:00Z").unwrap().with_timezone(&chrono::Utc);
        assert_eq!(parse_retry_after("3", now), Some(Duration::from_secs(3)));
        assert_eq!(parse_retry_after(" 1.5 ", now), Some(Duration::from_millis(1500)));
        assert_eq!(
            parse_retry_after("Sat, 26 Sep 2026 10:00:12 GMT", now),
            Some(Duration::from_secs(12))
        );
        assert_eq!(parse_retry_after("Sat, 26 Sep 2026 09:00:00 GMT", now), Some(Duration::ZERO), "past is now");
        assert_eq!(parse_retry_after("soon", now), None);
        assert_eq!(parse_retry_after("-4", now), None);
    }

    #[test]
    fn the_millisecond_header_is_preferred_when_both_are_sent() {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert("retry-after", "3".parse().unwrap());
        headers.insert("retry-after-ms", "250".parse().unwrap());
        assert_eq!(told_to_wait(&headers), Some(Duration::from_millis(250)));

        headers.remove("retry-after-ms");
        assert_eq!(told_to_wait(&headers), Some(Duration::from_secs(3)));
        assert_eq!(told_to_wait(&reqwest::header::HeaderMap::new()), None);
    }

    /// Nothing listening is worth another try — Ollama still starting, a
    /// laptop between networks. A five-minute generation that ran out of time
    /// is not; see `transport_worth_retrying`.
    #[tokio::test]
    async fn a_refused_connection_is_retried_and_a_whole_request_timeout_is_not() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let closed = listener.local_addr().unwrap();
        drop(listener);
        let refused = reqwest::Client::new().get(format!("http://{closed}/")).send().await.unwrap_err();
        assert!(transport_worth_retrying(&refused), "{refused:?}");

        // Accepts, then says nothing: the request, not the connect, times out.
        let silent = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = silent.local_addr().unwrap();
        tokio::spawn(async move {
            let (_socket, _) = silent.accept().await.unwrap();
            tokio::time::sleep(Duration::from_secs(5)).await;
        });
        let slow = reqwest::Client::builder()
            .timeout(Duration::from_millis(200))
            .build()
            .unwrap()
            .get(format!("http://{addr}/"))
            .send()
            .await
            .unwrap_err();
        assert!(slow.is_timeout());
        assert!(!transport_worth_retrying(&slow), "{slow:?}");
    }

    fn again(n: u32) -> Outcome<u32> {
        Outcome::Again { error: AppError::General(format!("busy {n}")), told: Some(Duration::ZERO) }
    }

    /// Three attempts and then the last thing the server said.
    #[tokio::test]
    async fn it_gives_up_after_three_and_says_the_last_failure() {
        let made = AtomicU32::new(0);
        let out = with_retry("test", &never, || {
            let n = made.fetch_add(1, Ordering::SeqCst) + 1;
            async move { again(n) }
        })
        .await;
        assert_eq!(made.load(Ordering::SeqCst), ATTEMPTS);
        assert!(out.unwrap_err().to_string().contains("busy 3"));
    }

    #[tokio::test]
    async fn a_success_after_a_stumble_is_a_success() {
        let made = AtomicU32::new(0);
        let out = with_retry("test", &never, || {
            let n = made.fetch_add(1, Ordering::SeqCst) + 1;
            async move { if n < 2 { again(n) } else { Outcome::Done(n) } }
        })
        .await;
        assert_eq!(out.unwrap(), Some(2));
    }

    /// A final failure is reported at once, not three times.
    #[tokio::test]
    async fn a_refusal_is_not_asked_again() {
        let made = AtomicU32::new(0);
        let out: Result<Option<u32>, _> = with_retry("test", &never, || {
            made.fetch_add(1, Ordering::SeqCst);
            async { Outcome::Fail(AppError::General("401".into())) }
        })
        .await;
        assert!(out.is_err());
        assert_eq!(made.load(Ordering::SeqCst), 1);
    }

    /// Stop pressed during the wait ends the wait, and ends it as a stop — not
    /// an error the person would read as something going wrong.
    #[tokio::test]
    async fn stop_during_the_wait_ends_it_as_a_stop() {
        let made = AtomicU32::new(0);
        let started = std::time::Instant::now();
        let stop = || made.load(Ordering::SeqCst) >= 1;
        let out = with_retry("test", &stop, || {
            made.fetch_add(1, Ordering::SeqCst);
            async { Outcome::<u32>::Again { error: AppError::General("busy".into()), told: Some(Duration::from_secs(10)) } }
        })
        .await;
        assert_eq!(out.unwrap(), None);
        assert_eq!(made.load(Ordering::SeqCst), 1);
        assert!(started.elapsed() < Duration::from_secs(2), "the ten-second wait was not sat through");
    }

    /// And during the request itself: a request taking its time is dropped.
    #[tokio::test]
    async fn stop_during_a_request_drops_it() {
        let flag = std::sync::atomic::AtomicBool::new(false);
        let stop = || flag.load(Ordering::SeqCst);
        let started = std::time::Instant::now();
        let out = unless_stopped(&stop, async {
            flag.store(true, Ordering::SeqCst);
            tokio::time::sleep(Duration::from_secs(30)).await;
            1
        })
        .await;
        assert_eq!(out, None);
        assert!(started.elapsed() < Duration::from_secs(2));
    }
}
