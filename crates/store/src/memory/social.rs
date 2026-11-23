//! Follows, cheers, comments and notices of the in-memory store.

use std::collections::{BTreeMap, BTreeSet};

use venti_domain::Timestamp;

use super::accounts::Accounts;
use super::flights::Flights;
use crate::flight::next_change;
use crate::{
    Cheering, Comment, Commenting, FlightSocial, FollowCounts, FollowSide, Following, NewComment,
    Notice, Notices, SocialGiven, SocialKey, SocialPilot,
};

#[derive(Debug, Default)]
pub(super) struct Social {
    /// By follower, then followed: since when.
    follows: BTreeMap<(String, String), Timestamp>,
    /// By flight, then account: when.
    cheers: BTreeMap<(String, String), Timestamp>,
    /// By identifier.
    comments: BTreeMap<String, NewComment>,
    /// The flights whose pilot takes no comments.
    closed: BTreeSet<String>,
    /// By account: what it was told.
    notices: BTreeMap<String, Told>,
}

/// What one account was told.
#[derive(Debug, Default)]
struct Told {
    /// The largest number used.
    latest: i64,
    /// The notices by number, each with the account that caused it.
    notices: BTreeMap<i64, (String, Notice)>,
}

fn pilot(accounts: &Accounts, account_id: &str, at: Timestamp) -> Option<SocialPilot> {
    let account = accounts.by_id(account_id)?;
    Some(SocialPilot {
        account_id: account.id.clone(),
        handle: account.handle.clone(),
        name: account.display_name.clone(),
        picture: account.picture.clone(),
        at,
    })
}

/// A page of a list sorted newest first.
fn newest_first(
    mut pilots: Vec<SocialPilot>,
    after: Option<&SocialKey>,
    limit: usize,
) -> Vec<SocialPilot> {
    pilots.retain(|pilot| after.is_none_or(|after| &pilot.key() < after));
    pilots.sort_by_key(|pilot| std::cmp::Reverse(pilot.key()));
    pilots.truncate(limit);
    pilots
}

impl Social {
    pub(super) fn follow(
        &mut self,
        follower: &str,
        followed: &str,
        now: Timestamp,
        most: usize,
        accounts: &Accounts,
    ) -> Following {
        if follower == followed
            || accounts.by_id(follower).is_none()
            || accounts.by_id(followed).is_none()
        {
            return Following::NoAccount;
        }
        let key = (follower.to_owned(), followed.to_owned());
        if self.follows.contains_key(&key) {
            return Following::Follows { new: false };
        }
        if self
            .follows
            .keys()
            .filter(|(one, _)| one == follower)
            .count()
            >= most
        {
            return Following::Full;
        }
        self.follows.insert(key, now);
        Following::Follows { new: true }
    }

    pub(super) fn unfollow(&mut self, follower: &str, followed: &str) -> bool {
        self.follows
            .remove(&(follower.to_owned(), followed.to_owned()))
            .is_some()
    }

    pub(super) fn counts(&self, account_id: &str) -> FollowCounts {
        let count = |of: fn(&(String, String)) -> &String| {
            self.follows
                .keys()
                .filter(|key| of(key) == account_id)
                .count() as u64
        };
        FollowCounts {
            followers: count(|key| &key.1),
            following: count(|key| &key.0),
        }
    }

    pub(super) fn follows_pilot(&self, follower: &str, followed: &str) -> bool {
        self.follows
            .contains_key(&(follower.to_owned(), followed.to_owned()))
    }

    pub(super) fn follows(
        &self,
        account_id: &str,
        side: FollowSide,
        named: impl Fn(&str) -> bool,
        after: Option<&SocialKey>,
        limit: usize,
        accounts: &Accounts,
    ) -> Vec<SocialPilot> {
        let pilots = self
            .follows
            .iter()
            .filter_map(|((follower, followed), at)| match side {
                FollowSide::Followers if followed == account_id => Some((follower, at)),
                FollowSide::Following if follower == account_id => Some((followed, at)),
                _ => None,
            })
            .filter(|(other, _)| named(other))
            .filter_map(|(other, at)| pilot(accounts, other, *at))
            .collect();
        newest_first(pilots, after, limit)
    }

