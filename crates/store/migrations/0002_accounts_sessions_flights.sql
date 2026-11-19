-- Accounts, what proves who owns one, the devices signed in to them, the
-- invites they are created with, and pilots' flights.
--
-- Identifiers are 26 lower-case letters and digits made by the server. The
-- "C" collation sorts them by bytes, as the Rust code does. Every table
-- that belongs to an account names it and goes when the account goes.

CREATE TABLE accounts (
    id            text COLLATE "C" PRIMARY KEY,
    -- Always lower case, so that two handles that differ only in case
    -- cannot both exist.
    handle        text COLLATE "C" NOT NULL UNIQUE CHECK (handle = lower(handle)),
    display_name  text NOT NULL,
    created_at    timestamptz NOT NULL,
    -- The largest change number the account's flights have used. A change
    -- to a flight locks this row, takes the next number and commits it
    -- together with the change, so numbers are handed out in the order the
    -- changes become visible.
    flight_change bigint NOT NULL DEFAULT 0
);

-- What proves that someone owns an account. Today the only kind is
-- 'password', whose secret is the Argon2id hash in its standard text form,
-- parameters and salt included. Passkeys and Sign in with Apple become
-- further kinds, with the columns they need added then; an account may hold
-- several credentials, but only one password.
CREATE TABLE credentials (
    id         bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    account_id text COLLATE "C" NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    kind       text NOT NULL,
    secret     text NOT NULL,
    created_at timestamptz NOT NULL
);

CREATE UNIQUE INDEX credentials_password_idx
    ON credentials (account_id) WHERE kind = 'password';
CREATE INDEX credentials_account_idx ON credentials (account_id);

-- One row per signed-in device. The token itself is not stored, only its
-- SHA-256 hash: whoever reads this table cannot sign in with it.
CREATE TABLE sessions (
    id           text COLLATE "C" PRIMARY KEY,
    account_id   text COLLATE "C" NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    token_hash   bytea NOT NULL UNIQUE,
    device       text,
    created_at   timestamptz NOT NULL,
    last_used_at timestamptz NOT NULL
);

-- An account's sessions, most recently used first.
CREATE INDEX sessions_account_idx ON sessions (account_id, last_used_at DESC, id DESC);
-- Sessions that have not been used for too long, for the daily cleanup.
CREATE INDEX sessions_last_used_idx ON sessions (last_used_at);

-- Invite codes, by their SHA-256 hash. An invite creates one account.
CREATE TABLE invites (
    code_hash  bytea PRIMARY KEY,
    created_at timestamptz NOT NULL,
    expires_at timestamptz,
    used_at    timestamptz,
    -- Who used it. Emptied when that account is deleted; the invite stays
    -- used.
    used_by    text COLLATE "C" REFERENCES accounts (id) ON DELETE SET NULL
);

-- One row per flight of an account. The file is kept as the server
-- compressed it (gzip) and everything from "analysis_version" down is
-- worked out from it, so those columns may be rewritten at any time.
CREATE TABLE flights (
    id                 text COLLATE "C" PRIMARY KEY,
    account_id         text COLLATE "C" NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    -- The hash of the flight's fixes. One account holds a flight once; two
    -- accounts may each hold the same flight.
    identity           text COLLATE "C" NOT NULL,
    -- Followers-only joins this list when follows exist.
    visibility         text NOT NULL CHECK (visibility IN ('private', 'unlisted', 'public')),
    uploaded_at        timestamptz NOT NULL,
    -- The number of the change that last touched the row.
    change             bigint NOT NULL,
    file               bytea NOT NULL,
    file_bytes         integer NOT NULL,
    analysis_version   integer NOT NULL,
    started_at         timestamptz NOT NULL,
    airtime_s          bigint NOT NULL,
    takeoff_at         timestamptz,
    takeoff_lat        double precision,
    takeoff_lon        double precision,
    takeoff_altitude_m integer,
    landing_at         timestamptz,
    landing_lat        double precision,
    landing_lon        double precision,
    landing_altitude_m integer,
    track_length_m     double precision NOT NULL,
    max_altitude_m     integer NOT NULL,
    max_climb_ms       double precision NOT NULL,
    score_kind         text CHECK (score_kind IN ('free_flight', 'free_triangle', 'fai_triangle')),
    score_closed       boolean,
    score_distance_m   double precision,
    score_points       double precision,
    glider             text,
    -- Latitude and longitude of each point in turn.
    outline            double precision[] NOT NULL,
    -- Seconds since the start and metres of each point in turn.
    profile            integer[] NOT NULL,
    UNIQUE (account_id, identity)
);

-- The file is compressed already; trying again would only cost time.
ALTER TABLE flights ALTER COLUMN file SET STORAGE EXTERNAL;

-- An account's flights newest first, and the page after any flight.
CREATE INDEX flights_account_start_idx ON flights (account_id, started_at, id);
-- The same for the flights everyone may see, which can be a few among many.
CREATE INDEX flights_public_start_idx ON flights (account_id, started_at, id)
    WHERE visibility = 'public';
-- What changed for an account after a change number.
CREATE INDEX flights_account_change_idx ON flights (account_id, change);
-- Flights whose derived columns come from an older analysis, in the order
-- in which they are worked out again.
CREATE INDEX flights_analysis_version_idx ON flights (analysis_version, id);

-- Flights an account deleted, kept so that its other devices can be told.
-- Nothing of the flight is left but its address and its identity.
CREATE TABLE flight_deletions (
    account_id text COLLATE "C" NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    flight_id  text COLLATE "C" NOT NULL,
    identity   text COLLATE "C" NOT NULL,
    change     bigint NOT NULL,
    deleted_at timestamptz NOT NULL,
    PRIMARY KEY (account_id, flight_id)
);

CREATE INDEX flight_deletions_change_idx ON flight_deletions (account_id, change);
