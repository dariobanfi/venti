-- Reports about a site that pilots made.
--
-- Every report so far names two accounts: who reports, and whose flight,
-- page, live track, group or plan it is. A site that pilots made is
-- nobody's: any pilot corrects it, and it stays when the account that
-- added it goes (0013). A report about one therefore names no second
-- account, and does not go when the site's author goes. It goes with its
-- reporter, like every report.
--
-- Nothing a site says is copied here. Every revision of a site is kept
-- (0013), so the report names the revision it is about and whoever runs
-- the server reads the words there.

ALTER TABLE reports ALTER COLUMN about_account_id DROP NOT NULL;

ALTER TABLE reports DROP CONSTRAINT reports_kind_check;
ALTER TABLE reports ADD CONSTRAINT reports_kind_check
    CHECK (kind IN ('flight', 'pilot', 'live_track', 'group', 'plan', 'site'));

-- For a site: the number of the revision the report is about. The one
-- the reporter named, or the one the site stood at when it was reported.
-- Like subject_id it refers to nothing: a report outlives what it is
-- about.
ALTER TABLE reports ADD COLUMN site_revision integer;

-- A report about a site, and no other, is without a second account.
ALTER TABLE reports ADD CONSTRAINT reports_site_is_nobodys
    CHECK ((kind = 'site') = (about_account_id IS NULL));
ALTER TABLE reports ADD CONSTRAINT reports_site_names_its_revision
    CHECK ((kind = 'site') = (site_revision IS NOT NULL)
       AND (site_revision IS NULL OR site_revision >= 1));

-- One open report per reporter and site, whichever revision each is
-- about. The index of 0009 cannot say this: it has the second account in
-- it, and two rows without one are never the same to it.
CREATE UNIQUE INDEX reports_open_once_site_idx
    ON reports (reporter_id, subject_id)
    WHERE closed_at IS NULL AND kind = 'site';