    /// The accounts an account follows.
    pub(super) fn followed_by(&self, account_id: &str) -> BTreeSet<String> {
        self.follows
            .keys()
            .filter(|(follower, _)| follower == account_id)
            .map(|(_, followed)| followed.clone())
            .collect()
    }

    pub(super) fn cheer(
        &mut self,
        account_id: &str,
        flight_id: &str,
        now: Timestamp,
        accounts: &Accounts,
        flights: &Flights,
    ) -> Cheering {
        if accounts.by_id(account_id).is_none() || !flights.is_public(flight_id) {
            return Cheering::NoSuchFlight;
        }
        let key = (flight_id.to_owned(), account_id.to_owned());
        if self.cheers.contains_key(&key) {
            return Cheering::Cheers { new: false };
        }
        self.cheers.insert(key, now);
        Cheering::Cheers { new: true }
    }

    pub(super) fn uncheer(&mut self, account_id: &str, flight_id: &str) -> bool {
        self.cheers
            .remove(&(flight_id.to_owned(), account_id.to_owned()))
            .is_some()
    }

    pub(super) fn cheers(
        &self,
        flight_id: &str,
        after: Option<&SocialKey>,
        limit: usize,
        accounts: &Accounts,
    ) -> Vec<SocialPilot> {
        let pilots = self
            .cheers
            .iter()
            .filter(|((flight, _), _)| flight == flight_id)
            .filter_map(|((_, account), at)| pilot(accounts, account, *at))
            .collect();
        newest_first(pilots, after, limit)
    }

    pub(super) fn of_flights(
        &self,
        flight_ids: &[String],
        viewer: Option<&str>,
    ) -> Vec<FlightSocial> {
        flight_ids
            .iter()
            .map(|id| FlightSocial {
                flight_id: id.clone(),
                cheers: self
                    .cheers
                    .keys()
                    .filter(|(flight, _)| flight == id)
                    .count() as u64,
                comments: self
                    .comments
                    .values()
                    .filter(|comment| &comment.flight_id == id)
                    .count() as u64,
                cheered: viewer.is_some_and(|viewer| {
                    self.cheers.contains_key(&(id.clone(), viewer.to_owned()))
                }),
                comments_closed: self.closed.contains(id),
            })
            .collect()
    }

    fn read(comment: &NewComment, accounts: &Accounts) -> Option<Comment> {
        let author = accounts.by_id(&comment.author_id)?;
        Some(Comment {
            id: comment.id.clone(),
            flight_id: comment.flight_id.clone(),
            author_id: comment.author_id.clone(),
            author_handle: author.handle.clone(),
            author_name: author.display_name.clone(),
            author_picture: author.picture.clone(),
            text: comment.text.clone(),
            created_at: comment.created_at,
        })
    }

    pub(super) fn add_comment(
        &mut self,
        new: &NewComment,
        accounts: &Accounts,
        flights: &Flights,
    ) -> Commenting {
        if accounts.by_id(&new.author_id).is_none() || !flights.is_public(&new.flight_id) {
            return Commenting::NoSuchFlight;
        }
        if self.closed.contains(&new.flight_id) {
            return Commenting::Closed;
        }
        self.comments.insert(new.id.clone(), new.clone());
        match Self::read(new, accounts) {
            Some(comment) => Commenting::Added(comment),
            None => Commenting::NoSuchFlight,
        }
    }

    pub(super) fn comments(
        &self,
        flight_id: &str,
        after: Option<&SocialKey>,
        limit: usize,
        accounts: &Accounts,
    ) -> Vec<Comment> {
        let mut comments: Vec<Comment> = self
            .comments
            .values()
            .filter(|comment| comment.flight_id == flight_id)
            .filter_map(|comment| Self::read(comment, accounts))
            .filter(|comment| after.is_none_or(|after| &comment.key() > after))
            .collect();
        comments.sort_by_key(Comment::key);
        comments.truncate(limit);
        comments
    }

    pub(super) fn comment(&self, id: &str, accounts: &Accounts) -> Option<Comment> {
        Self::read(self.comments.get(id)?, accounts)
    }

    pub(super) fn remove_comment(&mut self, id: &str, accounts: &Accounts) -> Option<Comment> {
        let comment = self.comment(id, accounts)?;
        self.comments.remove(id);
        Some(comment)
    }

