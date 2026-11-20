-- What pilots tell whoever runs the server about what is shared on it, and
-- what the operator does about a flight or an account.

-- A flight the operator took down is private and stays private until the
-- operator releases it. Who may see a flight is still said by one column:
-- a flight that is taken down has 'private' there, and the database holds
-- every writer to that.
ALTER TABLE flights ADD COLUMN taken_down_at timestamptz;
ALTER TABLE flights ADD CONSTRAINT flights_taken_down_is_private
    CHECK (taken_down_at IS NULL OR visibility = 'private');

-- A flight that is deleted while it is taken down leaves that behind, so
-- that it comes back taken down when its file is uploaded again: deleting
-- and uploading must not undo what the operator did.
ALTER TABLE flight_deletions ADD COLUMN taken_down_at timestamptz;

-- An account whose publishing the operator suspended can make no flight
-- public, and its live tracks are for those with their link.
ALTER TABLE accounts ADD COLUMN publishing_suspended_at timestamptz;

-- Reports: one row for each thing a pilot reported. A report names the
-- account that made it and the account it is about, and goes when either
-- of them goes. Nothing reads this table back to a pilot.
CREATE TABLE reports (
    id               text COLLATE "C" PRIMARY KEY,
    created_at       timestamptz NOT NULL,
    reporter_id      text COLLATE "C" NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    about_account_id text COLLATE "C" NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    kind             text NOT NULL CHECK (kind IN ('flight', 'pilot', 'live_track')),
    -- The flight's address or the live track's identifier; empty for a
    -- pilot's page, which the account names.
    subject_id       text COLLATE "C" NOT NULL,
    -- One of the reasons the API takes, by its name. No list here: which
    -- reasons there are may change, and a report made under an old one
    -- must stay readable.
    reason           text NOT NULL,
    -- What the reporter wrote beside the reason.
    reporter_text    text,
    -- Set together when the operator closes the report.
    closed_at        timestamptz,
    closing_note     text,
    CHECK ((closed_at IS NULL) = (closing_note IS NULL))
);

-- One open report per reporter and subject: reporting the same thing again
-- adds nothing for the operator to read.
CREATE UNIQUE INDEX reports_open_once_idx
    ON reports (reporter_id, about_account_id, kind, subject_id)
    WHERE closed_at IS NULL;
-- The open reports oldest first, and all of them.
CREATE INDEX reports_open_idx ON reports (created_at, id) WHERE closed_at IS NULL;
CREATE INDEX reports_created_idx ON reports (created_at, id);
-- The closed ones by when they were closed, for removing them after a while.
CREATE INDEX reports_closed_idx ON reports (closed_at) WHERE closed_at IS NOT NULL;
-- The reports of an account and the reports about one, for when it goes.
CREATE INDEX reports_reporter_idx ON reports (reporter_id);
CREATE INDEX reports_about_idx ON reports (about_account_id);
