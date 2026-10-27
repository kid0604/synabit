//! Carrying out a declared operation over HTTP.
//!
//! The executor for `transport: http`. What it may do is fixed here, not in
//! the declaration: `docs/syn-connectors-2026-10-08.md` §3.1–3.2.
//!
//! - **The host is the declaration's.** A value is only ever placed in a path
//!   segment, a query value, a header value or a body, encoded for where it
//!   goes; a path value that decodes to `/`, `..`, `@`, `#`, `?` or `\` is
//!   refused, and the finished address must still be on the declared host.
//! - **No redirect is followed.** An answer that points elsewhere is an answer.
//! - **A public host stays public.** On a `Public` instance the client is handed
//!   only public addresses (`PublicOnly`), so a name that later resolves to this
//!   machine or the local network is refused rather than reached.
//! - **A certificate nobody vouches for is accepted only as pinned.** No switch
//!   turns certificate checking off.
//! - **The secret goes where the declaration put it**, is never in the query
//!   the run records, and is scrubbed from the answer if the server echoes it.
//! - **Paging, retries and size have ceilings**, and reaching one is reported.

use std::sync::Arc;
use std::time::Duration;

use serde_json::Value;

use super::instance::{Instance, Network};
use super::manifest::{Auth, Manifest, Operation, PageStyle};
use crate::syn::resultset::Column;

/// One request.
const TIMEOUT: Duration = Duration::from_secs(30);
/// One page of an answer.
const PAGE_BYTES: usize = 4 * 1024 * 1024;
/// Every page of one call together.
const TOTAL_BYTES: usize = 20 * 1024 * 1024;
/// Times a `429` or `503` is waited out and asked again.
const RETRIES: u32 = 3;
/// The longest a `Retry-After` is honoured.
const LONGEST_WAIT_S: i64 = 30;

/// Why a call came to nothing, and whether the server's words are in it.
#[derive(Debug)]
pub struct Failed {
    pub said: String,
    pub answered: bool,
    /// What kind of failure, for the connect card to say in the person's
    /// words: `credential_rejected`, `unreachable`, `self_signed`,
    /// `not_found`, `rate_limited`, `server_error`, `shape_changed`.
    pub code: Option<&'static str>,
}

impl From<String> for Failed {
    fn from(said: String) -> Self {
        let code = Some(code_of_error(&said));
        Failed { said, answered: false, code }
    }
}

/// A failure to reach a server, by what the client said.
pub fn code_of_error(said: &str) -> &'static str {
    let s = said.to_lowercase();
    if s.contains("certificate") || s.contains("unknownissuer") || s.contains("invalid peer") || s.contains("self signed") || s.contains("self-signed") {
        "self_signed"
    } else {
        "unreachable"
    }
}

/// A server's answer that is not a success, by its status.
pub fn code_of_status(status: u16) -> &'static str {
    match status {
        401 | 403 => "credential_rejected",
        404 | 405 | 410 => "not_found",
        429 => "rate_limited",
        _ => "server_error",
    }
}

/// What one call brought back.
#[derive(Debug)]
pub struct Fetched {
    pub columns: Vec<Column>,
    pub rows: Vec<Vec<Value>>,
    pub missing: Vec<String>,
    /// What was asked, as the run records it — without the secret.
    pub asked: String,
}

