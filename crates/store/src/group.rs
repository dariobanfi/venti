//! Groups of pilots, and the plans their members make for a flying day.
//!
//! A group is its members' and nobody else's. So every call here names the
//! account that asks, and an account that is not in a group gets what it
//! would get for a group that does not exist: the two are one value, and
//! no caller can tell them apart by mistake.
//!
//! What a pilot put into a group hangs on their being a member of it.
//! When they leave, when the keeper removes them or when their account is
//! deleted, the plans they posted there go, and so does their word on the
//! plans of others.
//!
//! Whoever runs the server is no member and is not asked for an account:
//! the few calls made for the operator ([`Store::any_group`](crate::Store::any_group)
//! and those listed with it) answer for any group. No request of a pilot's
//! reaches one of them.

use chrono::{NaiveDate, NaiveTime};
use venti_domain::{GeoPoint, Timestamp};

use crate::{SecretHash, StoreError};

/// Refuses text no store keeps: text with a zero byte in it, which the
/// database does not take. The API lets none through.
fn check_text(what: &str, text: &str) -> Result<(), StoreError> {
    if text.contains('\0') {
        return Err(StoreError::Invalid(format!(
            "{what} with a zero byte in it"
        )));
    }
    Ok(())
}

/// Refuses a group no store keeps.
pub(crate) fn check_group(new: &NewGroup) -> Result<(), StoreError> {
    check_text("a group's identifier", &new.id)?;
    check_text("an account's identifier", &new.keeper_id)?;
    check_name(&new.name)
}

/// Refuses a name no store keeps.
pub(crate) fn check_name(name: &str) -> Result<(), StoreError> {
    check_text("a group's name", name)
}

/// The first and the last day a plan can be for: the years 1 to 9999,
/// which every calendar in use and the database have.
pub(crate) fn days_kept() -> (NaiveDate, NaiveDate) {
    let day = |year, month, day| NaiveDate::from_ymd_opt(year, month, day).unwrap_or_default();
    (day(1, 1, 1), day(9999, 12, 31))
}

/// Refuses what a plan says if no store keeps it.
pub(crate) fn check_details(details: &PlanDetails) -> Result<(), StoreError> {
    let (first, last) = days_kept();
    if !(first..=last).contains(&details.day) {
        return Err(StoreError::Invalid(
            "a plan for a day outside the years 1 to 9999".to_owned(),
        ));
    }
    check_text("a plan's place", &details.place)?;
    check_text("a plan's note", details.note.as_deref().unwrap_or_default())?;
    // A time to meet is kept to the minute.
    let whole_minutes = details.meet_at.is_none_or(|time| {
        use chrono::Timelike;
        time.second() == 0 && time.nanosecond() == 0
    });
    if !whole_minutes {
        return Err(StoreError::Invalid(
            "a plan's time to meet that is not a whole minute".to_owned(),
        ));
    }
    Ok(())
}

/// Refuses a plan no store keeps.
pub(crate) fn check_plan(new: &NewPlan) -> Result<(), StoreError> {
    check_text("a plan's identifier", &new.id)?;
    check_details(&new.details)
}

/// Whether identifiers can be the identifiers of anything. One with a zero
/// byte in it is the identifier of nothing, which is known without asking
/// a database that would refuse to compare it.
pub(crate) fn can_name(identifiers: &[&str]) -> bool {
    identifiers.iter().all(|text| !text.contains('\0'))
}

/// A pilot as the members of a group see each other.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupPilot {
    /// The account's identifier inside the server.
    pub account_id: String,
    /// Its handle.
    pub handle: String,
    /// The name the pilot is shown by.
    pub display_name: String,
}

/// A group.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Group {
    /// The group's identifier.
    pub id: String,
    /// Its name.
    pub name: String,
    /// When it was made.
    pub created_at: Timestamp,
    /// The member who keeps it.
    pub keeper: GroupPilot,
    /// How many members it has, the keeper included.
    pub members: u32,
}

/// A group with its members.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupMembers {
    /// The group.
    pub group: Group,
    /// Its members, the one who has been in it longest first, and by
    /// account identifier among those who came in at the same moment. The
    /// first of them who is not the keeper is who keeps the group next.
    pub members: Vec<GroupPilot>,
}

/// A group to make.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewGroup {
    /// The group's identifier.
    pub id: String,
    /// Its name.
    pub name: String,
    /// When it is made.
    pub created_at: Timestamp,
    /// The account that makes it: its keeper and first member.
    pub keeper_id: String,
    /// The hash of the invite code that lets others in. The store never
    /// sees a code.
    pub invite: SecretHash,
}

