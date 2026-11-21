-- Tokens a pilot gives to another program, and the name of the token on
-- what was written through one.
--
-- A session is the app's own sign-in and may do everything the pilot may.
-- A token is made by the pilot for one program they name, opens only what
-- its scopes say, and is withdrawn by the pilot at any time. As with a
-- session the secret itself is not stored, only its SHA-256 hash: whoever
-- reads this table cannot act with it.

CREATE TABLE access_tokens (
    id           text COLLATE "C" PRIMARY KEY,
    account_id   text COLLATE "C" NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    token_hash   bytea NOT NULL UNIQUE,
    -- What the pilot called the program the token is for.
    name         text NOT NULL,
    -- What the token may do, as the names of scopes ('flights:write').
    -- The names are the server's: a name it no longer knows opens nothing,
    -- so the list of names is not held here and a new scope needs no
    -- migration.
    scopes       text[] NOT NULL CHECK (cardinality(scopes) >= 1),
    created_at   timestamptz NOT NULL,
    -- The day, in UTC, of the latest use. No more than the day is kept.
    last_used_on date,
    -- The moment from which the token is no longer good. NULL never ends.
    expires_at   timestamptz
);

-- An account's tokens, the newest first.
CREATE INDEX access_tokens_account_idx ON access_tokens (account_id, created_at DESC, id DESC);

-- A revision of a pilot-made site says through what it came in. A token
-- is the third way, and the revision keeps the name the token had then:
-- text, not a reference, because a revision outlives the token.
ALTER TABLE pilot_site_revisions DROP CONSTRAINT pilot_site_revisions_via_check;
ALTER TABLE pilot_site_revisions ADD CONSTRAINT pilot_site_revisions_via_check
    CHECK (via IN ('session', 'operator', 'token'));
ALTER TABLE pilot_site_revisions ADD COLUMN via_token text;
ALTER TABLE pilot_site_revisions ADD CONSTRAINT pilot_site_revisions_token_has_its_name
    CHECK ((via = 'token') = (via_token IS NOT NULL));
