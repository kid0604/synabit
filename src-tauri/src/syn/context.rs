//! Keeping a run inside the model's window.
//!
//! # What went wrong without this
//!
//! Nothing measured how full the window was. Inside a run the history only
//! grew: every tool result — up to `tools::MAX_RESULT_CHARS`, forty thousand
//! characters — went back to the provider on every later round, whether or not
//! the model still needed it. Between turns the conversation was cut at a
//! message count, which says nothing about size: fifty short messages fit
//! easily, five long ones did not.
//!
//! The cost of that was different on each side of the wire, and bad on both:
//!
//! * **Ollama** truncates a prompt that does not fit, silently, from the
//!   front — which is where the system prompt is. A long run on a local model
//!   forgot who it was and what it was allowed to do, with nothing anywhere
//!   saying so.
//! * **A hosted model** charges for every token sent, every round. A result
//!   read in round two and never looked at again was paid for ten more times.
//!
//! # What this does, cheapest first
//!
//! 1. **Measure.** Characters, converted to tokens at a rate calibrated from
//!    what the provider reports it was actually sent (`Usage::input`). The rate
//!    starts at four characters a token and moves toward what this model and
//!    this language really cost — Vietnamese with its diacritics usually costs
//!    more than English.
//! 2. **Shorten old tool results.** A result the model has already answered
//!    from, two rounds ago, is replaced by its opening and a note saying how to
//!    get it back. The newest round is never touched: the model has not read
//!    it yet.
//! 3. **Summarise the earlier conversation.** When shortening is not enough,
//!    the turns before the current question are summarised in one call and
//!    replaced by the summary — where they used to be dropped outright.
//!
//! The system prompt is never touched by any of it.

use crate::syn::provider::ChatMessage;

/// Fill the window to this share, and no more.
///
/// The rest is room for what the round adds: the reply itself, and the tool
/// results that come back before the next request. Seventy per cent leaves
/// enough for a long answer on an 8,192-token local model and is barely a
/// constraint on a hosted one.
pub const FILL: f64 = 0.70;

/// Where the characters-per-token estimate starts, before anything is measured.
pub const DEFAULT_CHARS_PER_TOKEN: f64 = 4.0;

/// A tool result at or under this size is left alone however old it is. A
/// stub would not be much shorter.
pub const KEEP_WHOLE_CHARS: usize = 1_500;

/// How much of a shortened result is kept.
pub const KEPT_HEAD_CHARS: usize = 600;

/// The conversation before the current question is summarised only if it is
/// at least this many messages. Fewer are cheaper to keep than to summarise.
pub const MIN_TO_SUMMARISE: usize = 4;

/// Characters in what would be sent.
pub fn chars_in(messages: &[ChatMessage]) -> usize {
    messages
        .iter()
        .map(|m| {
            m.content.chars().count()
                + m.tool_calls
                    .as_ref()
                    .map(|calls| {
                        calls
                            .iter()
                            .map(|c| c.function.name.len() + c.function.arguments.to_string().len())
                            .sum::<usize>()
                    })
                    .unwrap_or(0)
        })
        .sum()
}

/// Tokens, estimated from characters at `chars_per_token`.
pub fn estimate_tokens(chars: usize, chars_per_token: f64) -> u64 {
    (chars as f64 / chars_per_token.max(0.5)).ceil() as u64
}

/// A better rate, having been told what `chars` actually cost.
///
/// Moved most of the way toward the measurement rather than all of it: one
/// request with a picture in it, or a provider that counts its own wrapping,
/// should not swing the next estimate by half. Clamped because a rate outside
/// this range is a measurement of something other than text.
pub fn calibrate(current: f64, chars: usize, input_tokens: u64) -> f64 {
    if chars == 0 || input_tokens == 0 {
        return current;
    }
    let measured = (chars as f64 / input_tokens as f64).clamp(1.5, 6.0);
    (current * 0.3 + measured * 0.7).clamp(1.5, 6.0)
}

/// Tokens this history may take for the next request.
pub fn allowance(window_tokens: u32) -> u64 {
    (window_tokens as f64 * FILL) as u64
}

