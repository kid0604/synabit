use chrono::{Datelike, NaiveDate, Weekday};

/// How often a series repeats.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Freq {
    Daily,
    Weekly,
    Monthly,
    Yearly,
}

impl Freq {
    fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_uppercase().as_str() {
            "DAILY" => Some(Freq::Daily),
            "WEEKLY" => Some(Freq::Weekly),
            "MONTHLY" => Some(Freq::Monthly),
            "YEARLY" => Some(Freq::Yearly),
            _ => None,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Freq::Daily => "DAILY",
            Freq::Weekly => "WEEKLY",
            Freq::Monthly => "MONTHLY",
            Freq::Yearly => "YEARLY",
        }
    }
}

fn weekday_code(day: Weekday) -> &'static str {
    match day {
        Weekday::Mon => "MO",
        Weekday::Tue => "TU",
        Weekday::Wed => "WE",
        Weekday::Thu => "TH",
        Weekday::Fri => "FR",
        Weekday::Sat => "SA",
        Weekday::Sun => "SU",
    }
}

fn parse_weekday(code: &str) -> Option<Weekday> {
    match code.trim().to_ascii_uppercase().as_str() {
        "MO" => Some(Weekday::Mon),
        "TU" => Some(Weekday::Tue),
        "WE" => Some(Weekday::Wed),
        "TH" => Some(Weekday::Thu),
        "FR" => Some(Weekday::Fri),
        "SA" => Some(Weekday::Sat),
        "SU" => Some(Weekday::Sun),
        _ => None,
    }
}

/// Days from Monday, which is where RFC 5545 starts a week unless told
/// otherwise. Deliberately not the locale's first day: that decides how the
/// grid is drawn, not what "every other week" means.
pub fn monday_offset(day: Weekday) -> i64 {
    day.num_days_from_monday() as i64
}

/// The Monday on or before `date`.
pub fn week_start(date: NaiveDate) -> NaiveDate {
    date - chrono::Duration::days(monday_offset(date.weekday()))
}

/// A recurrence rule — the subset of RFC 5545 this app understands.
///
/// `BYDAY` is honoured for weekly rules and ignored elsewhere, which covers
/// the pattern people actually ask for ("every Monday, Wednesday and Friday")
/// without pretending to support `BYSETPOS` or `BYMONTHDAY`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RRule {
    pub freq: Freq,
    pub interval: u32,
    pub by_day: Vec<Weekday>,
    pub count: Option<u32>,
    pub until: Option<NaiveDate>,
}

impl RRule {
    pub fn new(freq: Freq) -> Self {
        Self { freq, interval: 1, by_day: Vec::new(), count: None, until: None }
    }

    /// Parse `FREQ=WEEKLY;INTERVAL=2;BYDAY=MO,WE;UNTIL=20261231`.
    ///
    /// Anything unrecognised is skipped rather than refused: a rule written by
    /// another tool should still repeat weekly here even if it also carries a
    /// `BYSETPOS` this app has no idea about. `FREQ` is the one part that must
    /// be there and must make sense.
    pub fn parse(s: &str) -> Option<Self> {
        let s = s.trim();
        if s.is_empty() {
            return None;
        }
        let mut freq = None;
        let mut rule = Self::new(Freq::Daily);

        for part in s.split(';') {
            let Some((key, value)) = part.split_once('=') else { continue };
            match key.trim().to_ascii_uppercase().as_str() {
                "FREQ" => freq = Freq::parse(value.trim()),
                "INTERVAL" => {
                    if let Ok(n) = value.trim().parse::<u32>() {
                        if n >= 1 {
                            rule.interval = n;
                        }
                    }
                }
                "BYDAY" => {
                    rule.by_day = value.split(',').filter_map(parse_weekday).collect();
                }
                "COUNT" => {
                    if let Ok(n) = value.trim().parse::<u32>() {
                        if n >= 1 {
                            rule.count = Some(n);
                        }
                    }
                }
                "UNTIL" => rule.until = parse_until(value.trim()),
                _ => {}
            }
        }

        rule.freq = freq?;
        if rule.freq != Freq::Weekly {
            rule.by_day.clear();
        }
        rule.by_day.sort_by_key(|d| monday_offset(*d));
        rule.by_day.dedup();
        Some(rule)
    }

