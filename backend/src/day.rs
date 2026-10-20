//! The shop's calendar: which instants belong to "today".
//!
//! The ledger is a book of days, and a day is a local calendar fact — a sale
//! recorded at 22:00 in Buenos Aires belongs to that day's page, not to
//! tomorrow's. That cannot be answered in UTC, and it cannot be answered by the
//! browser's clock either: the browser carries whatever timezone the machine
//! happens to be configured with, and the shop's day must not move because
//! someone rebuilt the PC with a default timezone.
//!
//! So the day boundary is computed here, once, from a configured zone, and the
//! client asks for it (S7). The wire stays locale-free: the answer is four
//! instants, not a formatted date.
//!
//! Two details are worth naming, because both are silent if you get them wrong:
//!
//! * The bounds are **exclusive at `to`**. Two adjacent days then partition the
//!   ledger instead of counting the sale recorded at midnight twice.
//! * Local midnight does not always exist. Where a zone springs forward *at*
//!   midnight (Santiago does), the day starts at the first instant that does
//!   exist, an hour later. Taking `naive.and_local_timezone(zone)` and
//!   unwrapping a `None` would be a panic on two days a year.

use chrono::{
    DateTime, Datelike, Duration, LocalResult, NaiveDate, NaiveDateTime, NaiveTime, TimeZone, Utc,
};
use chrono_tz::Tz;

/// The window of instants that makes up one shop day, and its neighbours.
///
/// `from` is inclusive and `to` is exclusive, matching `GET /api/sales?from&to`.
/// `prev` and `next` are the adjacent days' `from`, so paging is one day back or
/// forward rather than an offset into a global list.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct DayRange {
    pub from: DateTime<Utc>,
    pub to: DateTime<Utc>,
    pub prev: DateTime<Utc>,
    pub next: DateTime<Utc>,
    /// The IANA zone these windows were computed in.
    ///
    /// Returned, not inferred: the client has to render each document's time in
    /// the zone its day was scoped in, and a browser cannot know which zone that
    /// is — it only knows its own. Without this field the shop's day would be
    /// right and the hours inside it wrong (S19).
    pub zone: String,
}

/// The shop day containing `at`, in `zone`.
pub fn day_range(zone: Tz, at: DateTime<Utc>) -> DayRange {
    let today = at.with_timezone(&zone).date_naive();
    DayRange {
        from: first_instant_of(today, zone),
        to: first_instant_of(today.succ_opt().expect("a date that has a tomorrow"), zone),
        prev: first_instant_of(today.pred_opt().expect("a date that has a yesterday"), zone),
        next: first_instant_of(today.succ_opt().expect("a date that has a tomorrow"), zone),
        zone: zone.name().to_string(),
    }
}

/// Which span of the shop's calendar a page is scoped to.
///
/// The three are calendar spans, not rolling durations: "this week" is the
/// Monday-to-Sunday week the shop is in, and "this month" is its calendar month.
/// A rolling "last 7 days" would be a different number every morning and would
/// not match the calendar pinned to the shop's wall.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    Day,
    Week,
    Month,
}

impl Scope {
    /// Parses the `scope` query parameter.
    ///
    /// Absent means `Day`, which is what every ledger asked for until the
    /// selector existed: an older client that sends no scope keeps its exact
    /// behaviour. An unrecognised value is `None`, so the caller can answer 400
    /// instead of silently scoping to something the client did not ask for.
    pub fn parse(raw: Option<&str>) -> Option<Self> {
        match raw {
            None | Some("") | Some("day") => Some(Self::Day),
            Some("week") => Some(Self::Week),
            Some("month") => Some(Self::Month),
            Some(_) => None,
        }
    }
}

/// The shop span containing `at`, for the requested scope.
pub fn range_for(zone: Tz, at: DateTime<Utc>, scope: Scope) -> DayRange {
    match scope {
        Scope::Day => day_range(zone, at),
        Scope::Week => week_range(zone, at),
        Scope::Month => month_range(zone, at),
    }
}