/// Shorten tool results older than `newest_round_from` until about `free`
/// characters have been given back. Oldest first. Returns what was freed.
///
/// A shortened result is still a result for the call it answers — the pairing
/// every provider insists on is kept — and it says what it was and how to get
/// the rest back, so a model that needs it can ask again rather than guess.
pub fn shorten_old_results(messages: &mut [ChatMessage], newest_round_from: usize, free: usize) -> usize {
    let mut freed = 0usize;
    for message in messages.iter_mut().take(newest_round_from) {
        if freed >= free {
            break;
        }
        if message.role != "tool" {
            continue;
        }
        let length = message.content.chars().count();
        if length <= KEEP_WHOLE_CHARS || is_shortened(&message.content) {
            continue;
        }
        let head: String = message.content.chars().take(KEPT_HEAD_CHARS).collect();
        let stub = serde_json::json!({
            "shortened": format!(
                "This result was {length} characters and has been shortened to keep the \
                 conversation inside the model's window. Its opening is below. If you need \
                 the rest, call the tool again."
            ),
            "opening": head,
        })
        .to_string();
        freed += length.saturating_sub(stub.chars().count());
        message.content = stub;
    }
    freed
}

fn is_shortened(content: &str) -> bool {
    content.starts_with("{\"opening\"") || content.starts_with("{\"shortened\"")
}

/// The earlier conversation, as a range of `messages`: after the system
/// prompt, before the message that asked the current question.
///
/// Only plain turns are in it. Earlier turns come back from the conversation
/// file as text — their tool calls were never stored as calls — so nothing in
/// the range answers anything outside it, and replacing it breaks no pairing.
pub fn earlier_conversation(messages: &[ChatMessage]) -> Option<std::ops::Range<usize>> {
    let start = usize::from(messages.first().is_some_and(|m| m.role == "system"));
    let question = messages.iter().rposition(|m| m.role == "user")?;
    let range = start..question;
    let plain = messages[range.clone()]
        .iter()
        .all(|m| (m.role == "user" || m.role == "assistant") && m.tool_calls.is_none() && m.tool_call_id.is_none());
    (range.len() >= MIN_TO_SUMMARISE && plain).then_some(range)
}

/// What the model is asked, to summarise the earlier conversation.
pub fn summary_request(earlier: &[ChatMessage]) -> Vec<ChatMessage> {
    let transcript = earlier
        .iter()
        .map(|m| {
            let who = if m.role == "user" { "User" } else { "Assistant" };
            format!("{who}: {}", m.content.trim())
        })
        .collect::<Vec<_>>()
        .join("\n\n");
    vec![
        ChatMessage {
            role: "system".to_string(),
            content: "You condense a conversation so it can be continued in less space. Keep \
                      every fact, decision, name, number, date, file or note name, link and \
                      open question; keep what the user asked for and what was promised. Drop \
                      pleasantries and repetition. Write it as short plain notes in the \
                      language the conversation was in. Do not answer anything, and do not \
                      add anything that was not said."
                .to_string(),
            tool_calls: None,
            tool_call_id: None,
            images: None,
        },
        ChatMessage {
            role: "user".to_string(),
            content: transcript,
            tool_calls: None,
            tool_call_id: None,
            images: None,
        },
    ]
}

/// Put the summary where the earlier conversation was.
///
/// As a user turn, framed as a record rather than a request, because every
/// provider accepts a user turn in that position and not every one accepts a
/// second system message or two assistant turns in a row.
pub fn replace_with_summary(messages: &mut Vec<ChatMessage>, range: std::ops::Range<usize>, summary: &str) {
    let count = range.len();
    let note = ChatMessage {
        role: "user".to_string(),
        content: format!(
            "[Earlier in this conversation — {count} messages, condensed to save space. This is a \
             record of what was said, not a new request.]\n{}",
            summary.trim()
        ),
        tool_calls: None,
        tool_call_id: None,
        images: None,
    };
    messages.splice(range, std::iter::once(note));
}

