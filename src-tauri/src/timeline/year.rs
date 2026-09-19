//! "Một năm bằng chính lời mày" — §7.6 of `docs/timeline-2026-09-17.md`.
//!
//! Ten to fifteen sentences the person wrote, in the order they wrote them,
//! each with its day and a way back to the note. No opening, no closing, no
//! adjective. The app does not tell anybody about their year; it puts them back
//! in front of themselves, fifteen times, in ten minutes.
//!
//! # The model chooses; it does not write
//!
//! This is the whole design, and it is enforced by the **protocol** rather than
//! by asking nicely. The model is handed a numbered list of sentences taken
//! from the vault, and it answers with numbers. There is no field in the reply
//! for prose, so "the model wrote something" is not a bug that can happen — it
//! is a shape the reply cannot take.
//!
//! That matters more here than anywhere else in the document. Every other
//! feature quotes one sentence; this one builds a whole year, which is exactly
//! the shape a model most wants to narrate. An instruction not to narrate is a
//! request. A reply format with no room for narration is a fact.
//!
//! The numbers are checked back against the list anyway ([`keep`]), because a
//! model can return an index that was never offered. §16 Bước 6 makes a
//! mismatched sentence a ship-blocking error, so it is handled as one: dropped,
//! not shown.
//!
//! # What the model never sees
//!
//! Candidates are filtered through [`super::quiet::allow`] *before* the prompt
//! is built, so a sealed note, a hushed day, a hushed person and a sentence
//! already waved away are not merely absent from the answer — they were never
//! sent anywhere. §8.1: the assistant does not read what is sealed.
//!
//! # Nothing is written down
//!
//! A year is built on demand and kept nowhere, so removing this feature removes
//! it entirely (§16 Bước 6, "Lùi"). The one thing that outlives a sitting is a
//! refusal, and that is an ordinary hush on one line ([`super::quiet::Subject::Line`]).

use chrono::NaiveDate;
use serde::Serialize;
use serde_json::Value;

use super::quiet::{self, Nudge, Quiet};
use super::seal::Seals;
use super::store::TimelineStore;
use super::{onthisday, when};
use crate::db::DbBridge;
use crate::error::{AppError, AppResult};
use crate::syn::provider::{ChatMessage, ChatProvider, ChatRequest};

/// How many sentences a year is made of. §7.6 says ten to fifteen.
pub const MOST_KEPT: usize = 15;

/// At most this many from any one day, so a single long note cannot become the
/// year. A day that mattered gets a few lines, not a chapter.
const MOST_PER_DAY: usize = 3;

/// The list the model reads has to fit in its context, and a year of daily
/// writing does not. Beyond this the year is thinned evenly across itself
/// rather than truncated, so December is as likely to be read as January.
const MOST_OFFERED: usize = 300;

/// One sentence that could be in the year.
#[derive(Debug, Clone, PartialEq)]
pub struct Candidate {
    pub day: String,
    pub node_id: String,
    pub text: String,
}

/// One sentence that is.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Line {
    pub day: String,
    pub node_id: String,
    /// Verbatim, and checked against the list that was offered.
    pub text: String,
}

/// Everything from `year` that may be offered, oldest first.
///
/// Reads the timeline for the year's days and the vault cache for their words,
/// then drops everything the person has refused — before any of it is sent
/// anywhere.
pub fn candidates(
    store: &TimelineStore,
    cache: &DbBridge,
    year: i32,
    today: NaiveDate,
    quiet: &Quiet,
    seals: &Seals,
) -> AppResult<Vec<Candidate>> {
    let span = when::parse(&year.to_string())
        .ok_or_else(|| AppError::General(format!("'{year}' is not a year")))?;
    let events = store.query(span, today)?;

    let mut out: Vec<Candidate> = Vec::new();
    let mut seen_nodes: Vec<String> = Vec::new();
    for event in &events {
        let node = event.container_node.clone().unwrap_or_else(|| event.node_id.clone());
        if seen_nodes.contains(&node) {
            continue;
        }
        seen_nodes.push(node.clone());

        let nudge = Nudge::on(event.happened_from.clone())
            .from_note(node.clone())
            .naming(event.links.iter().map(|l| l.node_id.clone()));
        if quiet::allow(vec![nudge], quiet, seals).is_empty() {
            continue;
        }

        let Some(content) = read_content(cache, &node) else {
            continue;
        };
        for text in onthisday::sentences(&content).into_iter().take(MOST_PER_DAY) {
            if quiet.hushes_line(&node, &text) {
                continue;
            }
            out.push(Candidate { day: event.happened_from.clone(), node_id: node.clone(), text });
        }
    }

    out.sort_by(|a, b| a.day.cmp(&b.day).then_with(|| a.node_id.cmp(&b.node_id)));
    Ok(thin(out))
}

