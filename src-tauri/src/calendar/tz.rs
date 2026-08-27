//! Turning a wall clock in one place into the wall clock in another.
//!
//! # What is stored, and why
//!
//! An event keeps the time you would read off a clock on the wall where it
//! happens — `2026-03-10T09:00` — together with the zone that wall is in.
//! Not an instant in UTC. That is what iCalendar does, and it is the only
//! choice that survives daylight saving: a nine o'clock stand-up is at nine
//! o'clock in March and at nine o'clock in November, and storing the instant
//! would move it by an hour halfway through the year.
//!
//! It also means nothing has to be migrated. An event with no zone is a
//! floating one — nine o'clock wherever you happen to be — which is exactly
//! what every event in every existing vault already is.
//!
//! # The two awkward hours of the year
//!
//! Converting a wall clock into an instant has two answers that are not one
//! answer:
//!
//! * When the clocks go back, an hour happens twice. `Ambiguous` — this takes
//!   the first, which is the earlier real moment and what a calendar showing
//!   "01:30" on that morning means to almost everyone.
//! * When the clocks go forward, an hour never happens at all. A meeting
//!   saved for 02:30 on that day has no moment to be at. Rather than dropping
//!   it — a disappearing appointment is the worst possible answer — it moves
//!   to the first moment that does exist.
//!
//! Both are decisions, not accidents, and the tests below name them.

use chrono::{LocalResult, NaiveDateTime, TimeZone};
use chrono_tz::Tz;

/// A zone name, if it is one this build knows.
pub fn zone(name: &str) -> Option<Tz> {
    let name = name.trim();
    if name.is_empty() {
        return None;
    }
    name.parse::<Tz>().ok()
}

/// Is this a zone name we can actually convert with?
pub fn is_known_zone(name: &str) -> bool {
    zone(name).is_some()
}

/// The moment a wall clock reading refers to, in `tz`.
///
/// See the note above for what happens on the two days a year when a wall
/// clock reading is not a single moment.
fn instant_in(tz: Tz, naive: NaiveDateTime) -> chrono::DateTime<Tz> {
    match tz.from_local_datetime(&naive) {
        LocalResult::Single(dt) => dt,
        // The clocks went back: this reading happened twice. Take the first.
        LocalResult::Ambiguous(first, _second) => first,
        // The clocks went forward: this reading never happened. Walk to the
        // first minute that did, rather than losing the event.
        LocalResult::None => {
            let mut probe = naive;
            for _ in 0..(24 * 60) {
                probe += chrono::Duration::minutes(1);
                if let LocalResult::Single(dt) = tz.from_local_datetime(&probe) {
                    return dt;
                }
                if let LocalResult::Ambiguous(dt, _) = tz.from_local_datetime(&probe) {
                    return dt;
                }
            }
            // Unreachable for any real zone; better than a panic if one exists.
            tz.from_utc_datetime(&naive)
        }
    }
}

/// The same moment, read off a clock in `to`.
///
/// `None` when either zone is unknown, so a caller can leave the time alone
/// rather than move it somewhere invented.
pub fn convert_wall_clock(naive: NaiveDateTime, from: &str, to: &str) -> Option<NaiveDateTime> {
    let (from_tz, to_tz) = (zone(from)?, zone(to)?);
    if from_tz == to_tz {
        return Some(naive);
    }
    Some(instant_in(from_tz, naive).with_timezone(&to_tz).naive_local())
}

/// `YYYY-MM-DDTHH:MM` in, `YYYY-MM-DDTHH:MM` out.
///
/// A bare date is returned untouched: an all-day event is the same day
/// everywhere, and giving it a clock so it could be converted is how a public
/// holiday ends up on the wrong day for anyone east or west of the author.
pub fn convert_stamp(stamp: &str, from: &str, to: &str) -> Option<String> {
    if !stamp.contains('T') {
        return Some(stamp.to_string());
    }
    let naive = parse_stamp(stamp)?;
    let moved = convert_wall_clock(naive, from, to)?;
    Some(moved.format("%Y-%m-%dT%H:%M").to_string())
}

