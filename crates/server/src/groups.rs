//! Groups of pilots, and the plans their members make for a flying day:
//! the numbers that bound them, and which days count.
//!
//! A pilot makes a group and hands its invite code to the pilots who are
//! to be in it. In the group, members post plans: a day, a place in their
//! own words, perhaps a time to meet and a note. The others say that they
//! are in. That is all a group is for so far.
//!
//! **A group is its members' and nobody else's.** It cannot be found by
//! its name, it is in no list, and to an account that is not in it every
//! request about it gets the answer for a group that does not exist. A
//! group shows its members nothing of each other beyond the handle and
//! the display name: not their flights, not their live tracks.
//!
//! **A plan is where people mean to be.** It is kept for a short while
//! after its day and then removed. Nothing here or in the handlers writes
//! a group's name or identifier, an invite code, or anything of a plan
//! into a log line or an error message.
//!
//! **The server knows no time zone.** A plan's day is a calendar date as
//! it is at the place, and its time to meet is what a clock there shows;
//! both are kept as they were sent. Where the server has to say which
//! days are today, ahead or gone, it goes by the date its own clock shows
//! in UTC, and leaves a day of room on the side that would lose a pilot's
//! plan: somewhere on Earth it is always the date before, or the date
//! after.

use chrono::{Days, NaiveDate};
use venti_domain::Timestamp;
use venti_store::{GroupRoom, PlanRoom};

/// For how many days a plan is kept after its day unless the
/// configuration says otherwise.
///
/// Long enough to look up, on the evening and on the day after, where the
/// others were going to fly, and for whoever looks for a pilot who did
/// not come home to read where that pilot meant to be. Short enough that
/// the server does not hold where a group of people were last week.
pub const DEFAULT_PLAN_DAYS: u32 = 2;

/// The numbers that bound groups and plans. They are set here and not in
/// the configuration, but for the days a plan is kept: none of the others
/// is something an operator tunes from day to day.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// Groups one account may be in, those it keeps included. A pilot has
    /// a club, a few circles of flying friends and now and then a trip;
    /// twenty is more than that comes to (an estimate). It bounds what the
    /// answer about all of a pilot's groups has to read.
    pub groups_per_account: u32,
    /// Members one group may have. A group is a circle of pilots who know
    /// each other and plan flying days together: fifty is such a circle
    /// with room to spare (an estimate). Its members are listed whole in
    /// one answer, and one invite code lets all of them in. A club of
    /// hundreds wants roles, several keepers and pages of members, none
    /// of which is built.
    pub members_per_group: u32,
    /// Plans one group may hold at a time, those kept after their day
    /// included. Two weeks ahead at seven plans a day, which is more than
    /// a busy group posts (an estimate). A group's plans are listed whole
    /// in one answer, and this is what bounds that answer.
    pub plans_per_group: u32,
    /// Groups one account may make within a day, whatever becomes of
    /// them. An account is in twenty at most; this keeps one from making
    /// and deleting groups without end.
    pub groups_made_per_day: u32,
    /// Invite codes one account may try within a day, right or wrong. A
    /// code has 80 random bits, so that at twenty tries a day nobody finds
    /// one by trying, from however many accounts. Twenty is more than a
    /// pilot needs who mistypes a code a few times.
    pub joins_per_day: u32,
    /// Plans one account may post or change within a day, whatever
    /// becomes of them. A member who posts fifty plans a day is not
    /// planning; the group's keeper can remove such a member, and this
    /// bounds what there is to clear up.
    pub plans_posted_per_day: u32,
    /// How many days ahead of the server's date a plan may be for: a
    /// year. Nobody plans a flying day further ahead, and a plan for a
    /// day that never comes would never be removed. `None` in the
    /// development mode, whose clock stands still.
    pub days_ahead: Option<u32>,
    /// For how many days a plan is kept after its day. Open plans are
    /// kept as long.
    pub plan_days: u32,
    /// Open plans one account may hold at a time, those kept after their
    /// day included. A pilot plans a few days ahead at most a trip or two
    /// at once (an estimate).
    pub open_plans_per_account: u32,
    /// Open plans one account may post or change within a day, whatever
    /// becomes of them. Everyone reads an open plan: this bounds what one
    /// account can put before everyone in a day.
    pub open_plans_posted_per_day: u32,
    /// Pilots who may be in on one open plan, its author included: a few
    /// cars to a take-off (an estimate). It bounds what one plan's answer
    /// carries.
    pub pilots_per_open_plan: u32,
    /// The most messages one account may post a day, in all its groups.
    pub messages_per_day: u32,
    /// The most pictures one account may send to groups a day.
    pub group_pictures_per_day: u32,
    /// The most pilots one account may invite into groups a day.
    pub invites_per_day: u32,
}

impl Limits {
    /// The limits of a server that keeps a plan for `plan_days` days after
    /// its day.
    pub fn keeping(plan_days: u32) -> Self {
        Self {
            groups_per_account: 20,
            // A public community needs room (decision 26); one cap for
            // every group, the session's choice of 2026-11-24.
            members_per_group: 1000,
            plans_per_group: 100,
            groups_made_per_day: 10,
            joins_per_day: 20,
            plans_posted_per_day: 50,
            days_ahead: Some(365),
            plan_days,
            open_plans_per_account: 5,
            open_plans_posted_per_day: 10,
            pilots_per_open_plan: 30,
            messages_per_day: 500,
            group_pictures_per_day: 60,
            invites_per_day: 50,
        }
    }