/// How large groups may grow. The numbers are the caller's; the store
/// holds to them in the step that would pass them, so that two calls
/// cannot share the last place.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GroupRoom {
    /// The most groups one account may be in, those it keeps included.
    pub groups_per_account: u32,
    /// The most members one group may have.
    pub members_per_group: u32,
}

/// What became of an attempt to make a group.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GroupCreation {
    /// The group exists now, with its maker as keeper and only member.
    Created(Group),
    /// The account is in as many groups as it may be. Nothing was stored.
    TooManyGroups,
}

/// What became of an attempt to come into a group with an invite code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GroupJoining {
    /// The account is a member now.
    Joined(Group),
    /// The account was a member already. Nothing was changed.
    AlreadyIn(Group),
    /// No group has this invite code: it never was one, a newer one
    /// replaced it, or it ended. This is found first, so that without a
    /// good code nothing is learned about any group.
    NoSuchInvite,
    /// The group has as many members as it may have.
    GroupFull,
    /// The account is in as many groups as it may be.
    TooManyGroups,
}

/// What became of taking a member out of a group.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemberRemoval {
    /// The member is out, and what they put into the group is gone.
    Removed,
    /// The member was the last one: the group is gone with them.
    GroupRemoved,
    /// The asker is a member, but not the keeper, and named someone else.
    NotKeeper,
    /// The keeper named a handle that no member of the group has.
    NoSuchMember,
    /// There is no such group, or the asker is not in it.
    NoSuchGroup,
}

/// What became of something only the keeper of a group may do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeeperAct<T> {
    /// It is done.
    Done(T),
    /// The asker is a member of the group, but not its keeper. Nothing was
    /// changed.
    NotKeeper,
    /// The handle named is the handle of no member of the group. Only
    /// calls that name a member answer this.
    NoSuchMember,
    /// There is no such group, or the asker is not in it.
    NoSuchGroup,
}

/// A group that whoever runs the server removed
/// ([`Store::remove_group`](crate::Store::remove_group)), as it was at
/// that moment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemovedGroup {
    /// The group with its members.
    pub group: GroupMembers,
    /// How many plans went with it, those of days gone by not counted.
    pub plans: u32,
}

/// What a plan says.
#[derive(Debug, Clone, PartialEq)]
pub struct PlanDetails {
    /// The day it is for, as a calendar at the place shows it. No time
    /// zone goes with it.
    pub day: NaiveDate,
    /// The place, in the author's words.
    pub place: String,
    /// Where the place is, if the author marked it.
    pub position: Option<GeoPoint>,
    /// When to meet or to start, as a clock at the place shows it, to the
    /// minute.
    pub meet_at: Option<NaiveTime>,
    /// What else the author had to say.
    pub note: Option<String>,
}

/// A plan to store.
#[derive(Debug, Clone, PartialEq)]
pub struct NewPlan {
    /// The plan's identifier.
    pub id: String,
    /// The group it is posted to.
    pub group_id: String,
    /// The member who posts it.
    pub author_id: String,
    /// When.
    pub created_at: Timestamp,
    /// What it says.
    pub details: PlanDetails,
}

/// A stored plan.
#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    /// The plan's identifier.
    pub id: String,
    /// The group it belongs to.
    pub group_id: String,
    /// The member who posted it.
    pub author: GroupPilot,
    /// What it says.
    pub details: PlanDetails,
    /// When it was posted.
    pub created_at: Timestamp,
    /// When its author last changed it; the moment it was posted until
    /// then.
    pub changed_at: Timestamp,
    /// The members who said that they are in, in the order in which they
    /// said so, and by account identifier among those of the same moment.
    pub pilots_in: Vec<GroupPilot>,
}

impl Plan {
    /// The plan's place in a list: by day, within a day by the time to
    /// meet with the plans that name none last, then in the order in which
    /// they were posted, then by identifier.
    pub fn key(&self) -> (NaiveDate, bool, Option<NaiveTime>, Timestamp, &str) {
        let details = &self.details;
        (
            details.day,
            details.meet_at.is_none(),
            details.meet_at,
            self.created_at,
            &self.id,
        )
    }
}

