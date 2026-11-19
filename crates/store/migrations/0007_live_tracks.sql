-- Live tracks: the positions an app sends while a glider flies, for those
-- its pilot lets see them. They are kept for a short time after a flight
-- and then removed; they are not flights.
--
-- Everything here names an account and goes when the account goes.

-- One row per track that is being flown or ended a short while ago.
CREATE TABLE live_tracks (
    -- What the fixes name the track by: eight bytes in each of their rows
    -- in place of the identifier's twenty-seven.
    key        bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    -- The identifier the app made: 26 lower-case letters and digits. Who
    -- has it can see the track.
    id         text COLLATE "C" NOT NULL UNIQUE,
    account_id text COLLATE "C" NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    visibility text NOT NULL CHECK (visibility IN ('link', 'public')),
    landed     boolean NOT NULL,
    -- The time of the oldest fix.
    started_at timestamptz NOT NULL,
    -- How many fixes the track holds, and the number the last one that
    -- arrived was given.
    fixes      integer NOT NULL,
    last_seq   bigint NOT NULL,
    -- The newest fix, here as well so that the list of tracks on a map
    -- reads no fix.
    last_at              timestamptz NOT NULL,
    last_lat             double precision NOT NULL,
    last_lon             double precision NOT NULL,
    last_altitude_m      integer NOT NULL,
    last_ground_speed_ms double precision,
    last_climb_ms        double precision,
    last_course_deg      double precision
);

-- The tracks everyone may see that had a fix lately: what a map asks for.
-- They are few among all, so the box is compared row by row.
CREATE INDEX live_tracks_public_idx ON live_tracks (last_at) WHERE visibility = 'public';
-- Tracks that ended long enough ago to be removed.
CREATE INDEX live_tracks_last_idx ON live_tracks (last_at);
-- An account's tracks, for when the account goes.
CREATE INDEX live_tracks_account_idx ON live_tracks (account_id);

-- One row per fix. A track has one fix for a second.
CREATE TABLE live_fixes (
    track           bigint NOT NULL REFERENCES live_tracks (key) ON DELETE CASCADE,
    time            timestamptz NOT NULL,
    -- The fix's number in its track, in the order in which fixes arrived.
    seq             bigint NOT NULL,
    lat             double precision NOT NULL,
    lon             double precision NOT NULL,
    altitude_m      integer NOT NULL,
    ground_speed_ms double precision,
    climb_ms        double precision,
    course_deg      double precision,
    PRIMARY KEY (track, time)
);

-- What arrived for a track after a given number.
CREATE UNIQUE INDEX live_fixes_seq_idx ON live_fixes (track, seq);

-- Identifiers that take nothing more: those of tracks that were deleted or
-- removed, and those an account took out of use before anything arrived
-- under them. Nothing of a track is left here but the identifier it had.
CREATE TABLE live_retired (
    id         text COLLATE "C" PRIMARY KEY,
    account_id text COLLATE "C" NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    retired_at timestamptz NOT NULL
);

CREATE INDEX live_retired_account_idx ON live_retired (account_id);