    /// Limits for the development mode. Its clock stands still, so a count
    /// of attempts would never start again, and an app under test may run
    /// in any later year: no count of attempts bites, and a plan may be
    /// for any day from that mode's yesterday on. Everything else is as on
    /// a real server.
    pub fn none() -> Self {
        Self {
            groups_made_per_day: u32::MAX,
            joins_per_day: u32::MAX,
            plans_posted_per_day: u32::MAX,
            open_plans_posted_per_day: u32::MAX,
            messages_per_day: u32::MAX,
            group_pictures_per_day: u32::MAX,
            invites_per_day: u32::MAX,
            days_ahead: None,
            ..Self::default()
        }
    }

    /// How large groups may grow, as the store is told.
    pub fn room(&self) -> GroupRoom {
        GroupRoom {
            groups_per_account: self.groups_per_account,
            members_per_group: self.members_per_group,
        }
    }

    /// How many plans a group may hold at `now`, as the store is told.
    pub fn plan_room(&self, now: Timestamp) -> PlanRoom {
        PlanRoom {
            plans_per_group: self.plans_per_group,
            gone_before: self.gone_before(now),
        }
    }

    /// The first day a plan may be made or changed to be for at `now`: the
    /// day before the server's date. West of Greenwich that day is not
    /// over when the server's is.
    pub fn first_day(&self, now: Timestamp) -> NaiveDate {
        days_before(today(now), 1)
    }

    /// The last day a plan may be for at `now`; `None` where any day
    /// ahead is taken.
    pub fn last_day(&self, now: Timestamp) -> Option<NaiveDate> {
        let ahead = Days::new(u64::from(self.days_ahead?));
        Some(today(now).checked_add_days(ahead).unwrap_or(NaiveDate::MAX))
    }

    /// The first day whose plans are still there at `now`. A plan for an
    /// earlier day is gone, whether or not the cleanup has removed it.
    pub fn gone_before(&self, now: Timestamp) -> NaiveDate {
        gone_before(now, self.plan_days)
    }
}

impl Default for Limits {
    fn default() -> Self {
        Self::keeping(DEFAULT_PLAN_DAYS)
    }
}

/// The date the server's clock shows at `now`, in UTC. This is the
/// server's "today".
pub fn today(now: Timestamp) -> NaiveDate {
    now.date_naive()
}

fn days_before(day: NaiveDate, days: u32) -> NaiveDate {
    day.checked_sub_days(Days::new(u64::from(days)))
        .unwrap_or(NaiveDate::MIN)
}

/// The first day whose plans are still kept at `now` on a server that
/// keeps a plan for `plan_days` days after its day.
///
/// A day ends last where the clocks are twelve hours behind UTC, at noon
/// of the server's next date. With one day kept or more, no plan is
/// therefore gone before its day has ended everywhere.
pub fn gone_before(now: Timestamp, plan_days: u32) -> NaiveDate {
    days_before(today(now), plan_days)
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};

    use super::*;

    fn day(year: i32, month: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(year, month, day).unwrap()
    }

    #[test]
    fn the_days_that_count_follow_the_servers_date_in_utc() {
        let limits = Limits::default();
        // The moment of the development mode's clock, and the last and the
        // first second of a date.
        for (hour, minute, second) in [(19, 5, 0), (23, 59, 59), (0, 0, 0)] {
            let now = Utc
                .with_ymd_and_hms(2026, 11, 18, hour, minute, second)
                .unwrap();
            assert_eq!(today(now), day(2026, 11, 18));
            // A plan is taken for yesterday, for those west of Greenwich,
            // and for a year ahead.
            assert_eq!(limits.first_day(now), day(2026, 11, 17));
            assert_eq!(limits.last_day(now), Some(day(2027, 11, 18)));
            // Two days after its day a plan is still there; on the third
            // it is gone.
            assert_eq!(limits.gone_before(now), day(2026, 11, 16));
            assert_eq!(limits.plan_room(now).gone_before, day(2026, 11, 16));
        }
        let new_year = Utc.with_ymd_and_hms(2027, 1, 1, 0, 30, 0).unwrap();
        assert_eq!(limits.first_day(new_year), day(2026, 12, 31));
        assert_eq!(Limits::keeping(7).gone_before(new_year), day(2026, 12, 25));
        assert_eq!(gone_before(new_year, 1), day(2026, 12, 31));
    }

    #[test]
    fn the_development_mode_takes_plans_for_any_day_ahead_and_counts_nothing() {
        let (real, dev) = (Limits::default(), Limits::none());
        let now = Utc.with_ymd_and_hms(2026, 11, 18, 19, 5, 0).unwrap();
        assert_eq!(dev.last_day(now), None);
        assert_eq!(dev.first_day(now), real.first_day(now));
        assert_eq!(dev.gone_before(now), real.gone_before(now));
        assert_eq!(dev.room(), real.room());
        assert_eq!(dev.plan_room(now), real.plan_room(now));
        assert_eq!(dev.joins_per_day, u32::MAX);
        // A real server's numbers, as the documents state them.
        assert_eq!(
            (real.groups_per_account, real.members_per_group),
            (20, 1000)
        );
        assert_eq!(real.plans_per_group, 100);
        assert_eq!(
            (
                real.groups_made_per_day,
                real.joins_per_day,
                real.plans_posted_per_day
            ),
            (10, 20, 50)
        );
        assert_eq!(real.plan_days, DEFAULT_PLAN_DAYS);
    }
}
