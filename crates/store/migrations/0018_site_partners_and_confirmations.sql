-- Accounts that whoever runs the server has named as trusted to add and
-- correct flying sites of the server's own layer (`pilot_sites`).
--
-- One fact about an account and no system of roles: since when it may
-- write a site, or NULL for every account that may not, which is every
-- account until the operator names one (`venti-server grant-sites`).
-- Withdrawing it sets the column to NULL again and touches nothing the
-- account wrote: its sites and revisions stay, with it as their author.
--
-- Until this migration any signed-in pilot could write a site on a server
-- whose setting `sites_from_pilots` said so. No account is trusted by this
-- migration: on a server that had the setting on, whoever wrote sites
-- before is named again by hand.

ALTER TABLE accounts ADD COLUMN edits_sites_since timestamptz;

-- What pilots say of a site of the server's own: "I took off here and it
-- was good."
--
-- One row for an account and a site: saying it again moves the day. It is
-- the pilot's word and not a record given to everyone, so it goes with
-- the account, where a site and its revisions stay (0013). It is no
-- revision either: the site takes a new change number (`pilot_sites.change`)
-- when one is given or withdrawn, so that whoever follows "what changed
-- since" reads the site's count again, and its history says nothing of it.
CREATE TABLE site_confirmations (
    site_id     text COLLATE "C" NOT NULL REFERENCES pilot_sites (id),
    account_id  text COLLATE "C" NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    -- The day the pilot speaks of, as they named it.
    day         date NOT NULL,
    -- Text the pilot typed, cleaned to one line by the server.
    note        text,
    -- Whether the server found a flight of the pilot's that took off at
    -- the site when this was said. The flight is not named here: its
    -- address is the key to its page.
    with_flight boolean NOT NULL DEFAULT false,
    -- When it was said, or said again.
    at          timestamptz NOT NULL,
    PRIMARY KEY (site_id, account_id)
);

CREATE INDEX site_confirmations_account_idx ON site_confirmations (account_id);
