//! Asking a model to **point** rather than to speak.
//!
//! What is left of "a year in your own words" after it became a question:
//! `events when:this-year | explode sentences | ask 15`. The reading of the
//! year moved into the language — `explode` gathers the sentences, the consent
//! layer drops what is sealed or hushed before any of them are read, and
//! `pipeline::keeping_picked` carries the three rules this module learnt
//! first: a number nobody offered is dropped, the same number twice is one
//! row, and they come back in the order they were offered.
//!
//! This is the protocol itself, and it is shared: the model is handed a
//! numbered list and answers with numbers. **There is nowhere in the reply to
//! put prose.** That is not a style choice — it is what makes an invented
//! sentence impossible rather than unlikely, which is the only basis on which
//! a model is allowed near somebody's own writing at all.

use serde_json::Value;

/// How many sentences a year is made of. §7.6 says ten to fifteen.
/// The same asking, for any numbered list of a person's own lines.
///
/// `| ask n` in the query language reaches this (`commands::nexus`), so there
/// is one protocol and not two: a numbered list goes out and numbers come
/// back, and **the reply has nowhere to put prose**. That is not a style
/// choice — it is what makes a made-up sentence impossible rather than
/// unlikely.
pub fn prompt_for(lines: &[String], room: usize) -> String {
    prompt_about("a person wrote", lines, room)
}

fn prompt_about(whose: &str, lines: &[String], room: usize) -> String {
    let list = lines
        .iter()
        .enumerate()
        .map(|(n, line)| format!("{n}\t{line}"))
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "Below are sentences {whose}, one per line, beginning with a number.\n\n\
         Choose the {room} that carry the most weight — the ones that, read \
         together in order, would show this person their own life. Prefer what \
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

#[cfg(test)]
mod tests {
    use super::*;

    /// The protocol, which is the whole of what is left here: a numbered list
    /// goes out and numbers come back, and **the reply has nowhere to put
    /// prose**. That is what makes an invented sentence impossible rather than
    /// unlikely.
    ///
    /// The choosing itself moved to `| ask` — `pipeline::keeping_picked` — and
    /// carried the three rules this module learnt first: a number nobody
    /// offered is dropped, the same number twice is one row, and they come
    /// back in the order they were offered.
    #[test]
    fn the_answer_has_nowhere_to_put_a_sentence_of_its_own() {
        let said = prompt_for(&["2026-01-01\tHôm nay trời lạnh.".to_string()], 15);
        assert!(said.contains("{\"keep\": [numbers]}"), "{said}");
        assert!(said.contains("0\t2026-01-01\tHôm nay trời lạnh."), "{said}");
        assert!(said.contains("Do not write any sentences of your own"), "{said}");
        assert!(said.contains("Choose the 15"), "{said}");
    }

    #[test]
    fn a_reply_that_is_not_the_json_asked_for_is_read_as_nothing() {
        assert_eq!(parse_reply("{\"keep\": [2, 0]}"), Some(vec![2, 0]));
        // Wrapped in the chatter a model cannot help adding.
        assert_eq!(parse_reply("Sure! {\"keep\":[1]} Hope that helps"), Some(vec![1]));
        assert_eq!(parse_reply("I picked the third and the first."), None);
        assert_eq!(parse_reply(""), None);
    }
}