    /// Parse a rule from somewhere else, refusing anything that would land on
    /// different days than this app can produce.
    ///
    /// [`parse`] is deliberately forgiving, which is right for a rule already
    /// in this vault: this app wrote it, so an unfamiliar part is noise. It is
    /// wrong for a file from another calendar. `FREQ=MONTHLY;BYDAY=-1FR` means
    /// the last Friday of the month; dropping the `-1FR` and keeping the rest
    /// leaves "the 26th of every month", which is a different series wearing
    /// the same name — imported silently, wrong forever.
    ///
    /// So an import either understands the whole rule or does not claim to.
    /// The event still arrives; it arrives without a repeat, which is visible,
    /// rather than with the wrong one, which is not.
    pub fn parse_foreign(s: &str) -> Option<Self> {
        let rule = Self::parse(s)?;

        for part in s.split(';') {
            let Some((key, value)) = part.split_once('=') else { continue };
            let key = key.trim().to_ascii_uppercase();
            let value = value.trim();
            match key.as_str() {
                "FREQ" | "INTERVAL" | "COUNT" | "UNTIL" => {}
                // Only plain weekdays on a weekly rule. An ordinal — `2MO`,
                // `-1FR` — picks one week of the month, which is a rule about
                // months, not weeks.
                "BYDAY" if rule.freq == Freq::Weekly => {
                    if value.split(',').any(|d| parse_weekday(d).is_none()) {
                        return None;
                    }
                }
                // The default, and the only one the week arithmetic here uses.
                "WKST" if value.eq_ignore_ascii_case("MO") => {}
                _ => return None,
            }
        }
        Some(rule)
    }

    /// The shapes older vaults stored: an enum plus a separate end date.
    pub fn from_legacy(recurrence: &str, recurrence_end_at: &str) -> Option<Self> {
        let freq = match recurrence.trim().to_ascii_lowercase().as_str() {
            "daily" => Freq::Daily,
            "weekly" => Freq::Weekly,
            "monthly" => Freq::Monthly,
            "yearly" => Freq::Yearly,
            _ => return None,
        };
        let mut rule = Self::new(freq);
        rule.until = NaiveDate::parse_from_str(recurrence_end_at.trim(), "%Y-%m-%d").ok();
        Some(rule)
    }

    pub fn to_rrule_string(&self) -> String {
        let mut out = format!("FREQ={}", self.freq.as_str());
        if self.interval > 1 {
            out.push_str(&format!(";INTERVAL={}", self.interval));
        }
        if !self.by_day.is_empty() {
            let codes: Vec<&str> = self.by_day.iter().map(|d| weekday_code(*d)).collect();
            out.push_str(&format!(";BYDAY={}", codes.join(",")));
        }
        if let Some(n) = self.count {
            out.push_str(&format!(";COUNT={}", n));
        }
        if let Some(until) = self.until {
            out.push_str(&format!(";UNTIL={}", until.format("%Y%m%d")));
        }
        out
    }
}

fn parse_until(value: &str) -> Option<NaiveDate> {
    // `20261231`, `20261231T235959Z`, and the `2026-12-31` this app used to
    // keep in its own field are all accepted.
    let date_part = value.split('T').next().unwrap_or(value);
    if date_part.contains('-') {
        return NaiveDate::parse_from_str(date_part, "%Y-%m-%d").ok();
    }
    NaiveDate::parse_from_str(date_part, "%Y%m%d").ok()
}

