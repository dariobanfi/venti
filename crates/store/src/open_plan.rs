//! Open plans: a pilot's word to everyone flying from an area that they
//! mean to fly from a place on a day, with seats free in a car or looking
//! for company.
//!
//! An open plan belongs to no group. Everyone may read it, and every
//! signed-in pilot may say that they are in; its author chose that when
//! posting it, and whoever says they are in chooses to be named on it. It
//! goes with its author's account, and a pilot's word on it with the
//! pilot's. Plans of days gone by are removed as a group's plans are
//! ([`Store::prune_plans`](crate::Store::prune_plans)).

use chrono::{NaiveDate, NaiveTime};
use venti_domain::{GeoPoint, Timestamp};

use crate::StoreError;
use crate::group::{check_details, days_kept};
use crate::{GroupPilot, PlanDetails};

/// What an open plan says.
#[derive(Debug, Clone, PartialEq)]
pub struct OpenPlanDetails {
    /// The day it is for, as a calendar at the place shows it.
    pub day: NaiveDate,
    /// The place, in the author's words.
    pub place: String,
    /// Where the place is: an open plan always says, since it is found
    /// by it.
    pub position: GeoPoint,
    /// When to meet or to start, as a clock at the place shows it, to the
    /// minute.
    pub meet_at: Option<NaiveTime>,
    /// The seats the author has free in a car, 1 to 8; `None` when the
    /// author is looking for company.
    pub seats: Option<u8>,
    /// What else the author had to say.
    pub note: Option<String>,
}

/// Refuses what an open plan says if no store keeps it.
pub(crate) fn check_open_details(details: &OpenPlanDetails) -> Result<(), StoreError> {
    check_details(&PlanDetails {
        day: details.day,
        place: details.place.clone(),
        position: Some(details.position),
        meet_at: details.meet_at,
        note: details.note.clone(),
    })?;
    if details.seats.is_some_and(|seats| !(1..=8).contains(&seats)) {
        return Err(StoreError::Invalid(
            "an open plan with other than 1 to 8 seats free".to_owned(),
        ));
    }
    Ok(())
}

/// A day to compare the days of plans with, as every store takes it.
pub(crate) fn bound(day: NaiveDate) -> NaiveDate {
    let (first, last) = days_kept();
    day.clamp(first, last)
}

/// An open plan to store.
#[derive(Debug, Clone, PartialEq)]
pub struct NewOpenPlan {
    /// The plan's identifier.
    pub id: String,
    /// The account that posts it.
    pub author_id: String,
    /// When.
    pub created_at: Timestamp,
    /// What it says.
    pub details: OpenPlanDetails,
}

/// A stored open plan.
#[derive(Debug, Clone, PartialEq)]
pub struct OpenPlan {
    /// The plan's identifier.
    pub id: String,
    /// The pilot who posted it.
    pub author: GroupPilot,
    /// What it says.
    pub details: OpenPlanDetails,
    /// When it was posted.
    pub created_at: Timestamp,
    /// When its author last changed it; the moment it was posted until
    /// then.
    pub changed_at: Timestamp,
    /// The pilots who said that they are in, in the order in which they
    /// said so, and by account identifier among those of the same moment.
    pub pilots_in: Vec<GroupPilot>,
}

impl OpenPlan {
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

    /// Whether the plan is for a day from `from` to `to`, both included,
    /// and its place within `radius_m` of `around`.
    pub fn is_near(&self, area: &OpenPlanArea) -> bool {
        let day = self.details.day;
        day >= area.from
            && day <= area.to
            && self.details.position.distance_m(area.around) <= area.radius_m
    }
}

/// How many open plans an account may hold and how many pilots one may
/// have in. The numbers are the caller's; the store holds to them in the
/// step that would pass them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OpenPlanRoom {
    /// The most open plans one account may hold at a time.
    pub plans_per_account: u32,
    /// The most pilots who may be in on one, its author included.
    pub pilots_per_plan: u32,
    /// The first day whose plans are still kept: a plan for an earlier
    /// day is gone, and takes no place.
    pub gone_before: NaiveDate,
}

/// The open plans asked for: those for the days from `from` to `to`,
/// both included, whose place is within `radius_m` of `around`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OpenPlanArea {
    /// The place asked about.
    pub around: GeoPoint,
    /// How far around it, metres.
    pub radius_m: f64,
    /// The first day.
    pub from: NaiveDate,
    /// The last day.
    pub to: NaiveDate,
}

/// What became of an open plan handed to
/// [`Store::save_open_plan`](crate::Store::save_open_plan).
#[derive(Debug, Clone, PartialEq)]
pub enum OpenPlanSaving {
    /// It was stored, with its author as the first who is in.
    Stored(Box<OpenPlan>),
    /// The author holds as many open plans as an account may. Nothing was
    /// stored.
    TooMany,
}

/// What became of a call that changes one open plan, or a pilot's word
/// on it.
#[derive(Debug, Clone, PartialEq)]
pub enum OpenPlanChange {
    /// The plan as it now is.
    Now(Box<OpenPlan>),
    /// The asker did not post the plan. Only a change to what a plan says
    /// answers this.
    NotAuthor,
    /// As many pilots are in as may be. Only saying "I am in" answers
    /// this.
    Full,
    /// There is no such plan, or none any more.
    NoSuchPlan,
}

/// What became of a call that deletes one open plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenPlanRemoval {
    /// It is gone, with who was in.
    Removed,
    /// The asker did not post it.
    NotAuthor,
    /// There is no such plan, or none any more.
    NoSuchPlan,
}
