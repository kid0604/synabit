//! How big something was, from what the event itself says.
//!
//! The timeline has no lanes. "Work", "life", "admin" are not what separates a
//! wedding from a lunch — size is (`docs/su-kien-2026-09-16.md` §2). So every
//! event carries one number, computed when the index is built, and a view that
//! wants only the large ones is a `WHERE`, not a filter over types.
//!
//! # What it is allowed to read
//!
//! Only the core: how long it lasted, how many took part, how much evidence
//! there is, how much was written, and whether a person wrote it down or the
//! app worked it out. It may **not** read the free-form part of an event
//! (§5). That is why the signals arrive here as a small struct of their own
//! rather than as an event: a field like `severity` cannot reach this function
//! without somebody first making it a core field, which is rule 4 of §3.3.
//!
//! # The weights are a first guess
//!
//! Open question 1 of the doc: there is nothing yet to ground them in. They are
//! written here so they can be measured against a real vault and changed. The
//! test below fixes the **order** they must produce on a hand-built set, not
//! the numbers, so tuning does not mean rewriting the tests.

use chrono::NaiveDate;

/// What an event says about its own size.
#[derive(Debug, Clone, Copy, Default)]
pub struct Signals<'a> {
    /// ISO, as stored.
    pub from: &'a str,
    pub to: &'a str,
    /// Nodes in the `with` role: people, systems, whoever was there.
    pub people: usize,
    /// Nodes in the `evidence` role: photographs, recordings, the note.
    pub evidence: usize,
    /// The title, and the label under it.
    pub text: &'a str,
    /// `user`, `extract` or `derived`.
    pub source: &'a str,
}

/// Nothing lasts longer than this for the purpose of size.
///
/// A job still held runs to `when::open_end()`, the year 9999. Left alone it
/// would be the largest thing that ever happened to anybody, for ever. Ten
/// years is the point past which longer stops meaning bigger.
const LONGEST: f64 = 3650.0;

/// Weights, in one place, so changing the guess is one edit.
const SPAN: f64 = 0.9;
const PEOPLE: f64 = 1.2;
const EVIDENCE: f64 = 0.6;
const WORDS: f64 = 0.5;

pub fn of(signals: Signals) -> f64 {
    let days = days_between(signals.from, signals.to).min(LONGEST);
    let words = signals.text.split_whitespace().count() as f64;

    // Every term saturates: the fiftieth photograph of a trip says much less
    // than the second, and no single signal can swamp the rest.
    1.0 + SPAN * days.ln_1p()
        + PEOPLE * (signals.people as f64).ln_1p()
        + EVIDENCE * (signals.evidence as f64).ln_1p()
        + WORDS * words.ln_1p()
        + written_by_hand(signals.source)
}

/// A person bothering to write it down is itself a claim that it mattered.
fn written_by_hand(source: &str) -> f64 {
    match source {
        "user" => 1.0,
        "extract" => 0.3,
        _ => 0.0,
    }
}

fn days_between(from: &str, to: &str) -> f64 {
    let day = |text: &str| NaiveDate::parse_from_str(text, "%Y-%m-%d").ok();
    match (day(from), day(to)) {
        (Some(from), Some(to)) => (to - from).num_days().max(0) as f64,
        _ => 0.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The prediction, written down before it is measured on a real vault.
    ///
    /// These are the user's own examples. What the test fixes is their order:
    /// a three-day trip with four people and a pile of photographs is the
    /// largest thing here, and a picture nobody is in is the smallest.
    #[test]
    fn a_trip_outweighs_a_wedding_outweighs_a_meeting_outweighs_lunch() {
        let trip = of(Signals {
            from: "2019-08-02",
            to: "2019-08-04",
            people: 4,
            evidence: 12,
            text: "Đi chơi 3 ngày ở Tuần Châu",
            source: "user",
        });
        let wedding = of(Signals {
            from: "2016-05-14",
            to: "2016-05-14",
            people: 3,
            evidence: 0,
            text: "Đám cưới Tuấn và Thuỳ",
            source: "user",
        });
        let meeting = of(Signals {
            from: "2026-06-18",
            to: "2026-06-18",
            people: 2,
            evidence: 0,
            text: "Họp với Khánh và Hải",
            source: "derived",
        });
        let lunch = of(Signals {
            from: "2026-06-18",
            to: "2026-06-18",
            text: "Ăn trưa 200k",
            source: "derived",
            ..Signals::default()
        });
        let picture = of(Signals {
            from: "2026-06-18",
            to: "2026-06-18",
            evidence: 1,
            source: "derived",
            ..Signals::default()
        });

        let order = [trip, wedding, meeting, lunch, picture];
        for pair in order.windows(2) {
            assert!(pair[0] > pair[1], "{order:?} is not in descending order");
        }
    }

    #[test]
    fn a_job_still_held_is_not_the_largest_thing_that_ever_happened() {
        let ongoing = of(Signals {
            from: "2021-03-01",
            to: "9999-12-31",
            text: "MDP",
            source: "user",
            ..Signals::default()
        });
        let decade = of(Signals {
            from: "2011-03-01",
            to: "2021-03-01",
            text: "MDP",
            source: "user",
            ..Signals::default()
        });
        assert_eq!(ongoing, decade, "an open end is ten years, not eight thousand");
    }

    #[test]
    fn an_unreadable_or_backwards_date_costs_nothing_and_crashes_nothing() {
        let broken = of(Signals { from: "sometime", to: "2016-05-14", text: "x", source: "user" , ..Signals::default() });
        let backwards = of(Signals { from: "2016-05-14", to: "2016-05-01", text: "x", source: "user", ..Signals::default() });
        let point = of(Signals { from: "2016-05-14", to: "2016-05-14", text: "x", source: "user", ..Signals::default() });
        assert_eq!(broken, point);
        assert_eq!(backwards, point);
    }
}