/// Keep the year's shape while making the list fit.
///
/// Taking the first `MOST_OFFERED` would hand the model January and call it a
/// year, so this keeps every *n*-th sentence instead. The months stay in
/// proportion to how much was written in them, which is the one property a
/// year of someone's writing has that must not be lost.
fn thin(mut all: Vec<Candidate>) -> Vec<Candidate> {
    if all.len() <= MOST_OFFERED {
        return all;
    }
    let step = all.len() as f64 / MOST_OFFERED as f64;
    let kept: Vec<Candidate> =
        (0..MOST_OFFERED).map(|n| all[(n as f64 * step) as usize].clone()).collect();
    all.clear();
    kept
}

fn read_content(cache: &DbBridge, node_id: &str) -> Option<String> {
    cache
        .conn()
        .query_row(
            "SELECT COALESCE(content, '') FROM nodes WHERE id = ?1 OR stable_id = ?1",
            rusqlite::params![node_id],
            |row| row.get::<_, String>(0),
        )
        .ok()
        .filter(|content| !content.trim().is_empty())
}

/// What the model is asked, which is to point rather than to speak.
pub fn prompt(year: i32, candidates: &[Candidate]) -> String {
    let list = candidates
        .iter()
        .enumerate()
        .map(|(n, c)| format!("{n}\t{}\t{}", c.day, c.text))
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "Below are sentences a person wrote during {year}, one per line, as \
         `number<TAB>day<TAB>sentence`.\n\n\
         Choose the {MOST_KEPT} that carry the most weight — the ones that, read \
         together in order, would show this person their own year. Prefer what \
         changed, what was decided, what was felt, what was first or last. Pass \
         over routine work notes and anything that only repeats another line.\n\n\
         Answer with JSON and nothing else: {{\"keep\": [numbers]}}\n\
         Use only numbers from the list. Do not write any sentences of your own; \
         there is nowhere in the answer for them.\n\n{list}"
    )
}

/// The numbers the model chose, or nothing if the reply was not the JSON asked
/// for.
pub fn parse_reply(reply: &str) -> Option<Vec<usize>> {
    let start = reply.find('{')?;
    let end = reply.rfind('}')? + 1;
    let value: Value = serde_json::from_str(reply.get(start..end)?).ok()?;
    let keep = value.get("keep")?.as_array()?;
    Some(keep.iter().filter_map(|n| n.as_u64().map(|n| n as usize)).collect())
}

/// The year, from what the model pointed at.
///
/// A number that was never offered is dropped rather than guessed at: §16
/// Bước 6 calls a sentence that does not match the vault a ship-blocking
/// error, so nothing that cannot be traced to a candidate reaches a screen.
pub fn keep(candidates: &[Candidate], picked: &[usize]) -> Vec<Line> {
    let mut lines: Vec<Line> = Vec::new();
    for n in picked {
        let Some(candidate) = candidates.get(*n) else {
            continue;
        };
        if lines.iter().any(|l| l.text == candidate.text && l.node_id == candidate.node_id) {
            continue;
        }
        lines.push(Line {
            day: candidate.day.clone(),
            node_id: candidate.node_id.clone(),
            text: candidate.text.clone(),
        });
    }
    // In the order they were lived, whatever order they were chosen in.
    lines.sort_by(|a, b| a.day.cmp(&b.day).then_with(|| a.node_id.cmp(&b.node_id)));
    lines.truncate(MOST_KEPT);
    lines
}