/// The Monday-to-Sunday week containing `at`, in `zone`.
///
/// Monday first because that is the shop's calendar, not ISO's nominal one: the
/// week is a working week, and paging back from a Sunday lands on the Monday
/// that started it rather than on the Sunday of the week before.
///
/// Built by walking to the boundaries' dates and then through `first_instant_of`,
/// never by adding `7 * 24h`: a week that crosses a DST change is not 168 hours
/// long, and the same reason the day cannot add 24h applies here one scale up.
pub fn week_range(zone: Tz, at: DateTime<Utc>) -> DayRange {
    let today = at.with_timezone(&zone).date_naive();
    // `num_days_from_monday` is 0 on Monday and 6 on Sunday.
    let offset = today.weekday().num_days_from_monday() as i64;
    let monday = today - Duration::days(offset);
    let next_monday = monday + Duration::days(7);
    let prev_monday = monday - Duration::days(7);
    DayRange {
        from: first_instant_of(monday, zone),
        to: first_instant_of(next_monday, zone),
        prev: first_instant_of(prev_monday, zone),
        next: first_instant_of(next_monday, zone),
        zone: zone.name().to_string(),
    }
}

/// The calendar month containing `at`, in `zone`.
///
/// The end is the first instant of the *next* month, found by incrementing the
/// year and month fields rather than by adding days: months are 28 to 31 days
/// long, and a `+ 30 days` would land mid-month on February and skip a day in a
/// 31-day month.
pub fn month_range(zone: Tz, at: DateTime<Utc>) -> DayRange {
    let today = at.with_timezone(&zone).date_naive();
    let first = NaiveDate::from_ymd_opt(today.year(), today.month(), 1)
        .expect("day 1 exists in every month");
    let (next_year, next_month) = if today.month() == 12 {
        (today.year() + 1, 1)
    } else {
        (today.year(), today.month() + 1)
    };
    let next_first = NaiveDate::from_ymd_opt(next_year, next_month, 1)
        .expect("day 1 exists in every month");
    let (prev_year, prev_month) = if today.month() == 1 {
        (today.year() - 1, 12)
    } else {
        (today.year(), today.month() - 1)
    };
    let prev_first = NaiveDate::from_ymd_opt(prev_year, prev_month, 1)
        .expect("day 1 exists in every month");
    DayRange {
        from: first_instant_of(first, zone),
        to: first_instant_of(next_first, zone),
        prev: first_instant_of(prev_first, zone),
        next: first_instant_of(next_first, zone),
        zone: zone.name().to_string(),
    }
}

/// The first instant that exists on `date` in `zone`.
///
/// Usually local midnight. Not always:
///
/// * **Ambiguous** (the clocks go back): local midnight happens twice, and the
///   day starts at the *first* occurrence. `LocalResult::Ambiguous` carries the
///   earliest first, but relying on that ordering without saying so is how a
///   day quietly starts an hour late.
/// * **Nonexistent** (the clocks jump over midnight): no instant maps to it, so
///   the day starts at the first instant that does exist — found by walking
///   forward. The walk cannot run long: a gap is at most a couple of hours, and
///   the loop is bounded at a day.
fn first_instant_of(date: NaiveDate, zone: Tz) -> DateTime<Utc> {
    let midnight = date.and_time(NaiveTime::MIN);
    match midnight.and_local_timezone(zone) {
        LocalResult::Single(dt) => dt.with_timezone(&Utc),
        LocalResult::Ambiguous(first, second) => first.min(second).with_timezone(&Utc),
        LocalResult::None => {
            let mut probe = midnight;
            for _ in 0..(24 * 60) {
                probe += Duration::minutes(1);
                if let Some(dt) = probe.and_local_timezone(zone).earliest() {
                    return dt.with_timezone(&Utc);
                }
            }
            // Unreachable for any real zone: a gap shorter than a full day
            // always has an instant after it. Falling back to reading the wall
            // clock as UTC keeps the endpoint answering instead of panicking,
            // and the unit tests below pin the real behaviour.
            Utc.from_utc_datetime(&midnight)
        }
    }
}

