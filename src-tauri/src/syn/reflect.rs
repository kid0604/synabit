//! Looking back at one exchange and asking whether anything in it should last.
//!
//! # What this is for, and what it is not
//!
//! `remember` handles being told. Somebody who says "nhớ giùm tao là tao không
//! họp sau 4h" has made a request, and a request does not need reviewing.
//!
//! This is the other half: what Syn *notices*. Those are inferences, they are
//! wrong a fair fraction of the time, and writing them straight into the vault
//! is how a folder of the user's own files fills up with a model's guesses
//! about them. So this proposes into a queue and somebody decides. See
//! `proposal.rs` for why the queue is not in the vault as nodes.
//!
//! # What it costs
//!
//! One extra completion per answered message, with no tools and a small prompt
//! — the last exchange plus the memories that already exist. Against a main
//! turn of five or six thousand characters it is roughly a fifth as much, and
//! it is skippable: `memory_reflection` in Syn settings turns it off, and it
//! never runs for a run that failed or was cancelled.
//!
//! It is deliberately not free and deliberately not hidden. A feature that
//! silently doubles the number of requests an app makes is a feature somebody
//! discovers on a bill.

use serde::Deserialize;

use crate::syn::memory::Memory;
use crate::syn::proposal::Proposal;
use crate::syn::provider::{ChatMessage, ChatProvider, ChatRequest};

/// The most a single exchange may propose.
///
/// Two. A model asked what is worth remembering from a chat about lunch will
/// find five things, and the tray is only reviewable while it is short.
const MAX_PER_EXCHANGE: usize = 2;

/// How much of the exchange is shown to the reflector.
///
/// Enough to judge, not enough to be its own conversation.
const MAX_EXCHANGE_CHARS: usize = 4_000;

/// What the model is asked to return, before it becomes a `Proposal`.
#[derive(Deserialize, Debug)]
struct Suggested {
    body: String,
    #[serde(default)]
    kind: Option<String>,
    #[serde(default)]
    subject: Option<String>,
    #[serde(default)]
    confidence: Option<f64>,
    #[serde(default)]
    because: Option<String>,
    #[serde(default)]
    supersedes: Option<String>,
    #[serde(default)]
    from_correction: Option<bool>,
}

fn cut(text: &str, limit: usize) -> String {
    text.chars().take(limit).collect()
}

/// The question put to the model.
fn prompt(user: &str, assistant: &str, existing: &[Memory]) -> String {
    let known = if existing.is_empty() {
        "(nothing yet)".to_string()
    } else {
        existing
            .iter()
            .take(40)
            .map(|m| format!("- {}", m.body))
            .collect::<Vec<_>>()
            .join("\n")
    };

    format!(
        "You are reviewing one exchange between a user and their assistant, to \
         decide whether anything in it is worth remembering about this person \
         after the conversation ends.\n\n\
         Already remembered — do not propose any of these again:\n{known}\n\n\
         The exchange:\n\
         USER: {}\n\
         ASSISTANT: {}\n\n\
         Propose only things that will still be true and useful in six months: \
         a standing preference, a fact about them or someone close to them, an \
         instruction about how they want to be helped. Do NOT propose:\n\
         - anything that belongs in their vault as a note, task or event;\n\
         - one-off details of what was just done;\n\
         - anything you were explicitly asked to remember, which is already saved;\n\
         - a restatement of something in the list above.\n\n\
         Two things change the answer:\n\
         - If the user corrected the assistant here — said that something it \
         assumed or did was wrong — that is the strongest evidence this app \
         ever gets. What the correction implies about them is usually worth \
         keeping. Set \"from_correction\": true on it.\n\
         - If what you want to propose updates or contradicts something in the \
         already-remembered list, do not propose it as new. Set \"supersedes\" \
         to the exact text of the entry it replaces, so accepting it retires \
         that one instead of leaving both.\n\n\
         Most exchanges are worth nothing. Returning an empty list is the \
         normal answer and is always acceptable.\n\n\
         Reply with ONLY a JSON array, at most {MAX_PER_EXCHANGE} entries, no \
         other text. Each entry:\n\
         {{\"body\": \"the memory, in the user's own language\", \
         \"kind\": \"fact|preference|instruction|relationship|project\", \
         \"subject\": \"who or what it is about, or null\", \
         \"confidence\": 0.0 to 1.0, \
         \"because\": \"the evidence in this exchange, briefly\", \
         \"supersedes\": \"the exact text of the entry this replaces, or null\", \
         \"from_correction\": true or false}}",
        cut(user, MAX_EXCHANGE_CHARS),
        cut(assistant, MAX_EXCHANGE_CHARS),
    )
}

