//! A connector written as a declaration rather than a program.
//!
//! # What this is
//!
//! `docs/syn-connectors-2026-10-08.md` §3: a connector is a *bản khai* — which
//! host, which secret, which operations, whether each reads or writes, what
//! request each sends and what to take from the answer — and Rust carries it
//! out with a fixed set of executors. The person approves the declaration, on
//! each device, and approves it again when it changes. Syn can write one; it
//! cannot write an executor.
//!
//! It lives in the vault as a note of kind `syn_connector` in `SynConnectors/`,
//! with the declaration in a fenced ` ```connector ` block of YAML and the
//! explanation above it in prose — the same argument `skill.rs` makes for a
//! skill: a change in what Syn can reach is a diff somebody can read and undo.
//!
//! # Not a language
//!
//! There is no condition, loop or transformation here, and that is the
//! requirement `recipe.rs` states for itself. Paging, a job, a fan-out are the
//! executor's own patterns with ceilings, declared by parameter. A declaration
//! that wants more is refused with [`NEEDS_LOGIC`], and those refusals are
//! counted: three real ones are the gate for a `script` tier (plan, step 11).
//!
//! # What this module decides and what it does not
//!
//! It reads and checks a declaration, and says what it is worth: the most an
//! operation may be treated as (`effect_ceiling`) and a fingerprint of the
//! whole. Whether this device trusts it is `instance`; carrying it out is `http`.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// The note kind a declaration is filed as.
pub const CONNECTOR_TYPE: &str = "syn_connector";

/// Where they are filed. Top level, not under `Syn/`, for the reason
/// `skill::SKILL_FOLDER` gives: `is_in_unscanned_dir` skips `Syn`.
pub const CONNECTOR_FOLDER: &str = "SynConnectors";

/// The fence that marks the declaration inside the note.
pub const FENCE: &str = "```connector";

/// The problem code for a declaration that wants logic the format does not have.
pub const NEEDS_LOGIC: &str = "needs_logic";

/// Keys that ask for logic. Refused by name rather than as unknown, so the
/// refusal can be counted.
const LOGIC_KEYS: &[&str] = &["if", "when", "else", "loop", "for", "for_each", "while", "script", "transform", "map", "filter", "code"];

/// The longest description an operation may carry — the same as a tool from
/// an MCP server, for the same reason: it reaches the model on every turn the
/// connector is offered, and it may have been written from somebody's page.
pub const MAX_DESCRIPTION: usize = 600;
pub const MAX_PARAM_DESCRIPTION: usize = 200;
pub const MAX_OPERATIONS: usize = 40;
pub const MAX_PAGES: u32 = 50;