/// Drop the earliest turns until about `free` characters are gone, when
/// summarising is not possible. Whole turns only, and never the question.
pub fn drop_earliest(messages: &mut Vec<ChatMessage>, range: std::ops::Range<usize>, free: usize) -> usize {
    let mut freed = 0usize;
    let mut end = range.start;
    while end < range.end && freed < free {
        freed += messages[end].content.chars().count();
        end += 1;
    }
    messages.drain(range.start..end);
    freed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::syn::{ToolCall, ToolCallFunction};

    fn msg(role: &str, content: &str) -> ChatMessage {
        ChatMessage { role: role.into(), content: content.into(), tool_calls: None, tool_call_id: None, images: None }
    }

    fn asked(tool: &str) -> ChatMessage {
        ChatMessage {
            role: "assistant".into(),
            content: String::new(),
            tool_calls: Some(vec![ToolCall {
                id: Some("c".into()),
                function: ToolCallFunction { name: tool.into(), arguments: serde_json::json!({}) },
                thought_signature: None,
            }]),
            tool_call_id: None,
            images: None,
        }
    }

    fn result(content: &str) -> ChatMessage {
        ChatMessage { role: "tool".into(), content: content.into(), tool_calls: None, tool_call_id: Some("c".into()), images: None }
    }

    #[test]
    fn the_rate_moves_toward_what_was_measured_and_stays_sane() {
        let rate = calibrate(4.0, 3_000, 1_500);
        assert!(rate > 2.0 && rate < 4.0, "{rate}");
        assert_eq!(calibrate(4.0, 0, 100), 4.0, "nothing measured, nothing learned");
        assert_eq!(calibrate(4.0, 100_000, 1), 6.0 * 0.7 + 4.0 * 0.3, "a nonsense measurement is clamped");
    }

    /// Old results give back their room; the newest round, unread, keeps all of it.
    #[test]
    fn old_results_are_shortened_and_the_newest_are_not() {
        let big = "x".repeat(10_000);
        let mut messages = vec![
            msg("system", "sys"),
            msg("user", "q"),
            asked("get_node"),
            result(&big),
            asked("get_node"),
            result(&big),
        ];
        let freed = shorten_old_results(&mut messages, 4, usize::MAX);
        assert!(freed > 9_000, "{freed}");
        assert!(messages[3].content.contains("\"shortened\""));
        assert_eq!(messages[3].tool_call_id.as_deref(), Some("c"), "still answers its call");
        assert_eq!(messages[5].content, big, "the newest round is untouched");
        assert_eq!(messages[0].content, "sys");
    }

    #[test]
    fn a_small_or_already_shortened_result_is_left_alone() {
        let mut messages = vec![asked("x"), result("short"), asked("x"), result(&"y".repeat(5_000))];
        shorten_old_results(&mut messages, 4, usize::MAX);
        let once = messages[3].content.clone();
        assert_eq!(messages[1].content, "short");
        assert_eq!(shorten_old_results(&mut messages, 4, usize::MAX), 0, "twice frees nothing");
        assert_eq!(messages[3].content, once);
    }

    #[test]
    fn it_stops_once_enough_is_free() {
        let big = "x".repeat(10_000);
        let mut messages = vec![asked("a"), result(&big), asked("b"), result(&big), msg("user", "q")];
        shorten_old_results(&mut messages, 4, 100);
        assert!(messages[1].content.contains("shortened"));
        assert_eq!(messages[3].content, big, "the second was not needed");
    }

    #[test]
    fn the_earlier_conversation_is_what_comes_before_the_question() {
        let messages = vec![
            msg("system", "s"),
            msg("user", "a"),
            msg("assistant", "b"),
            msg("user", "c"),
            msg("assistant", "d"),
            msg("user", "now"),
            asked("get_node"),
            result("r"),
        ];
        assert_eq!(earlier_conversation(&messages), Some(1..5));
        assert_eq!(earlier_conversation(&messages[..3]), None, "too short to be worth it");
    }

    #[test]
    fn a_summary_takes_the_place_of_what_it_summarises() {
        let mut messages = vec![
            msg("system", "s"),
            msg("user", "a"),
            msg("assistant", "b"),
            msg("user", "c"),
            msg("assistant", "d"),
            msg("user", "now"),
        ];
        replace_with_summary(&mut messages, 1..5, "- a, b, c, d");
        assert_eq!(messages.len(), 3);
        assert_eq!(messages[0].content, "s");
        assert!(messages[1].content.contains("4 messages") && messages[1].content.contains("- a, b, c, d"));
        assert_eq!(messages[2].content, "now");
    }

    #[test]
    fn the_request_for_a_summary_carries_the_whole_exchange() {
        let asked = summary_request(&[msg("user", "Minh ở đâu?"), msg("assistant", "Hà Nội.")]);
        assert_eq!(asked.len(), 2);
        assert!(asked[1].content.contains("User: Minh ở đâu?") && asked[1].content.contains("Assistant: Hà Nội."));
    }

    #[test]
    fn dropping_takes_whole_turns_from_the_front() {
        let mut messages = vec![msg("system", "s"), msg("user", "aaaa"), msg("assistant", "bbbb"), msg("user", "q")];
        let freed = drop_earliest(&mut messages, 1..3, 3);
        assert_eq!(freed, 4);
        assert_eq!(messages.iter().map(|m| m.content.as_str()).collect::<Vec<_>>(), vec!["s", "bbbb", "q"]);
    }
}