/// The last day on or before `target` that `rule` lands on, given `start`.
///
/// Returned without regard to how long an instance lasts: the caller decides
/// whether `target` is still inside the instance that began here.
pub fn anchor_on_or_before(rule: &RRule, start: NaiveDate, target: NaiveDate) -> Option<NaiveDate> {
    if target < start {
        return None;
    }
    let k = rule.interval.max(1) as i64;

    match rule.freq {
        Freq::Daily => {
            let days = (target - start).num_days();
            Some(start + chrono::Duration::days((days / k) * k))
        }
        Freq::Weekly if rule.by_day.is_empty() => {
            let weeks = (target - start).num_days().div_euclid(7);
            Some(start + chrono::Duration::days((weeks / k) * k * 7))
        }
        Freq::Weekly => {
            let w0 = week_start(start);
            let wt = week_start(target);
            let weeks_between = (wt - w0).num_days() / 7;
            let mut active = (weeks_between / k) * k;
            loop {
                let week = w0 + chrono::Duration::days(active * 7);
                // The days this rule lands on in that week, never before the
                // series itself began and never after the day being asked about.
                let best = rule
                    .by_day
                    .iter()
                    .map(|d| week + chrono::Duration::days(monday_offset(*d)))
                    .filter(|d| *d >= start && *d <= target)
                    .max();
                if let Some(found) = best {
                    return Some(found);
                }
                if active < k {
                    return None;
                }
                active -= k;
            }
        }
        Freq::Monthly => {
            let mut months = (target.year() - start.year()) as i64 * 12
                + (target.month() as i64 - start.month() as i64);
            months = (months.max(0) / k) * k;
            loop {
                let anchor = month_anchor(start, months)?;
                if anchor <= target {
                    return Some(anchor);
                }
                if months < k {
                    return None;
                }
                months -= k;
            }
        }
        Freq::Yearly => {
            let mut years = (target.year() - start.year()) as i64;
            years = (years.max(0) / k) * k;
            loop {
                let anchor = year_anchor(start, years)?;
                if anchor <= target {
                    return Some(anchor);
                }
                if years < k {
                    return None;
                }
                years -= k;
            }
        }
    }
}

/// The last day of a month, used to clamp a series that starts on a day the
/// target month does not have.
fn last_day_of_month(year: i32, month: u32) -> Option<NaiveDate> {
    let (ny, nm) = if month == 12 { (year + 1, 1) } else { (year, month + 1) };
    NaiveDate::from_ymd_opt(ny, nm, 1).and_then(|d| d.pred_opt())
}

fn month_anchor(start: NaiveDate, months_ahead: i64) -> Option<NaiveDate> {
    let total = start.month() as i64 - 1 + months_ahead;
    let year = start.year() + (total.div_euclid(12)) as i32;
    let month = (total.rem_euclid(12) + 1) as u32;
    NaiveDate::from_ymd_opt(year, month, start.day())
        .or_else(|| last_day_of_month(year, month))
}

fn year_anchor(start: NaiveDate, years_ahead: i64) -> Option<NaiveDate> {
    let year = start.year() + years_ahead as i32;
    NaiveDate::from_ymd_opt(year, start.month(), start.day()).or_else(|| {
        // A 29 February date lands on the 28th in a common year.
        if start.month() == 2 && start.day() == 29 {
            NaiveDate::from_ymd_opt(year, 2, 28)
        } else {
            None
        }
    })
}