/// Headers a declaration may not set. The executor sets the ones it needs.
const FORBIDDEN_HEADERS: &[&str] = &[
    "host", "authorization", "cookie", "set-cookie", "content-length", "transfer-encoding", "connection",
    "proxy-authorization", "upgrade", "te", "trailer",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Provenance {
    User,
    Bundled,
    Syn,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Transport {
    Http,
    Mqtt,
    Sql,
    Mail,
    Ssh,
    Device,
    Process,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Decoder {
    #[default]
    Json,
    Xml,
    Csv,
    Ics,
    Vcard,
    Rss,
    Html,
    Text,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Logic {
    #[default]
    Manifest,
    Mcp,
    Script,
}

/// How a request proves who is asking. The secret is a name; its value is in
/// this device's keychain, slot `connector:<instance>:<name>`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Auth {
    #[default]
    None,
    Bearer { secret: String },
    Header { name: String, secret: String },
    Query { name: String, secret: String },
    Basic { user: String, secret: String },
    /// An `ssh` connector's login: a password, or a private key (Ed25519 or
    /// ECDSA, without a passphrase) — whichever the person pastes on the card.
    Ssh { secret: String },
}

impl Auth {
    pub fn secret(&self) -> Option<&str> {
        match self {
            Auth::None => None,
            Auth::Bearer { secret }
            | Auth::Header { secret, .. }
            | Auth::Query { secret, .. }
            | Auth::Basic { secret, .. }
            | Auth::Ssh { secret } => Some(secret),
        }
    }
}

/// What a call does, as declared. Rust decides how much of that to believe —
/// see [`effect_ceiling`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Effect {
    /// Proven not to write: a `GET`.
    Read,
    /// Could write by its protocol; the person vouches the token cannot.
    ReadAttested,
    Write,
    /// Sent and cannot be taken back: mail, money, a hard delete.
    Irreversible,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct Call {
    pub method: String,
    #[serde(default)]
    pub path: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub query: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub headers: BTreeMap<String, String>,
    /// A JSON body, with `{{param}}` in its strings.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub json: Option<serde_json::Value>,
    /// A form body.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub form: BTreeMap<String, String>,
    /// A body sent as written, its placeholders filled — the XML of a WebDAV
    /// `PROPFIND` or `REPORT`. A value filled into one that starts with `<`
    /// is escaped as XML text, so it cannot add an element.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Param {
    #[serde(default = "string_type", rename = "type")]
    pub ty: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub required: bool,
    #[serde(default, rename = "enum", skip_serializing_if = "Vec::is_empty")]
    pub choices: Vec<String>,
    /// `recipient`, `destination` or `amount`: a value that must come from the
    /// person, never from what was read. See `syn::writes`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
}

fn string_type() -> String {
    "string".into()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PageStyle {
    Cursor,
    Page,
    Offset,
    LinkHeader,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Paginate {
    pub style: PageStyle,
    /// The query parameter that carries the cursor, page number or offset.
    #[serde(default)]
    pub param: String,
    /// Where the next cursor is in an answer, for `cursor`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next: Option<String>,
    #[serde(default = "default_pages")]
    pub max_pages: u32,
}

fn default_pages() -> u32 {
    10
}

/// One column taken from an answer: a path, and what the value is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Pick {
    Path(String),
    Typed {
        path: String,
        #[serde(default, rename = "type", skip_serializing_if = "Option::is_none")]
        ty: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tz: Option<String>,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        free_text: bool,
    },
}

impl Pick {
    pub fn path(&self) -> &str {
        match self {
            Pick::Path(p) | Pick::Typed { path: p, .. } => p,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Explode {
    pub path: String,
    #[serde(default)]
    pub pick: BTreeMap<String, Pick>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Truncated {
    pub total: String,
    pub got: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct ResultSpec {
    /// Where the records are: `$` for the answer itself, `$.issues[*]`.
    #[serde(default = "root")]
    pub each: String,
    /// For an answer in columns rather than records — an object of lists of
    /// equal length, `{"time": […], "rain": […]}` — the path to that object.
    /// Its lists are read side by side into one record per position.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub zip: Option<String>,
    /// Columns by name. Empty: every field of each record, as it is.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub pick: BTreeMap<String, Pick>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub explode: Option<Explode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub truncated_when: Option<Truncated>,
}

fn root() -> String {
    "$".into()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Operation {
    pub name: String,
    pub description: String,
    pub effect: Effect,
    /// The request, for `http`. A `device` operation has none.
    #[serde(default, skip_serializing_if = "Call::is_empty")]
    pub call: Call,
    /// The program and its arguments, for `ssh`: one word each, `{{param}}`
    /// filled into a word and quoted for the shell on the other side. The
    /// program is written here, never filled. See `connector::ssh`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub command: Vec<String>,
    /// The bridge it reads, for `device`. See `connector::device`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bridge: Option<String>,
    /// What the bridge keeps: fields the system wrote, never the content.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub filter: BTreeMap<String, serde_json::Value>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub params: BTreeMap<String, Param>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub paginate: Option<Paginate>,
    #[serde(default)]
    pub result: ResultSpec,
    /// `discover`: lists what there is (indexes, fields). Cheap and read-only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    /// The query language a parameter carries: `spl`, `jql`, `sql`…
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    /// What the request writes into — `spreadsheet`, `html`, `markdown`,
    /// `plain` — so what it carries is made inert there. See `writes::neutralize`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sink: Option<String>,
    /// How this operation's answer is read, when it is not the declaration's
    /// `decoder` — a CalDAV `PROPFIND` (xml) beside its `REPORT` (ics).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decoder: Option<Decoder>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub id: String,
    pub name: String,
    #[serde(default = "first")]
    pub version: u32,
    #[serde(default = "by_user")]
    pub provenance: Provenance,
    #[serde(default = "http")]
    pub transport: Transport,
    #[serde(default)]
    pub decoder: Decoder,
    #[serde(default)]
    pub logic: Logic,
    /// The service's address; empty for a `device` connector.
    #[serde(default)]
    pub base_url: String,
    #[serde(default)]
    pub auth: Auth,
    /// What only the person knows and is asked for on the connect card: an
    /// account, a key, their own server's address. See `Input`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub inputs: Vec<Input>,
    pub operations: Vec<Operation>,
    /// A `syn_skill` with how to use it, a map of its data, its definitions.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skill: Option<String>,
    /// Reads Rust runs on a schedule, with no model, into the vault. See
    /// `connector::mirror`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mirrors: Vec<Mirror>,
}

/// Text in the person's language: one string, or one per language code.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Words {
    Plain(String),
    By(BTreeMap<String, String>),
}

impl Words {
    /// In `lang`, else English, else any.
    pub fn in_lang(&self, lang: &str) -> String {
        match self {
            Words::Plain(s) => s.clone(),
            Words::By(m) => m.get(lang).or_else(|| m.get("en")).or_else(|| m.values().next()).cloned().unwrap_or_default(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InputKind {
    /// A name the service knows them by: an email, a user name. Kept on this device.
    Account,
    /// A key, a token, a password. Straight to the keychain; never to the model.
    Secret,
    /// Their own server's address. Becomes the connector's host — one they typed.
    Address,
}

/// One thing the connect card asks for, with how to find it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Input {
    pub name: String,
    pub kind: InputKind,
    pub label: Words,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub example: Option<String>,
    /// A pattern the value must match, checked before anything is sent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub guide: Option<Guide>,
}

/// How to find an input's value: a page on the service and the steps there.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Guide {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub link: Option<String>,
    #[serde(default)]
    pub steps: BTreeMap<String, Vec<String>>,
}

impl Manifest {
    pub fn input(&self, name: &str) -> Option<&Input> {
        self.inputs.iter().find(|i| i.name == name)
    }

    /// The declaration with the person's values in place of `{{input.*}}`: in
    /// the address (whole), the basic-auth user, and each call. A value in a
    /// path is one segment, encoded; in the address it is the address.
    pub fn with_inputs(&self, values: &BTreeMap<String, String>) -> Manifest {
        let mut m = self.clone();
        let plain = |s: &str| fill_inputs(s, values, false);
        m.base_url = plain(&m.base_url).trim_end_matches('/').to_string();
        if let Auth::Basic { user, .. } = &mut m.auth {
            *user = plain(user);
        }
        for op in &mut m.operations {
            op.call.path = fill_inputs(&op.call.path, values, true);
            for v in op.call.query.values_mut().chain(op.call.headers.values_mut()).chain(op.call.form.values_mut()) {
                *v = plain(v);
            }
            if let Some(b) = &mut op.call.body {
                *b = plain(b);
            }
            for word in &mut op.command {
                *word = plain(word);
            }
        }
        m
    }
}

/// `{{input.name}}` filled; encoded as a path segment when `segment`.
pub fn fill_inputs(text: &str, values: &BTreeMap<String, String>, segment: bool) -> String {
    let mut out = text.to_string();
    for (k, v) in values {
        let value = if segment { urlencoding::encode(v).into_owned() } else { v.clone() };
        out = out.replace(&format!("{{{{input.{k}}}}}"), &value);
    }
    out
}

/// A reading operation kept in the vault: its rows become notes, by key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Mirror {
    pub name: String,
    /// The `read` operation it runs.
    pub from: String,
    /// Its arguments. `{{window_start}}`/`{{window_end}}` (`20261012T000000Z`)
    /// and `{{window_start_date}}`/`{{window_end_date}}` are the window's.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub args: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub window: Option<MirrorWindow>,
    #[serde(default = "hourly")]
    pub every_minutes: u32,
    /// Worked out over the rows before they are kept — `where`, `eval` with
    /// `extract`, as `crunch` runs them — with no model: an amount out of a
    /// bank's notification.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pipeline: Option<String>,
    /// The column that names a row, across runs and devices.
    pub key: String,
    /// `event`, `person`, `note`, or `staging:transaction`.
    pub into: String,
    /// The note's field ← the row's column.
    #[serde(default)]
    pub fields: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MirrorWindow {
    #[serde(default)]
    pub past_days: u32,
    #[serde(default)]
    pub future_days: u32,
}

fn hourly() -> u32 {
    60
}

/// What each kind a mirror writes into may be filled with.
pub fn mirror_fields(into: &str) -> Option<&'static [&'static str]> {
    Some(match into {
        "event" => &["title", "content", "start_at", "end_at", "is_all_day", "location", "tzid", "rrule", "tags"],
        "person" => &["title", "content", "email", "phone", "birthday", "nickname"],
        "note" => &["title", "content", "tags"],
        "staging:transaction" => &["date", "amount", "note", "category", "type"],
        _ => return None,
    })
}

impl Call {
    pub fn is_empty(&self) -> bool {
        self == &Call::default()
    }
}

fn first() -> u32 {
    1
}
fn by_user() -> Provenance {
    Provenance::User
}
fn http() -> Transport {
    Transport::Http
}

/// One thing wrong with a declaration.
///
/// `code` is stable: Syn will be handed these to fix its own drafts, and
/// `needs_logic` is counted. `at` says where; `said` is for a person.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Problem {
    pub code: &'static str,
    pub at: String,
    pub said: String,
}

fn problem(code: &'static str, at: impl Into<String>, said: impl Into<String>) -> Problem {
    Problem { code, at: at.into(), said: said.into() }
}

/// The YAML inside the fence, or why there is none.
pub fn block(body: &str) -> Result<&str, Problem> {
    let start = body.find(FENCE).ok_or_else(|| problem("no_block", "", "there is no ```connector block"))?;
    let after = &body[start + FENCE.len()..];
    let end = after.find("```").ok_or_else(|| problem("unclosed_block", "", "the ```connector block is never closed"))?;
    let yaml = after[..end].trim();
    if yaml.is_empty() {
        return Err(problem("empty_block", "", "the ```connector block is empty"));
    }
    Ok(yaml)
}

/// Read a declaration from YAML. A key asking for logic is refused as
/// `needs_logic`, by name; any other unknown key, as what it is.
pub fn parse_yaml(yaml: &str) -> Result<Manifest, Problem> {
    if let Ok(raw) = serde_yaml::from_str::<serde_yaml::Value>(yaml) {
        if let Some(found) = logic_key(&raw, "") {
            return Err(problem(
                NEEDS_LOGIC,
                found.clone(),
                format!(
                    "`{found}` asks for logic — a condition, a loop or a transformation. A connector declares calls; \
                     it does not decide between them. Split it into operations, or leave the working-out to crunch."
                ),
            ));
        }
    }
    serde_yaml::from_str::<Manifest>(yaml).map_err(|e| problem("not_readable", "", format!("the declaration is not readable: {e}")))
}

fn logic_key(v: &serde_yaml::Value, at: &str) -> Option<String> {
    match v {
        serde_yaml::Value::Mapping(m) => m.iter().find_map(|(k, v)| {
            let key = k.as_str().unwrap_or_default();
            let here = if at.is_empty() { key.to_string() } else { format!("{at}.{key}") };
            // `call.query`, `call.json` and `pick` hold names the API chose, not ours.
            let theirs = at.ends_with("query") || at.ends_with("headers") || at.ends_with("json") || at.ends_with("form")
                || at.ends_with("pick") || at.ends_with("params");
            // A bridge's `filter` is a list of values for fields the system
            // wrote (`device::filters`), not a condition.
            let bridge_filter = key == "filter" && at.starts_with("operations[") && !at.contains('.');
            if !theirs && !bridge_filter && LOGIC_KEYS.contains(&key) {
                Some(here)
            } else {
                logic_key(v, &here)
            }
        }),
        serde_yaml::Value::Sequence(s) => s.iter().enumerate().find_map(|(i, v)| logic_key(v, &format!("{at}[{i}]"))),
        _ => None,
    }
}

/// Read the declaration in a note's body.
pub fn parse(body: &str) -> Result<Manifest, Problem> {
    parse_yaml(block(body)?)
}

/// The most an operation may be treated as, from its method alone. A
/// declaration may say less (`write` for a `GET` it knows has side effects)
/// and never more.
pub fn effect_ceiling(method: &str) -> Effect {
    match method.to_ascii_uppercase().as_str() {
        "GET" | "HEAD" | "PROPFIND" | "REPORT" => Effect::Read,
        _ => Effect::ReadAttested,
    }
}

/// `{{name}}` placeholders in a string.
pub fn placeholders(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find("{{") {
        let Some(end) = rest[start + 2..].find("}}") else { break };
        out.push(rest[start + 2..start + 2 + end].trim().to_string());
        rest = &rest[start + 2 + end + 2..];
    }
    out
}

fn json_placeholders(v: &serde_json::Value, out: &mut Vec<String>) {
    match v {
        serde_json::Value::String(s) => out.extend(placeholders(s)),
        serde_json::Value::Array(a) => a.iter().for_each(|x| json_placeholders(x, out)),
        serde_json::Value::Object(o) => o.values().for_each(|x| json_placeholders(x, out)),
        _ => {}
    }
}

/// A path into an answer: `$`, `$.a.b`, `$.issues[*]`, `fields.assignee.displayName`, `items[0].id`.
pub fn path_ok(path: &str) -> bool {
    let p = path.strip_prefix('$').unwrap_or(path);
    let p = p.strip_prefix('.').unwrap_or(p);
    if p.is_empty() {
        return true;
    }
    p.split('.').all(|seg| {
        if seg.is_empty() {
            return false;
        }
        let (name, rest) = seg.split_once('[').map_or((seg, ""), |(n, r)| (n, r));
        (name.is_empty() && !rest.is_empty() || name.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-' || c == '@'))
            && (rest.is_empty() || rest == "*]" || rest.trim_end_matches(']').chars().all(|c| c.is_ascii_digit()))
    })
}

/// Everything wrong with a declaration, all at once.
pub fn problems(m: &Manifest) -> Vec<Problem> {
    let mut out = Vec::new();

    if m.id.is_empty() || !m.id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_') || m.id.len() > 20 {
        out.push(problem("bad_id", "id", "an id is up to 20 of a–z, 0–9 and _"));
    }
    if m.name.trim().is_empty() || m.name.contains(':') {
        out.push(problem("bad_name", "name", "a name is needed, without ':'"));
    }
    let device = m.transport == Transport::Device;
    let ssh = m.transport == Transport::Ssh;
    if m.transport != Transport::Http && !device && !ssh {
        out.push(problem("transport_not_yet", "transport", format!("{:?} is not available yet; only http, ssh and device are", m.transport).to_lowercase()));
    }
    if device {
        // A bridge is the app's own code on this device: no host, no secret.
        if !m.base_url.is_empty() {
            out.push(problem("device_has_host", "base_url", "a device connector reaches no host; leave base_url out"));
        }
        if m.auth != Auth::None {
            out.push(problem("device_has_secret", "auth", "a device connector needs no secret; the system asks for permission"));
        }
    }
    if m.logic != Logic::Manifest {
        out.push(problem("logic_not_yet", "logic", "only a declared manifest is available here; MCP servers are added in settings"));
    }

    // Inputs: named once, each a kind, a secret only through auth.
    let mut input_names = std::collections::BTreeSet::new();
    for (i, input) in m.inputs.iter().enumerate() {
        if input.name.is_empty() || !input.name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_') || !input_names.insert(input.name.clone()) {
            out.push(problem("bad_input_name", format!("inputs[{i}].name"), "an input's name is a–z, 0–9 and _, and unique"));
        }
        if let Some(f) = &input.format {
            if regex::Regex::new(f).is_err() {
                out.push(problem("bad_input_format", format!("inputs[{i}].format"), "the format is not a pattern that reads"));
            }
        }
        if let Some(link) = input.guide.as_ref().and_then(|g| g.link.as_deref()) {
            if !url::Url::parse(link).is_ok_and(|u| u.scheme() == "https") {
                out.push(problem("bad_guide_link", format!("inputs[{i}].guide.link"), "a guide's link is an https address"));
            }
        }
    }
    if let Some(name) = m.auth.secret() {
        if !m.inputs.is_empty() && !m.inputs.iter().any(|i| i.name == name && i.kind == InputKind::Secret) {
            out.push(problem("secret_not_an_input", "auth.secret", format!("`{name}` is not a secret input")));
        }
    }
    let address_input = m.base_url.strip_prefix("{{input.").and_then(|r| r.split_once("}}")).map(|(n, _)| n.to_string());

    // The host is fixed: no placeholder may reach it, except the one address
    // the person types on the card, which is then theirs.
    if device {
    } else if let Some(name) = &address_input {
        if m.input(name).map(|i| i.kind) != Some(InputKind::Address) {
            out.push(problem("template_in_host", "base_url", format!("`{name}` is not an address input")));
        }
        if m.base_url.matches("{{").count() > 1 {
            out.push(problem("template_in_host", "base_url", "only one address input may be in base_url, at its start"));
        }
    } else if m.base_url.contains("{{") {
        out.push(problem("template_in_host", "base_url", "base_url cannot hold a {{placeholder}}; only paths, queries, headers and bodies can"));
    } else if ssh {
        if let Err(said) = super::ssh::target(&m.base_url) {
            out.push(problem("bad_url", "base_url", said));
        }
    } else {
        match url::Url::parse(&m.base_url) {
            Ok(u) => {
                if u.scheme() != "https" && u.scheme() != "http" {
                    out.push(problem("bad_scheme", "base_url", "base_url must be https (or http to this machine or a local network)"));
                }
                if !u.username().is_empty() || u.password().is_some() {
                    out.push(problem("credentials_in_url", "base_url", "a name or password in the address is not allowed; use auth"));
                }
                if u.query().is_some() || u.fragment().is_some() {
                    out.push(problem("query_in_base", "base_url", "base_url cannot carry a query or fragment"));
                }
                if u.host_str().is_none() {
                    out.push(problem("no_host", "base_url", "base_url has no host"));
                }
            }
            Err(e) => out.push(problem("bad_url", "base_url", format!("base_url is not an address: {e}"))),
        }
    }

    match &m.auth {
        Auth::None => {}
        a => {
            let s = a.secret().unwrap_or_default();
            if s.is_empty() || !s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                out.push(problem("bad_secret_name", "auth.secret", "a secret is named with a–z, 0–9 and _; its value is typed on the approval card"));
            }
            if let Auth::Header { name, .. } = a {
                if FORBIDDEN_HEADERS.contains(&name.to_ascii_lowercase().as_str()) && !name.eq_ignore_ascii_case("authorization") {
                    out.push(problem("forbidden_header", "auth.name", format!("`{name}` cannot be set")));
                }
            }
        }
    }

    // An ssh connector logs in with what the person pastes; nothing else does.
    match (&m.auth, ssh) {
        (Auth::Ssh { .. }, true) => {}
        (_, true) => out.push(problem("ssh_auth", "auth", "an ssh connector logs in with { type: ssh, secret: <name> }: a password or a private key, typed on the card")),
        (Auth::Ssh { .. }, false) => out.push(problem("ssh_auth", "auth", "type: ssh is the login of an ssh connector")),
        _ => {}
    }

    if m.operations.is_empty() {
        out.push(problem("no_operations", "operations", "a connector needs at least one operation"));
    }
    if m.operations.len() > MAX_OPERATIONS {
        out.push(problem("too_many_operations", "operations", format!("at most {MAX_OPERATIONS} operations")));
    }
    let mut seen = std::collections::BTreeSet::new();
    for (i, op) in m.operations.iter().enumerate() {
        let at = |what: &str| format!("operations[{i}]{what}");
        if op.name.is_empty() || !op.name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_') || op.name.len() > 40 {
            out.push(problem("bad_operation_name", at(".name"), "an operation name is up to 40 of a–z, 0–9 and _"));
        }
        if !seen.insert(op.name.clone()) {
            out.push(problem("duplicate_operation", at(".name"), format!("`{}` is declared twice", op.name)));
        }
        if op.description.trim().is_empty() || op.description.chars().count() > MAX_DESCRIPTION {
            out.push(problem("bad_description", at(".description"), format!("a description is needed, at most {MAX_DESCRIPTION} characters")));
        }
        if device {
            match op.bridge.as_deref() {
                Some(b) if super::device::known(b) => {
                    for (name, _) in &op.params {
                        if !super::device::takes(b, name) {
                            out.push(problem("bridge_param", at(&format!(".params.{name}")), format!("`{b}` takes no `{name}`")));
                        }
                    }
                    for (key, v) in &op.filter {
                        let plain = v.as_array().is_some_and(|a| a.iter().all(|x| x.is_string() || x.is_number()));
                        if !plain {
                            out.push(problem("bridge_filter", at(&format!(".filter.{key}")), "a filter is a list of values"));
                        }
                        if !super::device::filters(b, key) {
                            out.push(problem("bridge_filter", at(&format!(".filter.{key}")), format!("`{b}` cannot be filtered by `{key}`")));
                        }
                    }
                }
                Some(b) => out.push(problem("unknown_bridge", at(".bridge"), format!("there is no bridge `{b}`; there are {}", super::device::names().join(", ")))),
                None => out.push(problem("no_bridge", at(".bridge"), "a device operation names its bridge")),
            }
            if op.effect != Effect::Read {
                out.push(problem("bridge_reads", at(".effect"), "a bridge only reads"));
            }
            if !op.call.is_empty() {
                out.push(problem("device_has_call", at(".call"), "a device operation has a bridge, not a call"));
            }
            continue;
        }
        if op.bridge.is_some() || !op.filter.is_empty() {
            out.push(problem("bridge_on_http", at(".bridge"), "a bridge belongs to a device connector"));
        }
        if ssh {
            if !op.call.is_empty() {
                out.push(problem("ssh_has_call", at(".call"), "an ssh operation runs a command, not a call"));
            }
            if op.effect == Effect::Read {
                out.push(problem(
                    "effect_above_ceiling",
                    at(".effect"),
                    "nothing proves a command only reads: declare read_attested for one that only looks, which the person vouches for, or write",
                ));
            }
            if op.paginate.is_some() {
                out.push(problem("bad_paginate", at(".paginate"), "a command runs once; it has no pages"));
            }
            for said in super::ssh::command_problems(&op.command) {
                out.push(problem("bad_command", at(".command"), said));
            }
        } else if !op.command.is_empty() {
            out.push(problem("command_on_http", at(".command"), "a command belongs to an ssh connector"));
        }
        let method = op.call.method.to_ascii_uppercase();
        if !ssh {
            if !["GET", "HEAD", "POST", "PUT", "PATCH", "DELETE", "PROPFIND", "REPORT"].contains(&method.as_str()) {
                out.push(problem("bad_method", at(".call.method"), format!("`{}` is not a method this executor sends", op.call.method)));
            }
            if op.effect < effect_ceiling(&method) {
                out.push(problem(
                    "effect_above_ceiling",
                    at(".effect"),
                    format!("a {method} cannot be declared `read`: only GET, HEAD, PROPFIND and REPORT can be. Use read_attested if the token can only read, or write."),
                ));
            }
            if !op.call.path.starts_with('/') && !op.call.path.is_empty() {
                out.push(problem("bad_path", at(".call.path"), "a path starts with /"));
            }
            if op.call.path.contains("://") || op.call.path.starts_with("//") {
                out.push(problem("template_in_host", at(".call.path"), "a path cannot name another host"));
            }
            for name in op.call.headers.keys() {
                if FORBIDDEN_HEADERS.contains(&name.to_ascii_lowercase().as_str()) {
                    out.push(problem("forbidden_header", at(".call.headers"), format!("`{name}` cannot be set; the executor sets what it needs")));
                }
            }
            if op.sink.as_deref().is_some_and(|s| !["spreadsheet", "html", "markdown", "plain"].contains(&s)) {
                out.push(problem("bad_sink", at(".sink"), "a sink is spreadsheet, html, markdown or plain"));
            }
            if [op.call.json.is_some(), !op.call.form.is_empty(), op.call.body.is_some()].iter().filter(|b| **b).count() > 1 {
                out.push(problem("two_bodies", at(".call"), "a call has one body: json, form or body"));
            }
        }

        // Every placeholder names a parameter, and every parameter is used.
        let mut used = placeholders(&op.call.path);
        op.command.iter().for_each(|w| used.extend(placeholders(w)));
        op.call.query.values().chain(op.call.headers.values()).chain(op.call.form.values()).chain(op.call.body.iter()).for_each(|v| used.extend(placeholders(v)));
        if let Some(j) = &op.call.json {
            json_placeholders(j, &mut used);
        }
        for p in &used {
            if let Some(name) = p.strip_prefix("input.") {
                if m.input(name).is_none_or(|i| i.kind == InputKind::Secret) {
                    out.push(problem("unknown_param", at(".call"), format!("{{{{{p}}}}} names no account or address input")));
                }
                continue;
            }
            let known = op.params.contains_key(p)
                || (op.paginate.is_some() && (p == "cursor" || p == "page" || p == "offset"));
            if !known {
                out.push(problem("unknown_param", at(".call"), format!("{{{{{p}}}}} names no parameter")));
            }
        }
        for (name, param) in &op.params {
            if !used.contains(name) {
                out.push(problem("unused_param", at(&format!(".params.{name}")), format!("`{name}` is declared and never sent")));
            }
            if param.description.chars().count() > MAX_PARAM_DESCRIPTION {
                out.push(problem("bad_description", at(&format!(".params.{name}")), format!("at most {MAX_PARAM_DESCRIPTION} characters")));
            }
            if param.role.as_deref().is_some_and(|r| !["recipient", "destination", "amount"].contains(&r)) {
                out.push(problem("bad_role", at(&format!(".params.{name}")), "a role is recipient, destination or amount"));
            }
            if !["string", "number", "integer", "boolean", "array"].contains(&param.ty.as_str()) {
                out.push(problem("bad_param_type", at(&format!(".params.{name}")), "a parameter is a string, number, integer or boolean"));
            }
        }
        if let Some(pg) = &op.paginate {
            if pg.max_pages == 0 || pg.max_pages > MAX_PAGES {
                out.push(problem("too_many_pages", at(".paginate.max_pages"), format!("between 1 and {MAX_PAGES}")));
            }
            if pg.style == PageStyle::Cursor && pg.next.as_deref().is_none_or(|n| !path_ok(n)) {
                out.push(problem("bad_path", at(".paginate.next"), "a cursor needs `next`, the path of the next cursor in the answer"));
            }
            if pg.style != PageStyle::LinkHeader && pg.param.is_empty() {
                out.push(problem("bad_paginate", at(".paginate.param"), "which query parameter carries the page"));
            }
        }
        let r = &op.result;
        let mut paths: Vec<(&str, String)> = vec![(r.each.as_str(), ".result.each".into())];
        if let Some(z) = &r.zip {
            paths.push((z.as_str(), ".result.zip".into()));
            if r.each != "$" {
                out.push(problem("zip_and_each", at(".result"), "an answer in columns is read with zip, and then each is not used"));
            }
        }
        paths.extend(r.pick.iter().map(|(k, p)| (p.path(), format!(".result.pick.{k}"))));
        if let Some(e) = &r.explode {
            paths.push((e.path.as_str(), ".result.explode.path".into()));
            paths.extend(e.pick.iter().map(|(k, p)| (p.path(), format!(".result.explode.pick.{k}"))));
        }
        if let Some(t) = &r.truncated_when {
            paths.push((t.total.as_str(), ".result.truncated_when.total".into()));
            paths.push((t.got.as_str(), ".result.truncated_when.got".into()));
        }
        for (p, where_) in paths {
            if !path_ok(p) {
                out.push(problem("bad_path", at(&where_), format!("`{p}` is not a path like $.items[*].name")));
            }
        }
        for (name, pick) in r.pick.iter().chain(r.explode.iter().flat_map(|e| e.pick.iter())) {
            if let Pick::Typed { ty: Some(t), .. } = pick {
                if !["text", "number", "bool", "time", "json"].contains(&t.as_str()) {
                    out.push(problem("bad_column_type", at(&format!(".result.pick.{name}")), "a column is text, number, bool, time or json"));
                }
            }
        }
    }
    let mut mirror_names = std::collections::BTreeSet::new();
    for (i, mi) in m.mirrors.iter().enumerate() {
        let at = |f: &str| format!("mirrors[{i}]{f}");
        if mi.name.is_empty() || !mi.name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_') || !mirror_names.insert(mi.name.clone()) {
            out.push(problem("bad_mirror_name", at(".name"), "a mirror's name is a–z, 0–9 and _, and unique"));
        }
        // A mirror runs with nobody watching: it may only read.
        match m.operations.iter().find(|o| o.name == mi.from) {
            None => out.push(problem("bad_mirror_from", at(".from"), format!("there is no operation `{}`", mi.from))),
            Some(op) if op.effect != Effect::Read => {
                out.push(problem("mirror_not_read", at(".from"), "a mirror runs unattended, so only an operation declared `read` can be one"))
            }
            Some(op) => {
                for v in mi.args.values() {
                    for p in placeholders(v) {
                        if !p.starts_with("window_") && !op.params.contains_key(&p) {
                            out.push(problem("unknown_placeholder", at(".args"), format!("`{{{{{p}}}}}` is no parameter of `{}` nor a window value", op.name)));
                        }
                    }
                }
            }
        }
        if mi.every_minutes < 15 {
            out.push(problem("mirror_too_often", at(".every_minutes"), "a mirror runs at most every 15 minutes"));
        }
        if mi.key.trim().is_empty() {
            out.push(problem("mirror_no_key", at(".key"), "a mirror needs the column that names a row"));
        }
        match mirror_fields(&mi.into) {
            None => out.push(problem("bad_mirror_into", at(".into"), "a mirror goes into event, person, note or staging:transaction")),
            Some(allowed) => {
                for f in mi.fields.keys() {
                    if !allowed.contains(&f.as_str()) {
                        out.push(problem("bad_mirror_field", at(&format!(".fields.{f}")), format!("{} has no field `{f}` a mirror fills; it has {}", mi.into, allowed.join(", "))));
                    }
                }
            }
        }
    }
    out
}

/// What a declaration is, as one string nothing but the declaration decides:
/// SHA-256 of its canonical JSON. A word changed in a description changes it,
/// and with it the approval.
pub fn fingerprint(m: &Manifest) -> String {
    let canonical = serde_json::to_string(m).unwrap_or_default();
    hex::encode(Sha256::digest(canonical.as_bytes()))
}

/// One declaration as found in the vault: the note it is in, and what it says.
#[derive(Debug, Clone)]
pub struct Found {
    pub node_id: String,
    pub title: String,
    pub body: String,
    pub manifest: Result<Manifest, Problem>,
}

/// Every declaration in the vault, readable or not — an unreadable one is
/// shown on the settings screen with what is wrong, not hidden.
pub fn all(db: &crate::db::DbBridge) -> crate::error::AppResult<Vec<Found>> {
    Ok(db
        .get_nodes_by_type(CONNECTOR_TYPE)?
        .into_iter()
        .map(|n| Found { manifest: parse(&n.content), node_id: n.id, title: n.title, body: n.content })
        .collect())
}

/// The tool name an operation is offered under: `connector__<id>__<op>`, the
/// same shape as a tool on an MCP server, so the gate, the groups, the labels
/// and the audit treat both alike.
pub fn tool_name(m: &Manifest, op: &Operation) -> String {
    format!("{}{}__{}", super::provider::PREFIX, m.id, op.name)
}

/// A declaration for tests here and beside this module.
#[cfg(test)]
pub mod tests_support {
    pub const TODOIST: &str = r#"
Một connector cho Todoist.

```connector
id: todoist
name: Todoist
base_url: https://api.todoist.com/rest/v2
auth: { type: bearer, secret: todoist_token }
operations:
  - name: list_tasks
    description: Open tasks, by Todoist filter.
    effect: read
    call: { method: GET, path: /tasks, query: { filter: "{{filter}}" } }
    params:
      filter: { description: "today | overdue | …" }
    result:
      each: "$"
      pick:
        id: id
        content: { path: content, free_text: true }
        due: { path: due.date, type: time }
  - name: close_task
    description: Mark a task done.
    effect: write
    call: { method: POST, path: "/tasks/{{id}}/close" }
    params:
      id: { required: true }
```
"#;
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::tests_support::TODOIST;


    #[test]
    fn a_good_declaration_reads_and_has_no_problems() {
        let m = parse(TODOIST).expect("reads");
        assert_eq!(m.operations.len(), 2);
        assert_eq!(problems(&m), vec![]);
        assert_eq!(tool_name(&m, &m.operations[0]), "connector__todoist__list_tasks");
        assert_eq!(fingerprint(&m).len(), 64);
    }

    #[test]
    fn a_post_cannot_be_declared_a_read() {
        let mut m = parse(TODOIST).expect("reads");
        m.operations[1].effect = Effect::Read;
        assert!(problems(&m).iter().any(|p| p.code == "effect_above_ceiling"));
        m.operations[1].effect = Effect::ReadAttested;
        assert!(problems(&m).is_empty(), "read_attested is a POST's own word for it");
    }

    #[test]
    fn the_host_cannot_be_templated_or_named_from_a_path() {
        let mut m = parse(TODOIST).expect("reads");
        m.base_url = "https://{{host}}/api".into();
        assert!(problems(&m).iter().any(|p| p.code == "template_in_host"));
        let mut m = parse(TODOIST).expect("reads");
        m.operations[0].call.path = "//evil.example/x".into();
        assert!(problems(&m).iter().any(|p| p.code == "template_in_host"));
        let mut m = parse(TODOIST).expect("reads");
        m.base_url = "https://user:pw@api.todoist.com".into();
        assert!(problems(&m).iter().any(|p| p.code == "credentials_in_url"));
    }

    #[test]
    fn placeholders_and_parameters_must_agree() {
        let mut m = parse(TODOIST).expect("reads");
        m.operations[0].call.query.insert("project".into(), "{{project_id}}".into());
        assert!(problems(&m).iter().any(|p| p.code == "unknown_param"));
        let mut m = parse(TODOIST).expect("reads");
        m.operations[0].params.insert("spare".into(), Param { ty: "string".into(), description: String::new(), required: false, choices: vec![], role: None });
        assert!(problems(&m).iter().any(|p| p.code == "unused_param"));
    }

    #[test]
    fn a_forbidden_header_is_refused() {
        let mut m = parse(TODOIST).expect("reads");
        m.operations[0].call.headers.insert("Host".into(), "evil.example".into());
        assert!(problems(&m).iter().any(|p| p.code == "forbidden_header"));
    }

    /// Logic is refused by name, so the refusals can be counted.
    #[test]
    fn logic_is_refused_as_needs_logic() {
        let yaml = TODOIST.replace("    effect: read\n", "    effect: read\n    if: \"{{filter}} == today\"\n");
        let err = parse(&yaml).expect_err("refused");
        assert_eq!(err.code, NEEDS_LOGIC, "{err:?}");
        // A query parameter that happens to be called `filter` is the API's, not logic.
        assert!(parse(TODOIST).is_ok());
    }

    #[test]
    fn a_changed_description_is_a_different_declaration() {
        let a = parse(TODOIST).expect("reads");
        let mut b = a.clone();
        b.operations[0].description.push('!');
        assert_ne!(fingerprint(&a), fingerprint(&b));
        assert_eq!(fingerprint(&a), fingerprint(&parse(TODOIST).expect("reads")));
    }

    #[test]
    fn transports_not_built_yet_are_said_so() {
        let mut m = parse(TODOIST).expect("reads");
        m.transport = Transport::Mqtt;
        assert!(problems(&m).iter().any(|p| p.code == "transport_not_yet"));
    }

    #[test]
    fn paths_are_paths() {
        for ok in ["$", "$.issues[*]", "fields.assignee.displayName", "items[0].id", "$.entry[0].content.isDone"] {
            assert!(path_ok(ok), "{ok}");
        }
        for bad in ["$.a b", "a[x]", "a..b"] {
            assert!(!path_ok(bad), "{bad}");
        }
    }
}