    pub(super) fn close_comments(
        &mut self,
        account_id: &str,
        flight_id: &str,
        closed: bool,
        flights: &Flights,
    ) -> bool {
        if flights.owner_of(flight_id) != Some(account_id) {
            return false;
        }
        if closed {
            self.closed.insert(flight_id.to_owned());
        } else {
            self.closed.remove(flight_id);
        }
        true
    }

    pub(super) fn given(&self, account_id: &str, accounts: &Accounts) -> SocialGiven {
        let mut cheers: Vec<(String, Timestamp)> = self
            .cheers
            .iter()
            .filter(|((_, account), _)| account == account_id)
            .map(|((flight, _), at)| (flight.clone(), *at))
            .collect();
        cheers.sort_by(|a, b| (a.1, &a.0).cmp(&(b.1, &b.0)));
        let mut comments: Vec<Comment> = self
            .comments
            .values()
            .filter(|comment| comment.author_id == account_id)
            .filter_map(|comment| Self::read(comment, accounts))
            .collect();
        comments.sort_by_key(Comment::key);
        SocialGiven { cheers, comments }
    }

    pub(super) fn add_notice(
        &mut self,
        account_id: &str,
        actor_id: &str,
        at: Timestamp,
        record: &str,
        accounts: &Accounts,
    ) -> bool {
        if accounts.by_id(account_id).is_none() || accounts.by_id(actor_id).is_none() {
            return false;
        }
        let told = self.notices.entry(account_id.to_owned()).or_default();
        told.latest = next_change(told.latest, at);
        let notice = Notice {
            number: told.latest,
            at,
            record: record.to_owned(),
        };
        told.notices
            .insert(told.latest, (actor_id.to_owned(), notice));
        true
    }

    pub(super) fn notices(&self, account_id: &str, since: i64, limit: usize) -> Notices {
        let Some(told) = self.notices.get(account_id) else {
            return Notices::default();
        };
        let mut after: Vec<Notice> = told
            .notices
            .range(since.saturating_add(1)..)
            .map(|(_, (_, notice))| notice.clone())
            .collect();
        let more = after.len() > limit;
        after.truncate(limit);
        Notices {
            notices: after,
            more,
            latest: told.latest,
        }
    }

    /// How many notices of an account come after `after` and are at or
    /// after `from`, up to `most`.
    pub(super) fn count_notices(
        &self,
        account_id: &str,
        after: i64,
        from: Timestamp,
        most: u32,
    ) -> u32 {
        self.notices.get(account_id).map_or(0, |told| {
            let found = told
                .notices
                .range(after.saturating_add(1)..)
                .filter(|(_, (_, notice))| notice.at >= from)
                .take(most as usize)
                .count();
            u32::try_from(found).unwrap_or(most)
        })
    }

    /// The largest number an account's notices used, zero if none.
    pub(super) fn latest_notice(&self, account_id: &str) -> i64 {
        self.notices.get(account_id).map_or(0, |told| told.latest)
    }

    pub(super) fn prune_notices(&mut self, before: Timestamp) -> u64 {
        let mut removed = 0;
        for told in self.notices.values_mut() {
            let had = told.notices.len();
            told.notices.retain(|_, (_, notice)| notice.at >= before);
            removed += (had - told.notices.len()) as u64;
        }
        removed
    }

    /// Removes what was on a flight that is no more.
    pub(super) fn delete_flight(&mut self, flight_id: &str) {
        self.cheers.retain(|(flight, _), _| flight != flight_id);
        self.comments
            .retain(|_, comment| comment.flight_id != flight_id);
        self.closed.remove(flight_id);
    }

    /// Removes what an account gave and was told, and what was on its
    /// flights.
    pub(super) fn delete_account(&mut self, account_id: &str, its_flights: &[String]) {
        for flight in its_flights {
            self.delete_flight(flight);
        }
        self.follows
            .retain(|(follower, followed), _| follower != account_id && followed != account_id);
        self.cheers.retain(|(_, account), _| account != account_id);
        self.comments
            .retain(|_, comment| comment.author_id != account_id);
        self.notices.remove(account_id);
        for told in self.notices.values_mut() {
            told.notices.retain(|_, (actor, _)| actor != account_id);
        }
    }
}