/// Which occurrence `anchor` is, counting the first as 1.
///
/// Only meaningful for an anchor the rule actually produces. It exists for
/// `COUNT`, which asks how many have happened rather than how long it has been
/// going — and answering that by walking the series from the start would make
/// a year view walk it 365 times.
pub fn ordinal_of(rule: &RRule, start: NaiveDate, anchor: NaiveDate) -> u32 {
    let k = rule.interval.max(1) as i64;
    let n = match rule.freq {
        Freq::Daily => (anchor - start).num_days() / k,
        Freq::Weekly if rule.by_day.is_empty() => (anchor - start).num_days() / 7 / k,
        Freq::Weekly => {
            let w0 = week_start(start);
            let wa = week_start(anchor);
            let active = ((wa - w0).num_days() / 7) / k;
            let per_week = rule.by_day.len() as i64;
            let index = rule
                .by_day
                .iter()
                .position(|d| *d == anchor.weekday())
                .unwrap_or(0) as i64;
            if active == 0 {
                // The first week is short: nothing before the series started.
                let start_offset = monday_offset(start.weekday());
                let skipped = rule
                    .by_day
                    .iter()
                    .filter(|d| monday_offset(**d) < start_offset)
                    .count() as i64;
                index - skipped
            } else {
                let start_offset = monday_offset(start.weekday());
                let in_first = rule
                    .by_day
                    .iter()
                    .filter(|d| monday_offset(**d) >= start_offset)
                    .count() as i64;
                in_first + (active - 1) * per_week + index
            }
        }
        Freq::Monthly => {
            ((anchor.year() - start.year()) as i64 * 12
                + (anchor.month() as i64 - start.month() as i64))
                / k
        }
        Freq::Yearly => (anchor.year() - start.year()) as i64 / k,
    };
    (n.max(0) + 1) as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    #[test]
    fn a_plain_rule_parses_and_comes_back_the_same() {
        for text in [
            "FREQ=DAILY",
            "FREQ=WEEKLY;INTERVAL=2",
            "FREQ=WEEKLY;BYDAY=MO,WE,FR",
            "FREQ=WEEKLY;INTERVAL=2;BYDAY=TU;COUNT=10",
            "FREQ=MONTHLY;INTERVAL=3",
            "FREQ=YEARLY;UNTIL=20301231",
        ] {
            let rule = RRule::parse(text).unwrap_or_else(|| panic!("{} should parse", text));
            assert_eq!(rule.to_rrule_string(), text, "round trip of {}", text);
        }
    }

    #[test]
    fn byday_is_sorted_and_deduplicated_so_the_string_is_stable() {
        let rule = RRule::parse("FREQ=WEEKLY;BYDAY=FR,MO,FR,WE").unwrap();
        assert_eq!(rule.to_rrule_string(), "FREQ=WEEKLY;BYDAY=MO,WE,FR");
    }

    /// A rule from another tool should still repeat weekly here even if it
    /// also carries parts this app has never heard of.
    #[test]
    fn unknown_parts_are_skipped_rather_than_refusing_the_rule() {
        let rule = RRule::parse("FREQ=WEEKLY;BYSETPOS=-1;WKST=SU;X-THING=1").unwrap();
        assert_eq!(rule.freq, Freq::Weekly);
        assert_eq!(rule.interval, 1);
    }

    #[test]
    fn a_rule_with_no_usable_freq_is_no_rule_at_all() {
        assert!(RRule::parse("").is_none());
        assert!(RRule::parse("INTERVAL=2").is_none());
        assert!(RRule::parse("FREQ=FORTNIGHTLY").is_none());
        assert!(RRule::parse("nonsense").is_none());
    }

    #[test]
    fn nonsense_numbers_fall_back_rather_than_breaking_the_rule() {
        let rule = RRule::parse("FREQ=DAILY;INTERVAL=0;COUNT=0").unwrap();
        assert_eq!(rule.interval, 1, "an interval of zero would never advance");
        assert_eq!(rule.count, None, "a count of zero would hide the event entirely");
    }

    /// `BYDAY` on a monthly rule would mean `BYSETPOS` territory — "the first
    /// Monday of the month" — which this app does not do. Keeping the days
    /// would silently turn it into something else.
    #[test]
    fn byday_is_dropped_for_frequencies_that_do_not_use_it() {
        let rule = RRule::parse("FREQ=MONTHLY;BYDAY=MO").unwrap();
        assert!(rule.by_day.is_empty());
    }

    #[test]
    fn until_is_read_in_every_shape_it_gets_written() {
        for text in ["UNTIL=20261231", "UNTIL=20261231T235959Z", "UNTIL=2026-12-31"] {
            let rule = RRule::parse(&format!("FREQ=DAILY;{}", text)).unwrap();
            assert_eq!(rule.until, Some(d("2026-12-31")), "{}", text);
        }
    }

    /// The lenient reading is for rules this app wrote, where an unfamiliar
    /// part is noise. A file from another calendar gets the strict one, or
    /// its series quietly becomes a different series.
    #[test]
    fn a_foreign_rule_is_refused_when_it_would_land_on_other_days() {
        for refused in [
            "FREQ=MONTHLY;BYDAY=-1FR",   // the last Friday of the month
            "FREQ=MONTHLY;BYDAY=2MO",    // the second Monday
            "FREQ=WEEKLY;BYSETPOS=-1",
            "FREQ=MONTHLY;BYMONTHDAY=1,15",
            "FREQ=YEARLY;BYMONTH=3;BYDAY=SU",
            "FREQ=WEEKLY;INTERVAL=2;WKST=SU",
            "FREQ=DAILY;BYHOUR=9,17",
        ] {
            assert!(RRule::parse_foreign(refused).is_none(), "{} should be refused", refused);
            // The lenient reading still takes them, which is what makes the
            // strict one necessary rather than redundant.
            assert!(RRule::parse(refused).is_some(), "{} should still parse leniently", refused);
        }
    }

    #[test]
    fn a_foreign_rule_this_app_can_reproduce_is_taken_as_it_is() {
        for accepted in [
            "FREQ=DAILY",
            "FREQ=DAILY;INTERVAL=3",
            "FREQ=WEEKLY;BYDAY=MO,WE,FR",
            "FREQ=WEEKLY;INTERVAL=2;BYDAY=TU;COUNT=10",
            "FREQ=WEEKLY;INTERVAL=2;WKST=MO",
            "FREQ=MONTHLY;INTERVAL=2",
            "FREQ=YEARLY;UNTIL=20301231",
        ] {
            assert!(RRule::parse_foreign(accepted).is_some(), "{} should be accepted", accepted);
        }
    }

    #[test]
    fn the_old_enum_becomes_the_rule_it_always_meant() {
        let rule = RRule::from_legacy("weekly", "2026-12-31").unwrap();
        assert_eq!(rule.freq, Freq::Weekly);
        assert_eq!(rule.interval, 1);
        assert_eq!(rule.until, Some(d("2026-12-31")));
        assert!(RRule::from_legacy("none", "").is_none());
        assert!(RRule::from_legacy("", "").is_none());
    }

    #[test]
    fn week_start_is_monday_whatever_the_day() {
        assert_eq!(week_start(d("2026-03-02")), d("2026-03-02")); // a Monday
        assert_eq!(week_start(d("2026-03-08")), d("2026-03-02")); // the Sunday after
        assert_eq!(week_start(d("2026-03-09")), d("2026-03-09")); // the next Monday
    }

    #[test]
    fn an_anchor_is_the_most_recent_landing_on_or_before_a_day() {
        let rule = RRule::parse("FREQ=WEEKLY;INTERVAL=2").unwrap();
        let start = d("2026-03-02");
        assert_eq!(anchor_on_or_before(&rule, start, d("2026-03-02")), Some(d("2026-03-02")));
        assert_eq!(anchor_on_or_before(&rule, start, d("2026-03-10")), Some(d("2026-03-02")));
        assert_eq!(anchor_on_or_before(&rule, start, d("2026-03-16")), Some(d("2026-03-16")));
        assert_eq!(anchor_on_or_before(&rule, start, d("2026-03-01")), None);
    }

    /// With `BYDAY` the anchor has to come from an active week, and reaching
    /// back into an earlier one is the whole reason the search loops.
    #[test]
    fn a_byday_anchor_reaches_back_past_a_skipped_week() {
        let rule = RRule::parse("FREQ=WEEKLY;INTERVAL=2;BYDAY=MO,WE").unwrap();
        let start = d("2026-03-02");
        assert_eq!(anchor_on_or_before(&rule, start, d("2026-03-04")), Some(d("2026-03-04")));
        // The 9th to the 15th is a skipped week; the last landing was the 4th.
        assert_eq!(anchor_on_or_before(&rule, start, d("2026-03-12")), Some(d("2026-03-04")));
        assert_eq!(anchor_on_or_before(&rule, start, d("2026-03-16")), Some(d("2026-03-16")));
    }

    #[test]
    fn ordinals_count_occurrences_and_not_elapsed_time() {
        let weekly = RRule::parse("FREQ=WEEKLY").unwrap();
        let start = d("2026-03-02");
        assert_eq!(ordinal_of(&weekly, start, d("2026-03-02")), 1);
        assert_eq!(ordinal_of(&weekly, start, d("2026-03-16")), 3);

        // Two landings a week: the third is in the second week, not the third.
        let twice = RRule::parse("FREQ=WEEKLY;BYDAY=MO,WE").unwrap();
        assert_eq!(ordinal_of(&twice, start, d("2026-03-02")), 1);
        assert_eq!(ordinal_of(&twice, start, d("2026-03-04")), 2);
        assert_eq!(ordinal_of(&twice, start, d("2026-03-09")), 3);
        assert_eq!(ordinal_of(&twice, start, d("2026-03-11")), 4);
    }

    /// A series that starts mid-week has a short first week, and counting it
    /// as a full one would end a `COUNT` series early.
    #[test]
    fn a_short_first_week_does_not_inflate_the_count() {
        let rule = RRule::parse("FREQ=WEEKLY;BYDAY=MO,WE,FR").unwrap();
        let start = d("2026-03-04"); // a Wednesday
        assert_eq!(ordinal_of(&rule, start, d("2026-03-04")), 1);
        assert_eq!(ordinal_of(&rule, start, d("2026-03-06")), 2);
        assert_eq!(ordinal_of(&rule, start, d("2026-03-09")), 3);
    }

    #[test]
    fn monthly_and_yearly_ordinals_follow_their_interval() {
        let monthly = RRule::parse("FREQ=MONTHLY;INTERVAL=3").unwrap();
        assert_eq!(ordinal_of(&monthly, d("2026-01-15"), d("2026-07-15")), 3);
        let yearly = RRule::parse("FREQ=YEARLY;INTERVAL=4").unwrap();
        assert_eq!(ordinal_of(&yearly, d("2028-02-29"), d("2036-02-29")), 3);
    }
}