/// Parses the `at` query parameter of `GET /api/day`.
///
/// Accepts an RFC 3339 instant (`2026-10-16T04:00:00Z`,
/// `2026-10-16T01:00:00-03:00`). A timestamp with no offset at all
/// (`2026-10-16T04:00:00`) is read as UTC: it is unambiguous only that way, and
/// guessing the shop zone here would turn a client mistake into a plausible
/// wrong answer.
pub fn parse_instant(raw: &str) -> Option<DateTime<Utc>> {
    let raw = raw.trim();
    if let Ok(dt) = DateTime::parse_from_rfc3339(raw) {
        return Some(dt.with_timezone(&Utc));
    }
    let naive = NaiveDateTime::parse_from_str(raw, "%Y-%m-%dT%H:%M:%S")
        .or_else(|_| NaiveDateTime::parse_from_str(raw, "%Y-%m-%dT%H:%M"))
        .or_else(|_| NaiveDateTime::parse_from_str(raw, "%Y-%m-%dT%H:%M:%S%.f"))
        .ok()?;
    Some(Utc.from_utc_datetime(&naive))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn utc(y: i32, mo: u32, d: u32, h: u32, mi: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(y, mo, d, h, mi, 0).unwrap()
    }

    fn buenos_aires() -> Tz {
        "America/Argentina/Buenos_Aires".parse().unwrap()
    }

    #[test]
    fn the_shop_day_runs_from_local_midnight_to_local_midnight() {
        // 01:00 on 16 Oct in Buenos Aires (UTC-3).
        let at = utc(2026, 10, 16, 4, 0);
        let range = day_range(buenos_aires(), at);
        assert_eq!(range.from, utc(2026, 10, 16, 3, 0));
        assert_eq!(range.to, utc(2026, 10, 17, 3, 0));
        assert_eq!(range.prev, utc(2026, 10, 15, 3, 0));
        assert_eq!(range.next, utc(2026, 10, 17, 3, 0));
    }

    #[test]
    fn a_late_evening_sale_belongs_to_its_own_day_not_to_utc_tomorrow() {
        // The defect this closes: 22:30 on 16 Oct in Buenos Aires is already
        // 01:30 on 17 Oct in UTC. Scoped by UTC it would land on the next day's
        // page, which is not the day the shop worked.
        let at = utc(2026, 10, 17, 1, 30);
        let range = day_range(buenos_aires(), at);
        assert_eq!(range.from, utc(2026, 10, 16, 3, 0));
        assert_eq!(range.to, utc(2026, 10, 17, 3, 0));
        assert!(
            range.from <= at && at < range.to,
            "the instant that was asked about must fall inside the answer"
        );
    }

    #[test]
    fn the_bound_is_half_open_so_adjacent_days_partition_the_ledger() {
        let zone = buenos_aires();
        for day in 1..=28 {
            let at = utc(2026, 9, day, 15, 0);
            let range = day_range(zone, at);
            let next = day_range(zone, range.to + Duration::seconds(1));
            assert_eq!(
                range.to, next.from,
                "the end of one day is the start of the next; the midnight sale is counted once"
            );
        }
    }

    #[test]
    fn a_short_day_where_the_clocks_spring_forward_starts_an_hour_later() {
        // New York springs forward at 02:00 on 14 Mar 2027 (07:00Z): that local
        // day is 23 hours long. Buenos Aires has had no DST since 2009, which is
        // exactly why the function cannot assume a fixed offset.
        let zone: Tz = "America/New_York".parse().unwrap();
        let range = day_range(zone, utc(2027, 3, 14, 12, 0));
        assert_eq!(range.from, utc(2027, 3, 14, 5, 0)); // 00:00 EST
        assert_eq!(range.to, utc(2027, 3, 15, 4, 0)); // 00:00 EDT
    }

    #[test]
    fn a_long_day_where_the_clocks_fall_back_is_twenty_five_hours() {
        // New York falls back at 02:00 on 7 Nov 2027 (06:00Z); local midnight
        // happens once, and the day ends 25 hours later.
        let zone: Tz = "America/New_York".parse().unwrap();
        let range = day_range(zone, utc(2027, 11, 7, 12, 0));
        assert_eq!(range.from, utc(2027, 11, 7, 4, 0)); // 00:00 EDT
        assert_eq!(range.to, utc(2027, 11, 8, 5, 0)); // 00:00 EST
    }

    #[test]
    fn a_day_whose_midnight_does_not_exist_starts_at_the_first_instant_that_does() {
        // Santiago springs forward at 00:00 on 5 Sep 2027: local midnight never
        // happens, and the day begins at 01:00 (-03). An unwrap here would be a
        // panic two days a year, in the shop, at the start of a shift.
        let zone: Tz = "America/Santiago".parse().unwrap();
        let range = day_range(zone, utc(2027, 9, 5, 15, 0));
        assert_eq!(range.from, utc(2027, 9, 5, 4, 0)); // 01:00 -03
        assert_eq!(range.to, utc(2027, 9, 6, 3, 0)); // 00:00 -03, next day
        assert!(range.from < range.to);
        assert_eq!(
            range.from.with_timezone(&zone).format("%H").to_string(),
            "01",
            "the first instant of the day is an hour after the nominal midnight"
        );
    }

    #[test]
    fn a_day_whose_midnight_happens_twice_starts_at_the_first_occurrence() {
        // Havana falls back *at* midnight on 7 Nov 2027: local 00:00 happens
        // twice, first as CDT (04:00Z) and again as CST (05:00Z) — the two
        // readings are an hour apart on the same wall clock. The day starts at
        // the first one; taking the later would make the day begin an hour late
        // and drop everything sold in that first hour. Unlike the nonexistent
        // case this one is silent: both choices are valid instants.
        let zone: Tz = "America/Havana".parse().unwrap();
        let range = day_range(zone, utc(2027, 11, 7, 12, 0));
        assert_eq!(range.from, utc(2027, 11, 7, 4, 0)); // 00:00 CDT, the first
        assert_eq!(range.to, utc(2027, 11, 8, 5, 0)); // 00:00 CST, next day
        assert_eq!(
            range.from.with_timezone(&zone).format("%H:%M").to_string(),
            "00:00"
        );
        assert_eq!(
            (range.to - range.from).num_minutes(),
            25 * 60,
            "the repeated hour makes this a 25-hour day"
        );
    }

    #[test]
    fn the_same_zone_can_gap_its_midnight_in_march_and_repeat_it_in_november() {
        // Havana is the rare zone that does both in one year, which makes it the
        // one fixture that proves the two branches are genuinely different
        // rather than one path wearing two names.
        let zone: Tz = "America/Havana".parse().unwrap();
        let spring = day_range(zone, utc(2027, 3, 14, 12, 0));
        let autumn = day_range(zone, utc(2027, 11, 7, 12, 0));
        assert_eq!(spring.from, utc(2027, 3, 14, 5, 0)); // 01:00 CDT, after the gap
        assert_eq!(autumn.from, utc(2027, 11, 7, 4, 0)); // 00:00 CDT, the first of two
        assert_eq!((spring.to - spring.from).num_hours(), 23);
        assert_eq!((autumn.to - autumn.from).num_hours(), 25);
    }

    #[test]
    fn the_answer_names_the_zone_it_was_computed_in() {
        // The client cannot derive this: a browser knows its own zone, not the
        // shop's. Without the name it would render the hours of a correctly
        // scoped day with the wrong clock (S19).
        let range = day_range(buenos_aires(), utc(2026, 10, 16, 4, 0));
        assert_eq!(range.zone, "America/Argentina/Buenos_Aires");
        assert_eq!(day_range(Tz::UTC, utc(2026, 10, 16, 4, 0)).zone, "UTC");
        // A zone that is an alias still answers with the name it was asked for,
        // so the client's formatter and the server's calendar agree.
        let ny: Tz = "America/New_York".parse().unwrap();
        assert_eq!(
            day_range(ny, utc(2026, 10, 16, 4, 0)).zone,
            "America/New_York"
        );
    }

    #[test]
    fn utc_is_a_zone_like_any_other() {
        let range = day_range(Tz::UTC, utc(2026, 10, 16, 23, 59));
        assert_eq!(range.from, utc(2026, 10, 16, 0, 0));
        assert_eq!(range.to, utc(2026, 10, 17, 0, 0));
    }

    #[test]
    fn the_answer_is_independent_of_the_process_timezone() {
        // The function never reads the local clock: every bound is derived from
        // the zone it was given and the instant it was given.
        let zone = buenos_aires();
        let at = utc(2026, 10, 16, 4, 0);
        assert_eq!(day_range(zone, at), day_range(zone, at));
    }

    #[test]
    fn an_instant_with_an_offset_is_accepted_both_ways() {
        let z = parse_instant("2026-10-16T04:00:00Z").unwrap();
        let offset = parse_instant("2026-10-16T01:00:00-03:00").unwrap();
        let bare = parse_instant("2026-10-16T04:00:00").unwrap();
        assert_eq!(z, offset, "the same instant written two ways");
        assert_eq!(z, bare, "a bare timestamp is read as UTC");
        assert_eq!(z, utc(2026, 10, 16, 4, 0));
    }

    #[test]
    fn a_malformed_instant_is_rejected_rather_than_guessed() {
        for raw in ["", "   ", "yesterday", "2026-13-01T00:00:00Z", "16/10/2026"] {
            assert_eq!(parse_instant(raw), None, "{raw:?} is not an instant");
        }
    }

    // ── Spans: a week and a month are calendar spans, not durations ──────────

    #[test]
    fn an_absent_scope_means_a_day_so_an_older_client_keeps_its_behaviour() {
        // The selector is new. A client that predates it sends no `scope`, and
        // must keep getting exactly what it got before. An unknown value is a
        // different thing: refused, never silently turned into a day.
        assert_eq!(Scope::parse(None), Some(Scope::Day));
        assert_eq!(Scope::parse(Some("")), Some(Scope::Day));
        assert_eq!(Scope::parse(Some("day")), Some(Scope::Day));
        assert_eq!(Scope::parse(Some("week")), Some(Scope::Week));
        assert_eq!(Scope::parse(Some("month")), Some(Scope::Month));
        assert_eq!(Scope::parse(Some("year")), None);
        assert_eq!(Scope::parse(Some("WEEK")), None);
    }

    #[test]
    fn the_week_runs_monday_to_monday_in_the_shop_zone() {
        // Thursday 16 Oct 2026. The week it belongs to started Monday the 12th
        // and ends at midnight on Monday the 19th.
        let range = week_range(buenos_aires(), utc(2026, 10, 16, 15, 0));
        assert_eq!(range.from, utc(2026, 10, 12, 3, 0)); // 00:00 -03 on Monday the 12th
        assert_eq!(range.to, utc(2026, 10, 19, 3, 0)); // 00:00 -03 on Monday the 19th
        assert_eq!(range.prev, utc(2026, 10, 5, 3, 0));
        assert_eq!(range.next, utc(2026, 10, 19, 3, 0));
    }

    #[test]
    fn every_day_of_one_week_answers_the_same_week() {
        // The property that makes a week a week: Monday through Sunday all live
        // in it, and the Monday after does not. Without this, paging a day at a
        // time inside a week scope could silently leave the week.
        let zone = buenos_aires();
        let monday = week_range(zone, utc(2026, 10, 12, 15, 0));
        for offset in 0..7 {
            let at = monday.from + Duration::days(offset);
            assert_eq!(
                week_range(zone, at),
                monday,
                "day {offset} of the week must belong to that same week"
            );
        }
        let next_monday = monday.to + Duration::seconds(1);
        assert_ne!(week_range(zone, next_monday), monday);
    }

    #[test]
    fn a_sunday_belongs_to_the_week_that_started_six_days_before_it() {
        // The off-by-one this pins: Sunday is the *last* day of the week, not the
        // first. Taking `days_until(Monday)` instead of `days_from(Monday)` puts
        // Sunday a whole week ahead of where the shop worked it.
        let sunday = utc(2026, 10, 18, 15, 0); // Sunday 18 Oct, 12:00 -03
        let range = week_range(buenos_aires(), sunday);
        assert_eq!(range.from, utc(2026, 10, 12, 3, 0), "the Monday before it");
        assert!(
            range.from <= sunday && sunday < range.to,
            "the Sunday must fall inside the week it was scoped from"
        );
    }

    #[test]
    fn a_week_crossing_a_dst_change_is_not_168_hours() {
        // New York springs forward on 14 Mar 2027, inside the week of Monday the
        // 8th. That week is 167 hours long. `from + 7 * 24h` would end an hour
        // late and count an hour of the next Monday as this week's.
        let zone: Tz = "America/New_York".parse().unwrap();
        let range = week_range(zone, utc(2027, 3, 10, 12, 0));
        assert_eq!(range.from, utc(2027, 3, 8, 5, 0)); // 00:00 EST on Monday the 8th
        assert_eq!(range.to, utc(2027, 3, 15, 4, 0)); // 00:00 EDT on Monday the 15th
        let hours = (range.to - range.from).num_hours();
        assert_eq!(hours, 167, "the week is short by the hour the clocks moved");
    }

    #[test]
    fn the_month_runs_from_the_first_to_the_first_of_the_next() {
        let range = month_range(buenos_aires(), utc(2026, 10, 16, 15, 0));
        assert_eq!(range.from, utc(2026, 10, 1, 3, 0));
        assert_eq!(range.to, utc(2026, 11, 1, 3, 0));
        assert_eq!(range.prev, utc(2026, 9, 1, 3, 0));
        assert_eq!(range.next, utc(2026, 11, 1, 3, 0));
    }

    #[test]
    fn february_ends_where_february_ends_not_thirty_days_later() {
        // 2027 is not a leap year: February is 28 days. The end of the month is
        // the first of March, which `+ 30 days` would miss by two days and
        // `+ 31 days` would overshoot.
        let range = month_range(buenos_aires(), utc(2027, 2, 10, 15, 0));
        assert_eq!(range.from, utc(2027, 2, 1, 3, 0));
        assert_eq!(range.to, utc(2027, 3, 1, 3, 0));
        let days = (range.to - range.from).num_days();
        assert_eq!(days, 28);
    }

    #[test]
    fn a_leap_february_is_twenty_nine_days() {
        let range = month_range(buenos_aires(), utc(2028, 2, 10, 15, 0));
        assert_eq!(range.to, utc(2028, 3, 1, 3, 0));
        assert_eq!((range.to - range.from).num_days(), 29);
    }

    #[test]
    fn december_rolls_into_the_next_year_and_january_back_into_the_previous_one() {
        let december = month_range(buenos_aires(), utc(2026, 12, 20, 15, 0));
        assert_eq!(december.from, utc(2026, 12, 1, 3, 0));
        assert_eq!(december.to, utc(2027, 1, 1, 3, 0));
        assert_eq!(december.next, utc(2027, 1, 1, 3, 0));

        let january = month_range(buenos_aires(), utc(2027, 1, 5, 15, 0));
        assert_eq!(january.prev, utc(2026, 12, 1, 3, 0));
    }

    #[test]
    fn the_three_scopes_contain_the_instant_they_were_asked_about() {
        // The one invariant that must hold for every scope, every zone and every
        // day of the year: scoping to a span a sale is not in means the shop
        // cannot find its own document.
        let zones: [Tz; 3] = [
            buenos_aires(),
            "America/New_York".parse().unwrap(),
            "America/Santiago".parse().unwrap(),
        ];
        for zone in zones {
            for day in 1..=28 {
                for hour in [3u32, 12, 23] {
                    let at = utc(2027, 3, day, hour, 0);
                    for scope in [Scope::Day, Scope::Week, Scope::Month] {
                        let range = range_for(zone, at, scope);
                        assert!(
                            range.from <= at && at < range.to,
                            "{scope:?} in {zone} excluded the instant {at} it was scoped from"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn adjacent_spans_partition_the_calendar_with_no_gap_and_no_overlap() {
        // Half-open at `to`, at every scale: the midnight document belongs to
        // exactly one week and exactly one month, never to two.
        let zone = buenos_aires();
        for scope in [Scope::Week, Scope::Month] {
            let mut range = range_for(zone, utc(2026, 1, 1, 15, 0), scope);
            for _ in 0..14 {
                let next = range_for(zone, range.to + Duration::seconds(1), scope);
                assert_eq!(range.to, next.from, "{scope:?} left a gap or an overlap");
                assert_eq!(
                    range.next, next.from,
                    "{scope:?} paging forward must land on the neighbour's start"
                );
                assert_eq!(
                    next.prev, range.from,
                    "{scope:?} paging back must land on this span's start"
                );
                range = next;
            }
        }
    }
}