/// Pull a JSON array out of whatever the model actually said.
///
/// Models fence their JSON, apologise before it, explain after it, and
/// occasionally return a bare object instead of an array. This is the part that
/// breaks, so it is the part with the tests: everything else here is a string
/// and an HTTP call.
///
/// Anything that is not recoverable is an empty list rather than an error. A
/// reflection that could not be read costs the user nothing; a message that
/// failed because a reflection could not be read costs them the answer they
/// were waiting for.
fn extract(reply: &str) -> Vec<Suggested> {
    let trimmed = reply.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }

    // Inside a fence, if there is one; the fence may or may not name a language.
    let unfenced = match trimmed.split_once("```") {
        Some((_, rest)) => {
            let rest = rest.strip_prefix("json").unwrap_or(rest);
            rest.split("```").next().unwrap_or(rest).trim()
        }
        None => trimmed,
    };

    // The widest bracketed span, so prose either side is ignored.
    let array = match (unfenced.find('['), unfenced.rfind(']')) {
        (Some(start), Some(end)) if end > start => &unfenced[start..=end],
        _ => {
            // A single object where an array was asked for is a common near
            // miss, and refusing it would throw away a correct suggestion.
            match (unfenced.find('{'), unfenced.rfind('}')) {
                (Some(start), Some(end)) if end > start => {
                    return serde_json::from_str::<Suggested>(&unfenced[start..=end])
                        .map(|one| vec![one])
                        .unwrap_or_default();
                }
                _ => return Vec::new(),
            }
        }
    };

    serde_json::from_str::<Vec<Suggested>>(array).unwrap_or_else(|e| {
        log::warn!("[Syn] Reflection did not return usable JSON: {e}");
        Vec::new()
    })
}

/// Ask what is worth keeping from one exchange.
///
/// Returns proposals for the queue, never memories. Failure of any kind is an
/// empty list: this runs after the user already has their answer, and nothing
/// it can do is worth interrupting them for.
#[allow(clippy::too_many_arguments)]
pub async fn reflect(
    provider: &dyn ChatProvider,
    model: &str,
    num_ctx: u32,
    user_message: &str,
    assistant_reply: &str,
    existing: &[Memory],
    run_id: &str,
    conversation_id: Option<&str>,
) -> Vec<Proposal> {
    if user_message.trim().is_empty() || assistant_reply.trim().is_empty() {
        return Vec::new();
    }

    let messages = vec![ChatMessage::new(
        "user",
        prompt(user_message, assistant_reply, existing),
    )];

    let reply = match provider
        .chat(ChatRequest {
            model,
            messages: &messages,
            // Low, because this is an extraction task and not a creative one.
            temperature: Some(0.2),
            num_ctx,
            tools: None,
        })
        .await
    {
        Ok(reply) => reply,
        Err(e) => {
            log::warn!("[Syn] Reflection failed, proposing nothing: {e}");
            return Vec::new();
        }
    };

    into_proposals(
        extract(&reply.content),
        existing,
        run_id,
        conversation_id,
        &chrono::Utc::now().to_rfc3339(),
    )
}

