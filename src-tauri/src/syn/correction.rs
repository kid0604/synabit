//! Noticing that the user just told Syn it was wrong.
//!
//! # Why this is arithmetic and not a question for the model
//!
//! Reflection already mentions corrections: its prompt says *if the user
//! corrected the assistant here … set `from_correction`*. That asks the model
//! to **notice**, and `docs/adr-rag-vs-agentic-2026-09-03.md` measured what
//! asking is worth on a local model — an instruction written for one specific
//! failure changed nothing at all and was reverted.
//!
//! The same reasoning `skill::repeated_chain` is built on applies here: a
//! deterministic detector is free, runs without a network, is testable, and
//! cannot hallucinate a correction that did not happen. When it fires, the
//! reflection prompt stops asking and starts *telling* — which is a different
//! and much stronger instruction.
//!
//! # Precision over recall, deliberately
//!
//! This will miss corrections. That is the chosen direction: a missed one costs
//! a memory proposal that would probably have been declined anyway, and a false
//! one tells the model that the user disagreed with something when they did
//! not — which is how Syn ends up remembering a disagreement nobody had.
//!
//! So the markers are the blunt openers people actually use to contradict
//! somebody, and they only count near the start of the message, which is where
//! a contradiction lives. *"Không phải lo"* in the middle of a paragraph is not
//! somebody saying Syn was wrong.

/// How far into the message a marker still counts as an opening.
///
/// Sixty characters — a couple of short clauses. A contradiction is the first
/// thing somebody says, and past that the same words are ordinary sentence.
const OPENING_CHARS: usize = 60;

/// The openings people use to say "that is wrong".
///
/// Both languages, because the app is bilingual and a signal that only works in
/// English is one that does not work for the person who wrote the vault this
/// was built against. Each is a phrase rather than a word: `sai` alone appears
/// in *"kiểm tra xem có sai không"*, which is a question and not a correction.
const MARKERS: &[&str] = &[
    // Vietnamese
    "không phải",
    "ko phải",
    "hông phải",
    "sai rồi",
    "sai r",
    "nhầm rồi",
    "bị nhầm",
    "đâu có",
    "ý tao là",
    "ý mình là",
    "ý tôi là",
    "tao đâu có",
    "không đúng",
    "chưa đúng",
    // English
    "no, ",
    "nope",
    "that's wrong",
    "thats wrong",
    "that is wrong",
    "not what i",
    "i meant",
    "i said",
    "incorrect",
    "not quite",
    "wrong,",
];

/// Lowercased, with the punctuation that separates a clause kept.
///
/// Kept rather than stripped because `"no, "` needs its comma: a bare `no`
/// matches *"nothing"*, *"note"* and *"no idea"*, and the comma is what makes
/// it an opener.
fn opening(message: &str) -> String {
    message
        .trim_start()
        .chars()
        .take(OPENING_CHARS)
        .collect::<String>()
        .to_lowercase()
}

/// Whether this message reads as the user correcting the assistant.
///
/// `had_a_reply` is not optional politeness: a correction needs something to
/// correct. The first message of a conversation cannot be one, whatever it
/// says, and counting it would mark somebody's opening *"không phải kiểu đó"*
/// about their own notes as a disagreement with Syn.
pub fn looks_like_one(message: &str, had_a_reply: bool) -> bool {
    if !had_a_reply {
        return false;
    }
    let opening = opening(message);
    MARKERS.iter().any(|marker| opening.contains(marker))
}