/// Accepts the shapes this app writes: with seconds and without.
pub fn parse_stamp(stamp: &str) -> Option<NaiveDateTime> {
    NaiveDateTime::parse_from_str(stamp, "%Y-%m-%dT%H:%M:%S")
        .or_else(|_| NaiveDateTime::parse_from_str(stamp, "%Y-%m-%dT%H:%M"))
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn n(s: &str) -> NaiveDateTime {
        parse_stamp(s).unwrap()
    }

    #[test]
    fn a_call_at_nine_in_tokyo_is_seven_the_evening_before_in_new_york() {
        assert_eq!(
            convert_stamp("2026-03-10T09:00", "Asia/Tokyo", "America/New_York").as_deref(),
            Some("2026-03-09T20:00"),
        );
    }

    #[test]
    fn hanoi_and_bangkok_share_a_clock_and_nothing_moves() {
        assert_eq!(
            convert_stamp("2026-03-10T09:00", "Asia/Ho_Chi_Minh", "Asia/Bangkok").as_deref(),
            Some("2026-03-10T09:00"),
        );
    }

    #[test]
    fn the_same_zone_is_not_converted_at_all() {
        assert_eq!(
            convert_stamp("2026-03-10T09:00", "Asia/Tokyo", "Asia/Tokyo").as_deref(),
            Some("2026-03-10T09:00"),
        );
    }

    /// An all-day event is the same day everywhere. Converting it is how a
    /// public holiday lands on the wrong date for half the world.
    #[test]
    fn a_bare_date_is_never_moved() {
        assert_eq!(
            convert_stamp("2026-03-10", "Asia/Tokyo", "America/New_York").as_deref(),
            Some("2026-03-10"),
        );
    }

    #[test]
    fn a_zone_this_build_does_not_know_leaves_the_time_alone() {
        assert!(convert_stamp("2026-03-10T09:00", "Mars/Olympus", "UTC").is_none());
        assert!(convert_stamp("2026-03-10T09:00", "", "UTC").is_none());
        assert!(convert_stamp("2026-03-10T09:00", "UTC", "not a zone").is_none());
    }

    /// The stored wall clock does not move across a daylight saving change,
    /// which is the whole reason it is stored as a wall clock. What moves is
    /// how far it is from somewhere that did not change.
    #[test]
    fn a_nine_oclock_meeting_stays_at_nine_across_a_clock_change() {
        // London goes forward on 29 March 2026; Tokyo never changes.
        let winter = convert_stamp("2026-03-01T09:00", "Europe/London", "Asia/Tokyo").unwrap();
        let summer = convert_stamp("2026-04-01T09:00", "Europe/London", "Asia/Tokyo").unwrap();
        assert_eq!(winter, "2026-03-01T18:00", "GMT is nine hours behind Tokyo");
        assert_eq!(summer, "2026-04-01T17:00", "BST is eight");
    }

    /// 02:30 on the morning the clocks go forward does not exist in London.
    /// An event saved for it has to land somewhere; vanishing is not an option.
    #[test]
    fn a_time_that_never_happened_moves_forward_instead_of_disappearing() {
        let moved = convert_wall_clock(n("2026-03-29T01:30"), "Europe/London", "UTC");
        assert!(moved.is_some(), "the event must survive the gap");
        // 01:00 GMT is the moment the clocks jump to 02:00 BST.
        assert_eq!(moved.unwrap().format("%Y-%m-%dT%H:%M").to_string(), "2026-03-29T01:00");
    }

    /// 01:30 on the morning the clocks go back happens twice in London. The
    /// earlier one is what a calendar showing "01:30" means to most people.
    #[test]
    fn an_hour_that_happened_twice_resolves_to_the_first_of_them() {
        let moved = convert_wall_clock(n("2026-10-25T01:30"), "Europe/London", "UTC").unwrap();
        assert_eq!(moved.format("%Y-%m-%dT%H:%M").to_string(), "2026-10-25T00:30");
    }

    /// The one that moves an event to a different day, which is what makes
    /// this more than an hours-and-minutes problem.
    #[test]
    fn a_late_evening_event_can_belong_to_another_day_somewhere_else() {
        assert_eq!(
            convert_stamp("2026-03-10T23:00", "Asia/Tokyo", "America/Los_Angeles").as_deref(),
            Some("2026-03-10T07:00"),
        );
        assert_eq!(
            convert_stamp("2026-03-10T07:00", "America/Los_Angeles", "Asia/Tokyo").as_deref(),
            Some("2026-03-10T23:00"),
        );
        // And across midnight in the other direction.
        assert_eq!(
            convert_stamp("2026-03-10T22:00", "America/Los_Angeles", "Asia/Tokyo").as_deref(),
            Some("2026-03-11T14:00"),
        );
    }

    #[test]
    fn zone_names_are_recognised_or_refused_rather_than_guessed() {
        assert!(is_known_zone("Asia/Ho_Chi_Minh"));
        assert!(is_known_zone("UTC"));
        assert!(!is_known_zone("Asia/Saigon2"));
        assert!(!is_known_zone(""));
        assert!(!is_known_zone("   "));
    }
}