/// Build one year.
#[allow(clippy::too_many_arguments)]
pub async fn a_year(
    provider: &dyn ChatProvider,
    model: &str,
    num_ctx: u32,
    store: &TimelineStore,
    cache: &DbBridge,
    year: i32,
    today: NaiveDate,
    quiet: &Quiet,
    seals: &Seals,
) -> AppResult<Vec<Line>> {
    let candidates = candidates(store, cache, year, today, quiet, seals)?;
    if candidates.is_empty() {
        return Ok(Vec::new());
    }
    // Nothing to choose between: hand back what there is rather than pay a
    // model to renumber a short list.
    if candidates.len() <= MOST_KEPT {
        return Ok(keep(&candidates, &(0..candidates.len()).collect::<Vec<_>>()));
    }

    let messages = vec![ChatMessage::new("user", prompt(year, &candidates))];
    let reply = provider
        .chat(ChatRequest { model, messages: &messages, temperature: Some(0.0), num_ctx, tools: None })
        .await?;
    let picked = parse_reply(&reply.content)
        .ok_or_else(|| AppError::General("the reply was not the JSON asked for".into()))?;
    Ok(keep(&candidates, &picked))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(day: &str, node: &str, text: &str) -> Candidate {
        Candidate { day: day.into(), node_id: node.into(), text: text.into() }
    }

    #[test]
    fn the_answer_has_nowhere_to_put_a_sentence_of_its_own() {
        let said = prompt(2026, &[candidate("2026-01-01", "Notes/a.md", "Hôm nay trời lạnh.")]);
        assert!(said.contains("{\"keep\": [numbers]}"), "{said}");
        assert!(said.contains("0\t2026-01-01\tHôm nay trời lạnh."), "{said}");
    }

    #[test]
    fn a_number_that_was_never_offered_is_dropped() {
        let offered = [candidate("2026-01-01", "Notes/a.md", "Một.")];
        assert_eq!(keep(&offered, &[0, 7, 99]).len(), 1, "only the one that exists");
        assert!(keep(&offered, &[7]).is_empty());
    }

    #[test]
    fn every_line_is_a_sentence_that_was_offered() {
        let offered = [
            candidate("2026-03-02", "Notes/b.md", "Chuyển nhà xong."),
            candidate("2026-01-05", "Notes/a.md", "Bắt đầu học đàn."),
        ];
        for line in keep(&offered, &[1, 0]) {
            assert!(
                offered.iter().any(|c| c.text == line.text && c.node_id == line.node_id),
                "{line:?} was not on the list"
            );
        }
    }

    #[test]
    fn the_year_comes_back_in_the_order_it_was_lived() {
        let offered = [
            candidate("2026-03-02", "Notes/b.md", "Chuyển nhà xong."),
            candidate("2026-01-05", "Notes/a.md", "Bắt đầu học đàn."),
            candidate("2026-11-20", "Notes/c.md", "Nghỉ việc."),
        ];
        let days: Vec<String> = keep(&offered, &[2, 0, 1]).into_iter().map(|l| l.day).collect();
        assert_eq!(days, ["2026-01-05", "2026-03-02", "2026-11-20"]);
    }

    #[test]
    fn the_same_sentence_chosen_twice_appears_once() {
        let offered = [candidate("2026-01-05", "Notes/a.md", "Bắt đầu học đàn.")];
        assert_eq!(keep(&offered, &[0, 0, 0]).len(), 1);
    }

    #[test]
    fn a_year_is_at_most_fifteen_lines() {
        let offered: Vec<Candidate> = (0..40)
            .map(|n| candidate(&format!("2026-01-{:02}", n % 28 + 1), "Notes/a.md", &format!("Câu số {n} của năm.")))
            .collect();
        assert_eq!(keep(&offered, &(0..40).collect::<Vec<_>>()).len(), MOST_KEPT);
    }

    #[test]
    fn a_reply_wrapped_in_chatter_still_reads() {
        assert_eq!(parse_reply("Sure! {\"keep\": [1, 4]} hope that helps").unwrap(), [1, 4]);
        assert_eq!(parse_reply("no json here"), None);
        assert_eq!(parse_reply("{\"keep\": []}").unwrap(), Vec::<usize>::new());
    }

    #[test]
    fn thinning_keeps_the_shape_of_the_year_not_its_first_months() {
        let all: Vec<Candidate> = (0..1200)
            .map(|n| {
                let month = n / 100 + 1;
                candidate(&format!("2026-{month:02}-01"), "Notes/a.md", &format!("Câu {n}."))
            })
            .collect();
        let thinned = thin(all);
        assert_eq!(thinned.len(), MOST_OFFERED);
        let months: std::collections::HashSet<&str> =
            thinned.iter().map(|c| &c.day[5..7]).collect();
        assert_eq!(months.len(), 12, "every month survives: {months:?}");
    }

    // ─── The gate, end to end against a real vault ───────────────────

    use std::sync::Mutex;

    use serde_json::json;

    use crate::models::node::NodeMetadata;
    use crate::models::syn::{ModelInfo, ProviderStatus, SynProvider};
    use crate::syn::provider::{ChatReply, StreamSink};
    use crate::timeline::store::catch_up;

    /// A model that answers with whatever it is told to, cheating included.
    struct Answering(&'static str);

    #[async_trait::async_trait]
    impl ChatProvider for Answering {
        fn id(&self) -> SynProvider {
            SynProvider::Ollama
        }
        async fn check_status(&self) -> AppResult<ProviderStatus> {
            unreachable!()
        }
        async fn list_models(&self) -> AppResult<Vec<ModelInfo>> {
            unreachable!()
        }
        async fn chat(&self, _: ChatRequest<'_>) -> AppResult<ChatReply> {
            Ok(ChatReply {
                content: self.0.to_string(),
                tool_calls: Vec::new(),
                usage: Default::default(),
                duration_ms: Some(10),
            })
        }
        async fn chat_streaming(&self, req: ChatRequest<'_>, _: &StreamSink<'_>) -> AppResult<ChatReply> {
            self.chat(req).await
        }
    }

    fn diary(day: &str, body: &str) -> NodeMetadata {
        NodeMetadata {
            id: format!("Notes/{day}.md"),
            node_type: "note".into(),
            title: day.into(),
            content: body.into(),
            properties: json!({ "date": day }),
            created_at: format!("{day}T00:00:00.000Z"),
            updated_at: format!("{day}T00:00:00.000Z"),
            timestamp: 0,
            blocks: None,
        }
    }

    const WROTE: &[(&str, &str)] = &[
        ("2026-01-05", "Bắt đầu học đàn, ngón tay đau nhưng vui. Thầy bảo cứ chậm thôi."),
        ("2026-03-02", "Chuyển nhà xong, cả ngày khiêng đồ với Minh và Lan."),
        ("2026-05-14", "Sáng nay đi bộ với bố, ông kể chuyện năm 54."),
        ("2026-07-11", "Hôm nay con biết đi. Cả nhà đứng xem mà không ai dám thở."),
        ("2026-11-20", "Nộp đơn nghỉ việc. Nhẹ hơn mình tưởng rất nhiều."),
    ];

    fn a_vault() -> (Mutex<crate::db::DbBridge>, TimelineStore) {
        let db = crate::db::DbBridge::new_in_memory_full().unwrap();
        for (day, body) in WROTE {
            db.upsert_node(&diary(day, body)).unwrap();
        }
        let cache = Mutex::new(db);
        let mut timeline = TimelineStore::open_in_memory().unwrap();
        catch_up(&cache, &mut timeline).unwrap();
        (cache, timeline)
    }

    /// §16 Bước 6's gate: every sentence shown matches the vault byte for
    /// byte, and the note it names is the note it came from.
    #[tokio::test]
    async fn every_sentence_of_the_year_is_in_the_vault_byte_for_byte() {
        let (cache, timeline) = a_vault();
        let db = cache.lock().unwrap();
        let today = NaiveDate::from_ymd_opt(2026, 12, 31).unwrap();

        let lines = a_year(
            &Answering(r#"{"keep": [0, 1, 2, 3, 4]}"#),
            "any",
            4096,
            &timeline,
            &db,
            2026,
            today,
            &Quiet::default(),
            &Seals::default(),
        )
        .await
        .unwrap();

        assert!(!lines.is_empty(), "the year said nothing");
        for line in &lines {
            let content = read_content(&db, &line.node_id).expect("the note it names");
            assert!(
                content.contains(&line.text),
                "«{}» is not in {} word for word",
                line.text,
                line.node_id
            );
        }
    }

    #[tokio::test]
    async fn a_model_that_answers_with_prose_gets_nothing_through() {
        let (cache, timeline) = a_vault();
        let db = cache.lock().unwrap();
        let today = NaiveDate::from_ymd_opt(2026, 12, 31).unwrap();
        let many: Vec<Candidate> =
            candidates(&timeline, &db, 2026, today, &Quiet::default(), &Seals::default()).unwrap();
        assert!(!many.is_empty());

        // The reply format has no room for a sentence, so the worst a model can
        // do is answer with numbers nobody offered.
        assert!(keep(&many, &[900, 901]).is_empty());
        // And an answer that is prose is not an answer at all.
        assert_eq!(parse_reply("Năm 2026 của bạn thật đáng nhớ!"), None);
    }

    #[tokio::test]
    async fn what_is_sealed_or_hushed_is_never_even_offered() {
        let (cache, timeline) = a_vault();
        let db = cache.lock().unwrap();
        let today = NaiveDate::from_ymd_opt(2026, 12, 31).unwrap();

        let vault = tempfile::tempdir().unwrap();
        let vault_path = vault.path().to_str().unwrap();
        crate::timeline::seal::write_period(vault_path, "2026-11", "2026-11").unwrap();
        let seals = crate::timeline::seal::Seals::read(&db, vault_path).unwrap();

        // One whole day set aside, and one single sentence of another.
        quiet::write_hush(
            vault_path,
            &quiet::Subject::Moment { node: "Notes/2026-03-02.md".into(), day: "2026-03-02".into() },
            None,
        )
        .unwrap();
        quiet::write_hush(
            vault_path,
            &quiet::Subject::Line {
                node: "Notes/2026-01-05.md".into(),
                line: quiet::line_id("Thầy bảo cứ chậm thôi."),
            },
            None,
        )
        .unwrap();
        let quiet = Quiet::read(&db, vault_path, "2026-12-31").unwrap();

        let offered = candidates(&timeline, &db, 2026, today, &quiet, &seals).unwrap();
        let said: Vec<&str> = offered.iter().map(|c| c.text.as_str()).collect();

        assert!(!said.iter().any(|t| t.contains("Nộp đơn nghỉ việc")), "sealed month: {said:?}");
        assert!(!said.iter().any(|t| t.contains("Chuyển nhà")), "hushed day: {said:?}");
        assert!(!said.contains(&"Thầy bảo cứ chậm thôi."), "hushed line: {said:?}");
        assert!(
            said.iter().any(|t| t.contains("Bắt đầu học đàn")),
            "the rest of that note stays: {said:?}"
        );
    }

    #[tokio::test]
    async fn a_short_year_is_handed_back_without_paying_a_model() {
        let (cache, timeline) = a_vault();
        let db = cache.lock().unwrap();
        let today = NaiveDate::from_ymd_opt(2026, 12, 31).unwrap();
        // `Answering` would panic on an unexpected call shape; it is never
        // asked, because five days of writing is already a year.
        let lines = a_year(
            &Answering("not json at all"),
            "any",
            4096,
            &timeline,
            &db,
            2026,
            today,
            &Quiet::default(),
            &Seals::default(),
        )
        .await
        .unwrap();
        assert!(!lines.is_empty());
        assert_eq!(lines.first().unwrap().day, "2026-01-05");
    }

    /// The gate on the real vault, read only and without paying a model: what
    /// would be offered, and is every line of it in the vault word for word.
    ///
    ///   SYN_PROBE_CACHE=/path/to/a/copy/of/vault_cache.db \\
    ///     cargo test --lib -- --ignored a_real_year_of_candidates --nocapture
    #[test]
    #[ignore = "needs a copy of a real vault cache; run by hand"]
    fn a_real_year_of_candidates() {
        let path = std::env::var("SYN_PROBE_CACHE").expect("a copy of a vault_cache.db");
        let conn = rusqlite::Connection::open(&path).expect("the cache");
        let db = crate::db::DbBridge::init_with_conn(conn).expect("its schema");
        let cache = Mutex::new(db);
        let mut timeline = TimelineStore::open_in_memory().unwrap();
        let built = catch_up(&cache, &mut timeline).expect("built from the vault");
        let db = cache.lock().unwrap();

        let year: i32 = timeline
            .all_items(NaiveDate::from_ymd_opt(9999, 12, 31).unwrap())
            .unwrap()
            .iter()
            .filter_map(|e| e.happened_from.get(..4).and_then(|y| y.parse::<i32>().ok()))
            .max()
            .expect("something in the vault has a date");
        let today = NaiveDate::from_ymd_opt(year, 12, 31).unwrap();

        let offered =
            candidates(&timeline, &db, year, today, &Quiet::default(), &Seals::default()).unwrap();
        let mut off_by_a_byte = 0;
        for candidate in &offered {
            let content = read_content(&db, &candidate.node_id).unwrap_or_default();
            if !content.contains(&candidate.text) {
                off_by_a_byte += 1;
                eprintln!("  NOT IN VAULT: {} «{}»", candidate.node_id, candidate.text);
            }
        }

        let days: std::collections::HashSet<&str> =
            offered.iter().map(|c| c.day.as_str()).collect();
        eprintln!("\n═══ a year in your own words, {year} ═══  {} events", built.items);
        eprintln!("  sentences offered:  {}", offered.len());
        eprintln!("  days they span:     {}", days.len());
        eprintln!("  not in the vault:   {off_by_a_byte}");
        for candidate in offered.iter().take(8) {
            eprintln!("  {} «{}»", candidate.day, candidate.text);
        }

        assert_eq!(off_by_a_byte, 0, "§16 Bước 6: a sentence that does not match is a blocker");
    }

    /// The whole thing, once, against the vault's own model.
    ///
    ///   SYN_PROBE_CACHE=... cargo test --lib -- --ignored a_real_year_chosen --nocapture
    #[tokio::test]
    #[ignore = "spends real API credit and needs a network; run by hand"]
    async fn a_real_year_chosen() {
        let path = std::env::var("SYN_PROBE_CACHE").expect("a copy of a vault_cache.db");
        let vault_path = std::env::var("SYN_EVAL_VAULT").unwrap_or_else(|_| {
            format!("{}/Documents/vault", std::env::var("HOME").unwrap_or_default())
        });
        let conn = rusqlite::Connection::open(&path).expect("the cache");
        let db = crate::db::DbBridge::init_with_conn(conn).expect("its schema");
        let cache = Mutex::new(db);
        let mut timeline = TimelineStore::open_in_memory().unwrap();
        catch_up(&cache, &mut timeline).expect("built from the vault");
        let db = cache.lock().unwrap();

        let settings =
            crate::syn::settings::load_settings(&vault_path).expect("the real Syn settings");
        let config = crate::timeline::extract::Config {
            model: std::env::var("SYN_EVAL_MODEL").ok(),
            provider: std::env::var("SYN_EVAL_PROVIDER")
                .ok()
                .and_then(|p| serde_json::from_value(serde_json::Value::from(p)).ok()),
            ..Default::default()
        };
        let (settings, model) = crate::timeline::extract::reader(&config, &settings);
        let model = model.expect("a model");
        let provider = crate::syn::provider::for_settings(
            &settings,
            crate::secrets::SecretManager::get_syn_api_key(None, settings.provider.key_slot()),
        );

        let year = 2026;
        let today = NaiveDate::from_ymd_opt(year, 12, 31).unwrap();
        let lines = a_year(
            provider.as_ref(),
            &model,
            settings.num_ctx,
            &timeline,
            &db,
            year,
            today,
            &Quiet::default(),
            &Seals::default(),
        )
        .await
        .expect("a year");

        eprintln!("\n═══ {year}, chosen by {model} ═══\n");
        for line in &lines {
            let content = read_content(&db, &line.node_id).unwrap_or_default();
            let exact = if content.contains(&line.text) { " " } else { "✗" };
            eprintln!("{exact} {}  {}", line.day, line.text);
        }
        assert!(
            lines.iter().all(|l| read_content(&db, &l.node_id).unwrap_or_default().contains(&l.text)),
            "every line has to be in the vault word for word"
        );
    }
}
