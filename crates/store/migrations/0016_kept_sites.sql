-- The sites a pilot keeps as their own, on the account.
--
-- A pilot marks flying sites as theirs: the take-offs they fly from. With
-- an account the list follows the pilot to their other devices. It is
-- private to the account: nothing reads these tables for anybody else,
-- and they go with the account (ON DELETE CASCADE). It is the pilot's own
-- list and no contribution: a row is a copy of a site's record as it was
-- when the pilot kept it, under its source's licence, with the source
-- beside it. Rows of different sources are never put into one record.

CREATE TABLE kept_sites (
    account_id text COLLATE "C" NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    -- The site's identifier as its source gives it ('osm-free-flying:node/1').
    site_id    text COLLATE "C" NOT NULL,
    -- The source the record is of: the part of the identifier before the colon.
    source     text COLLATE "C" NOT NULL,
    -- The number of the change that put the row into the state it is in.
    -- Numbers are per account and only grow; see kept_site_changes.
    change     bigint NOT NULL,
    -- When the pilot kept the site. NULL for a site the pilot has since
    -- forgotten: the row stays for a while, so that the pilot's other
    -- devices learn of it.
    kept_at    timestamptz,
    -- The record as it was when kept, with its source's credit, as JSON
    -- text. The server hands it back as it took it.
    record     text,
    PRIMARY KEY (account_id, site_id),
    CONSTRAINT kept_sites_kept_or_forgotten CHECK ((kept_at IS NULL) = (record IS NULL))
);

-- What changed for an account after a change number, oldest first.
CREATE UNIQUE INDEX kept_sites_change_idx ON kept_sites (account_id, change);

-- The change numbers of an account's kept sites. The row is also what a
-- write locks, so that two writes of one account come one after the other.
CREATE TABLE kept_site_changes (
    account_id text COLLATE "C" PRIMARY KEY REFERENCES accounts (id) ON DELETE CASCADE,
    -- The largest change number the account has used.
    latest     bigint NOT NULL,
    -- The largest change number of a forgotten site whose row was removed
    -- to keep the table bounded. A device that last read before it cannot
    -- be told what was forgotten, and is told everything again.
    floor      bigint NOT NULL DEFAULT 0
);