/// What the model returned, turned into proposals the tray can hold.
///
/// Separated from `reflect` so it can be tested without a model. The judgement
/// in here is not arithmetic — which fields to distrust, and how far — and it
/// was reachable only through a network call, which is how the "null" subject
/// bug survived as long as it did.
fn into_proposals(
    suggested: Vec<Suggested>,
    existing: &[Memory],
    run_id: &str,
    conversation_id: Option<&str>,
    now: &str,
) -> Vec<Proposal> {
    suggested
        .into_iter()
        .filter(|s| !s.body.trim().is_empty())
        .take(MAX_PER_EXCHANGE)
        .map(|s| Proposal {
            id: uuid::Uuid::new_v4().to_string(),
            body: s.body.trim().to_string(),
            kind: s
                .kind
                .map(|k| k.trim().to_lowercase())
                .filter(|k| !k.is_empty())
                .unwrap_or_else(|| "fact".to_string()),
            subject: s.subject.map(|v| v.trim().to_string()).filter(|v| {
                // Models write the string "null" surprisingly often.
                !v.is_empty() && !v.eq_ignore_ascii_case("null")
            }),
            confidence: s.confidence.unwrap_or(0.5).clamp(0.0, 1.0),
            because: s
                .because
                .map(|b| b.trim().to_string())
                .filter(|b| !b.is_empty())
                .unwrap_or_else(|| "inferred from the conversation".to_string()),
            source_run: run_id.to_string(),
            conversation_id: conversation_id.map(str::to_string),
            // Only kept when it names something actually remembered. A model
            // that invents an entry to supersede would otherwise have the user
            // retire a memory that never existed — and the accept path, finding
            // nothing to retire, would write the new one anyway. A silent
            // half-apply is worse than ignoring the field.
            supersedes: s.supersedes.map(|v| v.trim().to_string()).filter(|v| {
                !v.is_empty()
                    && !v.eq_ignore_ascii_case("null")
                    && existing
                        .iter()
                        .any(|m| m.body.trim().to_lowercase() == v.to_lowercase())
            }),
            from_correction: s.from_correction.unwrap_or(false),
            proposed_at: now.to_string(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn known(body: &str) -> Memory {
        Memory {
            id: format!("SynMemory/{body}.md"),
            title: body.to_string(),
            body: body.to_string(),
            kind: "preference".into(),
            subject: None,
            confidence: 0.8,
            pinned: false,
            first_seen: "2026-09-01".into(),
            last_confirmed: "2026-09-01".into(),
            source_run: None,
            source_nodes: Vec::new(),
            review_after: None,
            supersedes: None,
        }
    }

    fn suggested(body: &str, supersedes: Option<&str>) -> Suggested {
        Suggested {
            body: body.to_string(),
            kind: None,
            subject: None,
            confidence: None,
            because: None,
            supersedes: supersedes.map(str::to_string),
            from_correction: None,
        }
    }

    /// A replacement has to name something that is actually there.
    ///
    /// The accept path resolves `supersedes` from text to an id and retires
    /// what it finds. Given a name for a memory that does not exist it finds
    /// nothing, retires nothing, and writes the new memory anyway — so the user
    /// is told one thing replaced another and ends up with both. Dropping the
    /// claim here is the only place that half-apply can be prevented.
    #[test]
    fn a_replacement_naming_nothing_real_is_not_a_replacement() {
        let existing = [known("Thích cà phê đen.")];

        let real = into_proposals(
            vec![suggested("Thích trà hơn cà phê.", Some("Thích cà phê đen."))],
            &existing,
            "run-1",
            None,
            "2026-09-04T00:00:00Z",
        );
        assert_eq!(
            real[0].supersedes.as_deref(),
            Some("Thích cà phê đen."),
            "naming a real memory is kept"
        );

        let invented = into_proposals(
            vec![suggested("Thích trà.", Some("Ghét cà phê, uống trà từ 2019."))],
            &existing,
            "run-1",
            None,
            "2026-09-04T00:00:00Z",
        );
        assert!(
            invented[0].supersedes.is_none(),
            "a memory nobody has is not superseded"
        );

        for null in ["null", "NULL", "  "] {
            let noise = into_proposals(
                vec![suggested("Điều gì đó.", Some(null))],
                &existing,
                "run-1",
                None,
                "2026-09-04T00:00:00Z",
            );
            assert!(noise[0].supersedes.is_none(), "`{null}` is not a memory");
        }
    }

    /// A correction reports itself, and is false unless it does.
    #[test]
    fn a_correction_is_marked_and_nothing_else_is() {
        let mut asked = suggested("Người dùng dùng macOS, không phải Windows.", None);
        asked.from_correction = Some(true);

        let out = into_proposals(
            vec![asked, suggested("Người dùng thích trà.", None)],
            &[],
            "run-1",
            None,
            "2026-09-04T00:00:00Z",
        );
        assert!(out[0].from_correction, "the one that said so");
        assert!(!out[1].from_correction, "and only that one");
    }

    #[test]
    fn a_plain_array_is_read() {
        let out = extract(r#"[{"body":"họp buổi sáng","kind":"preference"}]"#);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].body, "họp buổi sáng");
    }

    /// The shape models actually return most of the time.
    #[test]
    fn a_fenced_array_is_read() {
        let out = extract("```json\n[{\"body\": \"thích trà\"}]\n```");
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].body, "thích trà");

        let unlabelled = extract("```\n[{\"body\": \"x\"}]\n```");
        assert_eq!(unlabelled.len(), 1);
    }

    #[test]
    fn prose_either_side_is_ignored() {
        let out = extract(
            "Sure! Here is what I would remember:\n\
             [{\"body\": \"sống ở Hà Nội\", \"confidence\": 0.9}]\n\
             Let me know if you want changes.",
        );
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].confidence, Some(0.9));
    }

    /// A near miss worth recovering: one object where an array was asked for.
    #[test]
    fn a_bare_object_is_taken_as_a_list_of_one() {
        let out = extract(r#"{"body": "ghét hành", "kind": "preference"}"#);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].body, "ghét hành");
    }

    /// The normal answer. Most exchanges are worth nothing.
    #[test]
    fn nothing_worth_remembering_reads_as_nothing() {
        for reply in ["[]", "```json\n[]\n```", "", "   ", "Nothing worth remembering."] {
            assert!(extract(reply).is_empty(), "`{reply}` should propose nothing");
        }
    }

    /// Unparseable output costs the user nothing, because the answer they were
    /// waiting for has already been delivered.
    #[test]
    fn broken_json_proposes_nothing_rather_than_failing() {
        for reply in [
            "[{\"body\": \"x\"",
            "[not json at all]",
            "```json\n{{{\n```",
            "[{'body': 'single quotes'}]",
        ] {
            assert!(extract(reply).is_empty(), "`{reply}` should propose nothing");
        }
    }

    #[test]
    fn the_prompt_names_what_is_already_known_so_it_is_not_proposed_again() {
        let existing = vec![crate::syn::memory::Memory::from_node(
            &crate::models::node::NodeMetadata {
                id: "SynMemory/a.md".into(),
                node_type: "syn_memory".into(),
                title: "a".into(),
                content: "Tên là Minh".into(),
                properties: serde_json::json!({}),
                created_at: String::new(),
                updated_at: String::new(),
                timestamp: 0,
                blocks: None,
            },
        )];

        let p = prompt("chào", "chào bạn", &existing);
        assert!(p.contains("Tên là Minh"));
        assert!(p.contains("do not propose any of these again"));
        assert!(p.contains("Returning an empty list is the normal answer"));
    }

    #[test]
    fn an_empty_exchange_is_not_worth_a_request() {
        let p = prompt("", "", &[]);
        assert!(p.contains("(nothing yet)"));
    }
}

/// What does reflecting after every turn actually cost, next to the turn?
///
/// A timing probe, not a pass/fail assertion: it prints characters, because
/// the question it settles — "is running this every message reasonable?" — is
/// a ratio question, and a ratio nobody has measured is an opinion.
///
/// ```bash
/// cargo test --lib what_reflection_costs -- --ignored --nocapture
/// ```
#[cfg(test)]
mod what_reflection_costs {
    use super::*;

    #[test]
    #[ignore = "a sizing probe, not a pass/fail assertion"]
    fn beside_the_chat_turn_it_sits_after() {
        // Forty memories is the cap the reflection prompt lists, so this is the
        // worst case it can reach, not a typical one.
        let memories: Vec<Memory> = (0..40)
            .map(|i| Memory {
                id: format!("SynMemory/m{i}.md"),
                title: format!("m{i}"),
                body: format!("Một điều đã nhớ về người dùng, số {i}, dài cỡ một câu thật."),
                kind: "preference".to_string(),
                subject: None,
                confidence: 0.8,
                pinned: i < 16,
                first_seen: "2026-09-04".to_string(),
                last_confirmed: "2026-09-04".to_string(),
                source_run: None,
                source_nodes: Vec::new(),
                review_after: None,
                supersedes: None,
            })
            .collect();

        let user = "Đặt lịch họp với Minh chiều mai lúc 5h được không?";
        let assistant = "Minh không họp sau 16h, nên 17h sẽ không được. \
                         Tao đề xuất 15h cùng ngày, được không?";

        let reflection = prompt(user, assistant, &memories);

        // The other side: what the chat turn itself sends. The system prompt at
        // its declared budget, plus the tool schemas, which are sent in full on
        // every single turn.
        let system = crate::syn::prompt::PromptPlan::for_chat(crate::syn::prompt::ChatPrompt {
            context: &"x".repeat(12_000),
            personality: "auto",
            custom: None,
            memory: Some(&"y".repeat(crate::syn::memory::MEMORY_BUDGET_CHARS)),
            budget_chars: crate::syn::prompt::DEFAULT_BUDGET_CHARS,
        })
        .render();
        let tools = serde_json::to_string(&crate::syn::tools::get_tool_definitions())
            .expect("schemas serialise");

        let turn = system.chars().count() + tools.chars().count();
        let reflect_chars = reflection.chars().count();

        eprintln!("\n═══ one chat turn vs the reflection that follows it ═══");
        eprintln!("  chat system prompt   {:>7} chars", system.chars().count());
        eprintln!("  tool schemas         {:>7} chars", tools.chars().count());
        eprintln!("  ── chat turn, input   {:>7} chars  (before history)", turn);
        eprintln!("  reflection prompt    {:>7} chars  (40 memories, the cap)", reflect_chars);
        eprintln!(
            "\n  reflection is {:.0}% of one turn's input, and doubles the request count.\n",
            100.0 * reflect_chars as f64 / turn as f64
        );
    }
}