/// Which plans count as there. A plan for a day before `gone_before` is
/// gone, whether or not [`Store::prune_plans`](crate::Store::prune_plans)
/// has come by.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlanRoom {
    /// The most plans one group may hold at a time.
    pub plans_per_group: u32,
    /// The first day whose plans are still kept.
    pub gone_before: NaiveDate,
}

/// What became of a plan handed to
/// [`Store::save_plan`](crate::Store::save_plan).
#[derive(Debug, Clone, PartialEq)]
pub enum PlanSaving {
    /// It was stored, with its author as the first who is in.
    Stored(Box<Plan>),
    /// The group holds as many plans as it may. Nothing was stored.
    GroupFull,
    /// There is no such group, or the author is not in it.
    NoSuchGroup,
}

/// What became of a call that changes one plan, or a pilot's word on it.
#[derive(Debug, Clone, PartialEq)]
pub enum PlanChange {
    /// The plan as it now is.
    Now(Box<Plan>),
    /// The asker is a member of the group and did not post the plan.
    /// Nothing was changed. Only a change to what a plan says answers
    /// this.
    NotAuthor,
    /// The group has no such plan, or none any more.
    NoSuchPlan,
    /// There is no such group, or the asker is not in it.
    NoSuchGroup,
}

/// What became of a call that deletes one plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlanRemoval {
    /// It is gone, with who was in.
    Removed,
    /// The asker is a member of the group, and neither posted the plan nor
    /// keeps the group.
    NotAllowed,
    /// The group has no such plan, or none any more.
    NoSuchPlan,
    /// There is no such group, or the asker is not in it.
    NoSuchGroup,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn details() -> PlanDetails {
        PlanDetails {
            day: NaiveDate::from_ymd_opt(2026, 11, 19).unwrap(),
            place: "Musterberg".to_owned(),
            position: None,
            meet_at: NaiveTime::from_hms_opt(9, 30, 0),
            note: None,
        }
    }

    #[test]
    fn text_with_a_zero_byte_and_a_time_with_seconds_are_refused() {
        assert_eq!(check_details(&details()), Ok(()));
        let place = PlanDetails {
            place: "Muster\0berg".to_owned(),
            ..details()
        };
        assert!(matches!(check_details(&place), Err(StoreError::Invalid(_))));
        let note = PlanDetails {
            note: Some("\0".to_owned()),
            ..details()
        };
        assert!(matches!(check_details(&note), Err(StoreError::Invalid(_))));
        let seconds = PlanDetails {
            meet_at: NaiveTime::from_hms_opt(9, 30, 15),
            ..details()
        };
        assert!(matches!(
            check_details(&seconds),
            Err(StoreError::Invalid(_))
        ));
        for far in [NaiveDate::MIN, NaiveDate::MAX] {
            let day = PlanDetails {
                day: far,
                ..details()
            };
            assert!(matches!(check_details(&day), Err(StoreError::Invalid(_))));
        }
        assert!(matches!(check_name("a\0"), Err(StoreError::Invalid(_))));
        assert!(can_name(&["0000", "x' OR '1'='1"]));
        assert!(!can_name(&["0000", "\0"]));
    }

    #[test]
    fn plans_sort_by_day_then_by_the_time_to_meet_with_those_without_one_last() {
        let plan = |id: &str, day: u32, meet: Option<(u32, u32)>, posted: i64| Plan {
            id: id.to_owned(),
            group_id: "g".to_owned(),
            author: GroupPilot {
                account_id: "a".to_owned(),
                handle: "anna".to_owned(),
                display_name: "Anna".to_owned(),
            },
            details: PlanDetails {
                day: NaiveDate::from_ymd_opt(2026, 11, day).unwrap(),
                meet_at: meet.and_then(|(hour, minute)| NaiveTime::from_hms_opt(hour, minute, 0)),
                ..details()
            },
            created_at: chrono::DateTime::from_timestamp(posted, 0).unwrap(),
            changed_at: chrono::DateTime::from_timestamp(posted, 0).unwrap(),
            pilots_in: Vec::new(),
        };
        let mut plans = [
            plan("f", 20, None, 5),
            plan("e", 19, None, 9),
            plan("d", 19, None, 7),
            plan("c", 19, None, 7),
            plan("b", 19, Some((14, 0)), 8),
            plan("a", 19, Some((9, 30)), 9),
        ];
        plans.sort_by(|a, b| a.key().cmp(&b.key()));
        let order: Vec<&str> = plans.iter().map(|plan| plan.id.as_str()).collect();
        assert_eq!(order, ["a", "b", "c", "d", "e", "f"]);
    }
}