/// A request ready to send.
#[derive(Debug, Clone, PartialEq)]
pub struct Prepared {
    pub method: reqwest::Method,
    pub url: url::Url,
    pub headers: Vec<(String, String)>,
    pub body: Option<Body>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Body {
    Json(Value),
    Form(Vec<(String, String)>),
    /// As written, with its content type.
    Raw(String, String),
}

/// `<`, `&` and quotes as XML text.
fn xml_escaped(v: &str) -> String {
    v.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;").replace('\'', "&apos;")
}

/// A value from the arguments, as text. `None` for one not given.
fn arg_text(args: &Value, name: &str) -> Option<String> {
    match args.get(name)? {
        Value::Null => None,
        Value::String(s) => Some(s.clone()),
        // A list of plain values in a query, a path or a header is the comma
        // list APIs read — `symbols=USD,JPY` — not its JSON. The 2026-10-26
        // eval sent `symbols=["USD"]` and Frankfurter answered 404.
        Value::Array(items) if items.iter().all(|v| !v.is_array() && !v.is_object()) => Some(
            items
                .iter()
                .filter(|v| !v.is_null())
                .map(|v| v.as_str().map(str::to_string).unwrap_or_else(|| v.to_string()))
                .collect::<Vec<_>>()
                .join(","),
        ),
        other => Some(other.to_string()),
    }
}

/// Fill `{{name}}` from the arguments.
fn fill(template: &str, args: &Value, extra: &[(&str, String)]) -> Result<String, String> {
    let mut out = String::new();
    let mut rest = template;
    while let Some(start) = rest.find("{{") {
        out.push_str(&rest[..start]);
        let end = rest[start + 2..].find("}}").ok_or("an unclosed {{")?;
        let name = rest[start + 2..start + 2 + end].trim();
        let value = extra
            .iter()
            .find(|(k, _)| *k == name)
            .map(|(_, v)| v.clone())
            .or_else(|| arg_text(args, name))
            .unwrap_or_default();
        if value.contains(['\r', '\n']) {
            return Err(format!("`{name}` cannot contain a line break"));
        }
        out.push_str(&value);
        rest = &rest[start + 2 + end + 2..];
    }
    out.push_str(rest);
    Ok(out)
}

/// A template that is nothing but one placeholder, whose value was not given:
/// the key is left out rather than sent empty.
fn absent(template: &str, args: &Value, extra: &[(&str, String)]) -> bool {
    let t = template.trim();
    t.starts_with("{{") && t.ends_with("}}") && t.matches("{{").count() == 1 && {
        let name = t[2..t.len() - 2].trim();
        !extra.iter().any(|(k, _)| *k == name) && arg_text(args, name).is_none()
    }
}

/// A JSON body with its strings filled; a string that is only a placeholder
/// takes the argument's own type, so a number stays a number.
fn fill_json(v: &Value, args: &Value) -> Result<Value, String> {
    Ok(match v {
        Value::String(s) => {
            let t = s.trim();
            if t.starts_with("{{") && t.ends_with("}}") && t.matches("{{").count() == 1 {
                args.get(t[2..t.len() - 2].trim()).cloned().unwrap_or(Value::Null)
            } else {
                Value::String(fill(s, args, &[])?)
            }
        }
        Value::Array(a) => Value::Array(a.iter().map(|x| fill_json(x, args)).collect::<Result<_, _>>()?),
        Value::Object(o) => Value::Object(o.iter().map(|(k, x)| Ok((k.clone(), fill_json(x, args)?))).collect::<Result<_, String>>()?),
        other => other.clone(),
    })
}

/// Every string in the arguments, XML-escaped.
fn escape_strings(args: &Value) -> Value {
    match args {
        Value::String(s) => Value::String(xml_escaped(s)),
        Value::Array(a) => Value::Array(a.iter().map(escape_strings).collect()),
        Value::Object(o) => Value::Object(o.iter().map(|(k, v)| (k.clone(), escape_strings(v))).collect()),
        other => other.clone(),
    }
}

/// A raw body's placeholders filled. Line breaks are a body's own, so they
/// are allowed here, unlike in a header or the address.
fn fill_raw(template: &str, args: &Value) -> Result<String, String> {
    let mut out = String::new();
    let mut rest = template;
    while let Some(start) = rest.find("{{") {
        out.push_str(&rest[..start]);
        let end = rest[start + 2..].find("}}").ok_or("an unclosed {{ in the body")?;
        let name = rest[start + 2..start + 2 + end].trim();
        out.push_str(&arg_text(args, name).unwrap_or_default());
        rest = &rest[start + 2 + end + 2..];
    }
    out.push_str(rest);
    Ok(out)
}

/// The path, each placeholder encoded as one segment and refused if it would
/// be more than one.
fn fill_path(template: &str, args: &Value) -> Result<String, String> {
    let mut out = String::new();
    let mut rest = template;
    while let Some(start) = rest.find("{{") {
        out.push_str(&rest[..start]);
        let end = rest[start + 2..].find("}}").ok_or("an unclosed {{ in the path")?;
        let name = rest[start + 2..start + 2 + end].trim();
        let value = arg_text(args, name).ok_or_else(|| format!("`{name}` is needed"))?;
        let decoded = urlencoding::decode(&value).map(|c| c.into_owned()).unwrap_or_else(|_| value.clone());
        if value.is_empty()
            || [&value, &decoded].iter().any(|v| v.contains(['/', '\\', '@', '#', '?']) || v.split('/').any(|s| s == "..") || v.as_str() == "..")
        {
            return Err(format!("`{name}` cannot be used in the path: \"{value}\""));
        }
        out.push_str(&urlencoding::encode(&value));
        rest = &rest[start + 2 + end + 2..];
    }
    out.push_str(rest);
    Ok(out)
}

/// Build the request for one operation and one page.
pub fn prepare(m: &Manifest, op: &Operation, args: &Value, secret: Option<&str>, page: &[(&str, String)]) -> Result<Prepared, String> {
    for (name, p) in &op.params {
        if p.required && arg_text(args, name).is_none() {
            return Err(format!("`{name}` is required"));
        }
        if !p.choices.is_empty() {
            if let Some(v) = arg_text(args, name) {
                if !p.choices.contains(&v) {
                    return Err(format!("`{name}` must be one of {}", p.choices.join(", ")));
                }
            }
        }
    }
    let base = url::Url::parse(&m.base_url).map_err(|e| format!("base_url: {e}"))?;
    let path = fill_path(&op.call.path, args)?;
    let joined = format!("{}{}", m.base_url.trim_end_matches('/'), path);
    let mut url = url::Url::parse(&joined).map_err(|e| format!("the address: {e}"))?;
    if url.host_str() != base.host_str() || url.port_or_known_default() != base.port_or_known_default() || url.scheme() != base.scheme() {
        return Err("the address left the declared host".into());
    }
    {
        let mut q = url.query_pairs_mut();
        for (k, v) in &op.call.query {
            if absent(v, args, page) {
                continue;
            }
            q.append_pair(k, &fill(v, args, page)?);
        }
        if let (Auth::Query { name, .. }, Some(s)) = (&m.auth, secret) {
            q.append_pair(name, s);
        }
    }
    if url.query() == Some("") {
        url.set_query(None);
    }

    let mut headers = Vec::new();
    for (k, v) in &op.call.headers {
        if absent(v, args, &[]) {
            continue;
        }
        headers.push((k.clone(), fill(v, args, &[])?));
    }
    match (&m.auth, secret) {
        (Auth::Bearer { .. }, Some(s)) => headers.push(("Authorization".into(), format!("Bearer {s}"))),
        (Auth::Header { name, .. }, Some(s)) => headers.push((name.clone(), s.to_string())),
        (Auth::Basic { user, .. }, Some(s)) => {
            use base64::Engine;
            headers.push(("Authorization".into(), format!("Basic {}", base64::engine::general_purpose::STANDARD.encode(format!("{user}:{s}")))));
        }
        // An ssh login is never a header; `problems` keeps it off http.
        (Auth::None | Auth::Query { .. } | Auth::Ssh { .. }, _) => {}
        (_, None) => return Err(format!("{} needs its secret, and this device does not have it", m.name)),
    }
    if headers.iter().any(|(_, v)| v.contains(['\r', '\n'])) {
        return Err("a header value cannot contain a line break".into());
    }

    let body = match (&op.call.json, op.call.form.is_empty()) {
        _ if op.call.body.is_some() => {
            let raw = op.call.body.as_deref().unwrap_or_default();
            let is_xml = raw.trim_start().starts_with('<');
            let args = if is_xml { escape_strings(args) } else { args.clone() };
            let given = op.call.headers.iter().find(|(k, _)| k.eq_ignore_ascii_case("content-type")).map(|(_, v)| v.clone());
            let kind = given.unwrap_or_else(|| if is_xml { "application/xml; charset=utf-8".into() } else { "text/plain; charset=utf-8".into() });
            Some(Body::Raw(fill_raw(raw, &args)?, kind))
        }
        (Some(j), _) => Some(Body::Json(fill_json(j, args)?)),
        (None, false) => Some(Body::Form(
            op.call
                .form
                .iter()
                .filter(|(_, v)| !absent(v, args, &[]))
                .map(|(k, v)| Ok((k.clone(), fill(v, args, &[])?)))
                .collect::<Result<_, String>>()?,
        )),
        (None, true) => None,
    };
    let method = reqwest::Method::from_bytes(op.call.method.to_ascii_uppercase().as_bytes()).map_err(|e| e.to_string())?;
    Ok(Prepared { method, url, headers, body })
}

/// What the run records as asked: the method and the address without the
/// secret, and the body.
pub fn asked(p: &Prepared, m: &Manifest) -> String {
    let mut url = p.url.clone();
    if let Auth::Query { name, .. } = &m.auth {
        let kept: Vec<(String, String)> = url.query_pairs().filter(|(k, _)| k != name).map(|(k, v)| (k.into_owned(), v.into_owned())).collect();
        url.set_query(None);
        if !kept.is_empty() {
            url.query_pairs_mut().extend_pairs(kept);
        }
    }
    match &p.body {
        Some(Body::Json(j)) => format!("{} {} {j}", p.method, url),
        Some(Body::Form(f)) => format!("{} {} {}", p.method, url, f.iter().map(|(k, v)| format!("{k}={v}")).collect::<Vec<_>>().join("&")),
        Some(Body::Raw(b, _)) => format!("{} {} {}", p.method, url, b.chars().take(2_000).collect::<String>()),
        None => format!("{} {}", p.method, url),
    }
}

// ═══════════════════════════════════════════════════════════════
//  CERTIFICATES
// ═══════════════════════════════════════════════════════════════

/// The fingerprint of a certificate as the approval card shows and keeps it:
/// SHA-256 of the leaf certificate, base64.
pub fn cert_fingerprint(der: &[u8]) -> String {
    use base64::Engine;
    use sha2::{Digest, Sha256};
    base64::engine::general_purpose::STANDARD.encode(Sha256::digest(der))
}

/// Accepts exactly the pinned certificate, and nothing else — or, to read a
/// certificate for the card, anything, keeping its fingerprint and sending no
/// request after the handshake.
#[derive(Debug)]
struct Pinned {
    pin: Option<String>,
    seen: std::sync::Mutex<Option<String>>,
    algorithms: rustls::crypto::WebPkiSupportedAlgorithms,
}

impl rustls::client::danger::ServerCertVerifier for Pinned {
    fn verify_server_cert(
        &self,
        end_entity: &rustls::pki_types::CertificateDer<'_>,
        _intermediates: &[rustls::pki_types::CertificateDer<'_>],
        _server_name: &rustls::pki_types::ServerName<'_>,
        _ocsp: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        let got = cert_fingerprint(end_entity.as_ref());
        if let Ok(mut seen) = self.seen.lock() {
            *seen = Some(got.clone());
        }
        match &self.pin {
            Some(pin) if *pin == got => Ok(rustls::client::danger::ServerCertVerified::assertion()),
            Some(_) => Err(rustls::Error::General("the certificate is not the one that was approved".into())),
            // Reading only: the handshake is let through so the card can show
            // what it is, and `peek_certificate` sends nothing after it.
            None => Ok(rustls::client::danger::ServerCertVerified::assertion()),
        }
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &rustls::pki_types::CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(message, cert, dss, &self.algorithms)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &rustls::pki_types::CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(message, cert, dss, &self.algorithms)
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        self.algorithms.supported_schemes()
    }
}

fn pinned_config(pin: Option<String>) -> (rustls::ClientConfig, Arc<Pinned>) {
    let provider = rustls::crypto::ring::default_provider();
    let verifier = Arc::new(Pinned { pin, seen: std::sync::Mutex::new(None), algorithms: provider.signature_verification_algorithms });
    let config = rustls::ClientConfig::builder_with_provider(Arc::new(provider))
        .with_safe_default_protocol_versions()
        .expect("the default protocol versions")
        .dangerous()
        .with_custom_certificate_verifier(verifier.clone())
        .with_no_client_auth();
    (config, verifier)
}

fn client(inst: &Instance) -> Result<reqwest::Client, String> {
    let mut b = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(TIMEOUT)
        .connect_timeout(Duration::from_secs(10))
        .user_agent(concat!("Synabit/", env!("CARGO_PKG_VERSION")));
    if inst.network == Network::Public {
        b = b.dns_resolver(Arc::new(crate::feed_engine::fetcher::PublicOnly));
    }
    if let Some(pin) = &inst.pinned_cert {
        b = b.use_preconfigured_tls(pinned_config(Some(pin.clone())).0);
    }
    b.build().map_err(|e| e.to_string())
}

/// The fingerprint of the certificate `url` presents, for the approval card,
/// without trusting it and without sending a request after the handshake.
pub async fn peek_certificate(url: &str) -> Result<String, String> {
    let (config, verifier) = pinned_config(None);
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(10))
        .use_preconfigured_tls(config)
        .build()
        .map_err(|e| e.to_string())?;
    // A HEAD, so that if anything is sent at all it asks for nothing. The answer is ignored.
    let _ = client.head(url).send().await;
    let seen = verifier.seen.lock().map_err(|e| e.to_string())?.clone();
    seen.ok_or_else(|| format!("{url} presented no certificate"))
}

// ═══════════════════════════════════════════════════════════════
//  SENDING
// ═══════════════════════════════════════════════════════════════

async fn send_once(client: &reqwest::Client, p: &Prepared) -> Result<(reqwest::StatusCode, reqwest::header::HeaderMap, Vec<u8>), String> {
    let mut retried = 0;
    loop {
        let mut req = client.request(p.method.clone(), p.url.clone());
        for (k, v) in &p.headers {
            req = req.header(k, v);
        }
        req = match &p.body {
            Some(Body::Json(j)) => req.json(j),
            Some(Body::Form(f)) => req.form(f),
            Some(Body::Raw(b, kind)) => {
                let req = if p.headers.iter().any(|(k, _)| k.eq_ignore_ascii_case("content-type")) { req } else { req.header("Content-Type", kind) };
                req.body(b.clone())
            }
            None => req,
        };
        let resp = req.send().await.map_err(|e| {
            if e.is_timeout() { "the server did not answer in time".to_string() } else { format!("could not reach the server: {e}") }
        })?;
        let status = resp.status();
        if (status.as_u16() == 429 || status.as_u16() == 503) && retried < RETRIES {
            let wait = crate::feed_engine::fetcher::retry_after_seconds(resp.headers()).unwrap_or(2_i64.pow(retried + 1)).clamp(1, LONGEST_WAIT_S);
            retried += 1;
            tokio::time::sleep(Duration::from_secs(wait as u64)).await;
            continue;
        }
        let headers = resp.headers().clone();
        let body = crate::feed_engine::fetcher::read_capped(resp, PAGE_BYTES).await?;
        return Ok((status, headers, body));
    }
}

/// The next page's address from a `Link` header, if it is on the same host.
fn next_link(headers: &reqwest::header::HeaderMap, base: &url::Url) -> Option<url::Url> {
    let raw = headers.get(reqwest::header::LINK)?.to_str().ok()?;
    raw.split(',').find_map(|part| {
        let (target, params) = part.split_once(';')?;
        params.contains("rel=\"next\"").then(|| target.trim().trim_start_matches('<').trim_end_matches('>').to_string())
    })
    .and_then(|u| base.join(&u).ok())
    .filter(|u| u.host_str() == base.host_str() && u.scheme() == base.scheme())
}

/// Carry out `op` by its connector's transport: a request, or a bridge.
pub async fn fetch(m: &Manifest, inst: &Instance, op: &Operation, args: &Value, secret: Option<&str>) -> Result<Fetched, Failed> {
    // The person's account and address in place of `{{input.*}}`.
    let filled = m.with_inputs(&inst.inputs);
    let m = &filled;
    let op = &m.operations.iter().find(|o| o.name == op.name).cloned().unwrap_or_else(|| op.clone());
    match m.transport {
        super::manifest::Transport::Device => super::device::run(m, inst, op, args).await,
        super::manifest::Transport::Ssh => super::ssh::run(m, inst, op, args, secret).await,
        _ => run(m, inst, op, args, secret).await,
    }
}

/// Carry out `op` with `args`: every page up to the ceiling, made into one table.
pub async fn run(m: &Manifest, inst: &Instance, op: &Operation, args: &Value, secret: Option<&str>) -> Result<Fetched, Failed> {
    let base = url::Url::parse(&m.base_url).map_err(|e| e.to_string())?;
    if base.scheme() == "http" && inst.network == Network::Public {
        return Err(Failed::from(format!("{} is plain http on a public network; approve it as a local network, or use https", m.base_url)));
    }
    // `PublicOnly` judges names as it resolves them; an address written as an
    // IP, or `localhost`, is never resolved, so it is judged here.
    if inst.network == Network::Public {
        crate::feed_engine::fetcher::guard_url(&m.base_url)
            .map_err(|why| format!("{} is approved as a public host, and {why}. Approve it as a local network if that is what it is.", m.base_url))?;
    }
    let client = client(inst)?;
    let scrub = |text: &str| match secret {
        Some(s) if s.len() >= 4 => text.replace(s, "[hidden]"),
        _ => text.to_string(),
    };

    let mut columns: Vec<Column> = Vec::new();
    let mut rows: Vec<Vec<Value>> = Vec::new();
    let mut missing: Vec<String> = Vec::new();
    let mut total = 0usize;
    let mut first_asked = String::new();
    let mut page_arg: Vec<(&str, String)> = Vec::new();
    let mut link_next: Option<url::Url> = None;
    let max_pages = op.paginate.as_ref().map(|p| p.max_pages).unwrap_or(1);

    for page_no in 0..max_pages {
        let mut prepared = prepare(m, op, args, secret, &page_arg)?;
        if let Some(next) = link_next.take() {
            prepared.url = next;
        }
        if page_no == 0 {
            first_asked = asked(&prepared, m);
        }
        let (status, headers, body) = send_once(&client, &prepared).await?;
        total += body.len();
        let text = scrub(&String::from_utf8_lossy(&body));
        if !status.is_success() {
            let said: String = text.chars().take(2_000).collect();
            return Err(Failed { said: format!("{} answered {status}: {said}", m.name), answered: true, code: Some(code_of_status(status.as_u16())) });
        }
        let decoder = op.decoder.unwrap_or(m.decoder);
        let answer: Value = super::decode::decode(decoder, text.as_bytes()).map_err(|said| Failed { said: format!("{}: {said}", m.name), answered: true, code: Some("shape_changed") })?;
        let (cols, mut page_rows, page_missing) = super::pick::table(&answer, &op.result);
        let got = page_rows.len();
        if columns.is_empty() {
            columns = cols;
        } else if cols.len() == columns.len() && cols.iter().zip(&columns).any(|(a, b)| a.name != b.name) {
            // Pages that came back with columns in another order are put in this one.
            let order: Vec<usize> = columns.iter().map(|c| cols.iter().position(|x| x.name == c.name).unwrap_or(0)).collect();
            page_rows = page_rows.into_iter().map(|r| order.iter().map(|i| r[*i].clone()).collect()).collect();
        }
        rows.extend(page_rows);
        for m in page_missing {
            if !missing.contains(&m) {
                missing.push(m);
            }
        }

        let Some(pg) = &op.paginate else { break };
        let more = match pg.style {
            PageStyle::Cursor => {
                let next = pg.next.as_deref().map(|p| super::pick::one(&answer, p)).unwrap_or(Value::Null);
                match next {
                    Value::String(s) if !s.is_empty() => {
                        page_arg = vec![("cursor", s)];
                        true
                    }
                    Value::Number(n) => {
                        page_arg = vec![("cursor", n.to_string())];
                        true
                    }
                    _ => false,
                }
            }
            PageStyle::Page => {
                page_arg = vec![("page", (page_no + 2).to_string())];
                got > 0
            }
            PageStyle::Offset => {
                page_arg = vec![("offset", rows.len().to_string())];
                got > 0
            }
            PageStyle::LinkHeader => {
                link_next = next_link(&headers, &base);
                link_next.is_some()
            }
        };
        if !more {
            break;
        }
        if page_no + 1 == max_pages {
            missing.push(format!("stopped after {max_pages} pages; there were more"));
        }
        if total > TOTAL_BYTES {
            missing.push(format!("stopped after {} MB; there was more", TOTAL_BYTES / 1024 / 1024));
            break;
        }
    }
    Ok(Fetched { columns, rows, missing, asked: first_asked })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::syn::connector::manifest::{self, tests_support::TODOIST};
    use serde_json::json;

    fn todoist() -> Manifest {
        manifest::parse(TODOIST).expect("reads")
    }

    /// A list goes into a query as the comma list APIs read, not as JSON.
    #[test]
    fn a_list_in_a_query_is_a_comma_list() {
        let m = todoist();
        let p = prepare(&m, &m.operations[0], &json!({ "filter": ["USD", "JPY"] }), Some("tok"), &[]).expect("built");
        assert_eq!(p.url.as_str(), "https://api.todoist.com/rest/v2/tasks?filter=USD%2CJPY");
        let one = prepare(&m, &m.operations[0], &json!({ "filter": ["USD"] }), Some("tok"), &[]).expect("built");
        assert_eq!(one.url.as_str(), "https://api.todoist.com/rest/v2/tasks?filter=USD");
    }

    #[test]
    fn a_request_is_built_from_the_declaration() {
        let m = todoist();
        let p = prepare(&m, &m.operations[0], &json!({ "filter": "today & p1" }), Some("tok"), &[]).expect("built");
        assert_eq!(p.method, reqwest::Method::GET);
        assert_eq!(p.url.as_str(), "https://api.todoist.com/rest/v2/tasks?filter=today+%26+p1");
        assert!(p.headers.contains(&("Authorization".into(), "Bearer tok".into())));
        // An optional parameter not given is left out, not sent empty.
        let bare = prepare(&m, &m.operations[0], &json!({}), Some("tok"), &[]).expect("built");
        assert_eq!(bare.url.as_str(), "https://api.todoist.com/rest/v2/tasks");
    }

    /// The attacks on the path: a value that would be more than one segment,
    /// climb out, or name another host.
    #[test]
    fn a_value_cannot_climb_out_of_its_segment() {
        let m = todoist();
        for bad in ["../../admin", "1/../../x", "a@evil.example", "x#frag", "x?y=1", "%2e%2e", "a%2Fb", "..", ""] {
            let r = prepare(&m, &m.operations[1], &json!({ "id": bad }), Some("tok"), &[]);
            assert!(r.is_err(), "{bad} got through: {r:?}");
        }
        let ok = prepare(&m, &m.operations[1], &json!({ "id": "8123 x" }), Some("tok"), &[]).expect("built");
        assert_eq!(ok.url.as_str(), "https://api.todoist.com/rest/v2/tasks/8123%20x/close");
    }

    #[test]
    fn a_required_value_and_a_line_break_are_checked() {
        let m = todoist();
        assert!(prepare(&m, &m.operations[1], &json!({}), Some("tok"), &[]).is_err());
        assert!(prepare(&m, &m.operations[0], &json!({ "filter": "a\r\nX-Evil: 1" }), Some("tok"), &[]).is_err());
    }

    #[test]
    fn the_secret_is_not_in_what_the_run_records() {
        let mut m = todoist();
        m.auth = Auth::Query { name: "token".into(), secret: "todoist_token".into() };
        let p = prepare(&m, &m.operations[0], &json!({ "filter": "today" }), Some("s3cr3t"), &[]).expect("built");
        assert!(p.url.as_str().contains("s3cr3t"));
        let recorded = asked(&p, &m);
        assert!(!recorded.contains("s3cr3t"), "{recorded}");
        assert!(recorded.contains("filter=today"), "{recorded}");
    }

    #[test]
    fn no_secret_on_this_device_is_said() {
        let m = todoist();
        let err = prepare(&m, &m.operations[0], &json!({}), None, &[]).expect_err("needs it");
        assert!(err.contains("needs its secret"), "{err}");
    }

    #[test]
    fn a_json_body_keeps_a_numbers_type() {
        let mut m = todoist();
        m.operations[1].call.json = Some(json!({ "ids": "{{id}}", "note": "id {{id}}" }));
        let p = prepare(&m, &m.operations[1], &json!({ "id": 42 }), Some("t"), &[]).expect("built");
        assert_eq!(p.body, Some(Body::Json(json!({ "ids": 42, "note": "id 42" }))));
    }

    #[test]
    fn a_next_link_on_another_host_is_not_followed() {
        let base = url::Url::parse("https://api.example.com/v1/items").expect("url");
        let mut h = reqwest::header::HeaderMap::new();
        h.insert(reqwest::header::LINK, "<https://evil.example/v1/items?page=2>; rel=\"next\"".parse().expect("header"));
        assert_eq!(next_link(&h, &base), None);
        h.insert(reqwest::header::LINK, "</v1/items?page=2>; rel=\"next\"".parse().expect("header"));
        assert_eq!(next_link(&h, &base).map(|u| u.to_string()), Some("https://api.example.com/v1/items?page=2".into()));
    }
}
