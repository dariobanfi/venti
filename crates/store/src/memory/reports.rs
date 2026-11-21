//! Reports of the in-memory store.

use std::collections::BTreeMap;

use venti_domain::Timestamp;

use super::accounts::Accounts;
use crate::report::{check, check_note, is_the_same_report};
use crate::{
    NewReport, Report, ReportClosing, ReportClosure, ReportKey, ReportListing, ReportSaved,
    StoreError,
};

#[derive(Debug, Default)]
pub(super) struct Reports {
    /// By identifier.
    reports: BTreeMap<String, Entry>,
}

#[derive(Debug)]
struct Entry {
    new: NewReport,
    closed: Option<ReportClosure>,
}

impl Reports {
    /// A stored report with its two accounts as they are now.
    fn read(entry: &Entry, accounts: &Accounts) -> Option<Report> {
        let reporter = accounts.by_id(&entry.new.reporter_id)?;
        let about = accounts.by_id(&entry.new.about_account_id)?;
        Some(Report {
            id: entry.new.id.clone(),
            created_at: entry.new.created_at,
            reporter_handle: reporter.handle.clone(),
            about_account_id: about.id.clone(),
            about_handle: about.handle.clone(),
            about_name: about.display_name.clone(),
            subject: entry.new.subject.clone(),
            reason: entry.new.reason.clone(),
            text: entry.new.text.clone(),
            closed: entry.closed.clone(),
        })
    }

    pub(super) fn save(
        &mut self,
        new: &NewReport,
        accounts: &Accounts,
    ) -> Result<ReportSaved, StoreError> {
        check(new)?;
        let both_there = accounts.by_id(&new.reporter_id).is_some()
            && accounts.by_id(&new.about_account_id).is_some();
        if !both_there {
            return Ok(ReportSaved::NoSuchAccount);
        }
        let already_open = self
            .reports
            .values()
            .any(|entry| entry.closed.is_none() && is_the_same_report(&entry.new, new));
        if already_open {
            return Ok(ReportSaved::AlreadyOpen);
        }
        if self.reports.contains_key(&new.id) {
            return Err(StoreError::Invalid(
                "a report under an identifier that is in use".to_owned(),
            ));
        }
        let entry = Entry {
            new: new.clone(),
            closed: None,
        };
        self.reports.insert(new.id.clone(), entry);
        Ok(ReportSaved::Stored)
    }

    pub(super) fn list(
        &self,
        listing: ReportListing,
        after: Option<&ReportKey>,
        limit: usize,
        accounts: &Accounts,
    ) -> Vec<Report> {
        let mut reports: Vec<Report> = self
            .reports
            .values()
            .filter(|entry| listing == ReportListing::All || entry.closed.is_none())
            .filter_map(|entry| Self::read(entry, accounts))
            .filter(|report| after.is_none_or(|after| &report.key() > after))
            .collect();
        reports.sort_by_key(Report::key);
        reports.truncate(limit);
        reports
    }

    pub(super) fn close(
        &mut self,
        id: &str,
        note: &str,
        now: Timestamp,
    ) -> Result<ReportClosing, StoreError> {
        check_note(note)?;
        let Some(entry) = self.reports.get_mut(id) else {
            return Ok(ReportClosing::NoSuchReport);
        };
        if let Some(closed) = &entry.closed {
            return Ok(ReportClosing::AlreadyClosed(closed.clone()));
        }
        entry.closed = Some(ReportClosure {
            at: now,
            note: note.to_owned(),
        });
        Ok(ReportClosing::Closed)
    }

    pub(super) fn prune(&mut self, closed_before: Timestamp) -> u64 {
        let before = self.reports.len();
        self.reports.retain(|_, entry| {
            entry
                .closed
                .as_ref()
                .is_none_or(|closed| closed.at >= closed_before)
        });
        (before - self.reports.len()) as u64
    }

    /// Removes the reports an account made and the reports about it.
    pub(super) fn delete_account(&mut self, account_id: &str) {
        self.reports.retain(|_, entry| {
            entry.new.reporter_id != account_id && entry.new.about_account_id != account_id
        });
    }
}