/// The line the reflection prompt carries when a correction was detected.
///
/// It states the fact rather than asking about it, and says what to do with it.
/// Empty when nothing was detected, so a prompt with no correction reads
/// exactly as it did before this existed.
pub fn note(detected: bool) -> &'static str {
    if !detected {
        return "";
    }
    "This exchange contains a correction: the user told the assistant that \
     something it assumed or did was wrong. That is the strongest evidence \
     this app ever gets about how they want to be helped. Look first at what \
     the correction implies as a standing instruction — not at the detail \
     being corrected, which belongs in the conversation and not in memory — \
     and set \"from_correction\": true on anything you propose from it.\n\n"
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The sentence the ADR names as the highest-signal moment the app gets.
    #[test]
    fn the_openings_people_actually_use() {
        for said in [
            "không phải, tao muốn cái kia",
            "Không phải vậy đâu",
            "ko phải cái đó",
            "sai rồi, tao nói là tuần trước",
            "nhầm rồi má",
            "ý tao là cái note tháng 8",
            "no, I meant the other one",
            "Nope, that's the wrong file",
            "not quite — try again with the tasks",
            "that's wrong",
        ] {
            assert!(looks_like_one(said, true), "missed: {said}");
        }
    }

    /// A correction needs something to correct. Counting the first message
    /// would mark somebody's opening complaint about their own notes as a
    /// disagreement with Syn.
    #[test]
    fn nothing_can_be_corrected_before_there_is_a_reply() {
        assert!(!looks_like_one("không phải, tao muốn cái kia", false));
        assert!(!looks_like_one("no, I meant the other one", false));
    }

    /// The direction this errs in, stated as cases. A false positive tells the
    /// model the user disagreed when they did not.
    #[test]
    fn ordinary_sentences_are_not_corrections() {
        for said in [
            "kiểm tra xem có sai không",
            "tao không có note nào về vụ này",
            "note this down for me",
            "nothing urgent today",
            "no idea what that means",
            "tìm giúp tao mấy task chưa xong",
            "summarise the meeting notes",
            "",
            "   ",
        ] {
            assert!(!looks_like_one(said, true), "false positive: {said}");
        }
    }

    /// The words only count as an opening. Deep in a paragraph they are
    /// ordinary language — *"không phải lo"* is reassurance, not a correction.
    #[test]
    fn the_same_words_later_in_a_message_are_just_words() {
        let long = format!(
            "{} không phải chuyện lớn",
            "tóm tắt giúp tao cuộc họp sáng nay với team hạ tầng về vụ alert, ".repeat(2)
        );
        assert!(!looks_like_one(&long, true), "matched deep in the message");
    }

    #[test]
    fn leading_space_does_not_hide_an_opening() {
        assert!(looks_like_one("   không phải vậy", true));
    }

    /// The comma is what makes `no` an opener rather than a syllable.
    #[test]
    fn a_bare_no_inside_a_word_does_not_match() {
        for said in ["nothing to do today", "note the number", "no idea"] {
            assert!(!looks_like_one(said, true), "false positive: {said}");
        }
    }

    /// The point of the module, as one case.
    ///
    /// Reflection's prompt already mentioned corrections — it *asked* the model
    /// to notice one. This asserts the difference: once Rust has decided, the
    /// prompt states it.
    #[test]
    fn a_detected_correction_is_asserted_rather_than_asked_about() {
        let plain = crate::syn::reflect::prompt_for_test("chào", "chào bạn", false);
        let told =
            crate::syn::reflect::prompt_for_test("không phải, tao nói tuần trước", "ok", true);

        assert!(!plain.contains("This exchange contains a correction"));
        assert!(told.contains("This exchange contains a correction"), "{told}");

        // And nothing else moved, so a turn with no correction reads exactly as
        // it did before this module existed.
        assert!(plain.contains("Two things change the answer"));
        assert!(told.contains("Two things change the answer"));
    }

    /// A prompt with no correction has to read exactly as it did before this
    /// module existed, or every prompt snapshot changes for nothing.
    #[test]
    fn no_correction_adds_no_text() {
        assert_eq!(note(false), "");
        assert!(!note(true).is_empty());
    }

    /// It tells rather than asks, which is the whole difference from what the
    /// reflection prompt already said.
    #[test]
    fn the_note_states_the_fact_rather_than_asking_about_it() {
        let note = note(true);
        assert!(note.contains("This exchange contains a correction"), "{note}");
        assert!(note.contains("from_correction"), "{note}");
        assert!(
            note.contains("standing instruction"),
            "it points at what to keep, not the detail:\n{note}"
        );
    }
}