#[cfg(test)]
mod editor_contract_tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Deserialize)]
    struct Case {
        name: String,
        rrule: String,
        fields: Fields,
    }

    #[derive(Deserialize)]
    struct Fields {
        freq: String,
        interval: u32,
        #[serde(rename = "byDay")]
        by_day: Vec<String>,
        #[serde(rename = "endMode")]
        end_mode: String,
        until: String,
        count: u32,
    }

    #[derive(Deserialize)]
    struct Fixture {
        cases: Vec<Case>,
    }

    const FIXTURE: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../contracts/rrule.json"
    ));

    /// The other half of the loop: `src/mini-apps/calendar/rrule.ts` writes
    /// these strings from the editor's fields, and this checks that reading
    /// them back here gives the rule the editor thought it was describing.
    #[test]
    fn reads_every_rule_the_editor_can_write() {
        let fixture: Fixture = serde_json::from_str(FIXTURE).expect("contract parses");
        assert!(fixture.cases.len() >= 10, "contract looks truncated");

        for case in &fixture.cases {
            if case.fields.freq == "none" {
                assert!(case.rrule.is_empty(), "{}: no rule means no string", case.name);
                assert!(RRule::parse(&case.rrule).is_none(), "{}", case.name);
                continue;
            }

            let rule = RRule::parse(&case.rrule)
                .unwrap_or_else(|| panic!("{}: {:?} did not parse", case.name, case.rrule));

            assert_eq!(rule.freq.as_str().to_ascii_lowercase(), case.fields.freq, "{}", case.name);
            assert_eq!(rule.interval, case.fields.interval, "{} interval", case.name);

            let days: Vec<String> = rule.by_day.iter().map(|d| weekday_code(*d).to_string()).collect();
            assert_eq!(days, case.fields.by_day, "{} byDay", case.name);

            match case.fields.end_mode.as_str() {
                "count" => assert_eq!(rule.count, Some(case.fields.count), "{} count", case.name),
                "until" => assert_eq!(
                    rule.until.map(|d| d.format("%Y-%m-%d").to_string()).unwrap_or_default(),
                    case.fields.until, "{} until", case.name),
                _ => {
                    assert!(rule.count.is_none(), "{} should not end on a count", case.name);
                    assert!(rule.until.is_none(), "{} should not end on a date", case.name);
                }
            }

            // And the string this side writes is the one the editor wrote.
            assert_eq!(rule.to_rrule_string(), case.rrule, "{} round trip", case.name);
        }
    }
}
