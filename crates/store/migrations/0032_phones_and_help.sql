-- Push to the phone apps, and what pilots nearby do: the phones an
-- account registered for Apple's and Google's push services, the
-- account's settings for open plans near a place and for being asked for
-- help, the coarse positions of pilots who may be asked, and requests for
-- help after landing with whom each was sent to and who can help.
--
-- Everything here goes with its account (ON DELETE CASCADE); a phone also
-- goes with the session that registered it, so that signing out of a
-- phone ends what it is told.

CREATE TABLE phone_devices (
    platform    text NOT NULL CHECK (platform IN ('apns', 'fcm')),
    -- The token the platform gave the app. With the platform, the phone's
    -- identity: one token is of one account at a time.
    token       text COLLATE "C" NOT NULL,
    account_id  text COLLATE "C" NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    session_id  text COLLATE "C" NOT NULL REFERENCES sessions (id) ON DELETE CASCADE,
    -- An iPhone app built for development, whose token is of Apple's
    -- sandbox.
    sandbox     boolean NOT NULL DEFAULT false,
    created_at  timestamptz NOT NULL,
    PRIMARY KEY (platform, token)
);

-- An account's phones, the oldest first; and a session's, for when it goes.
CREATE INDEX phone_devices_account_idx ON phone_devices (account_id, created_at);
CREATE INDEX phone_devices_session_idx ON phone_devices (session_id);

-- An account's settings for what pilots nearby do. No row: both off.
CREATE TABLE nearby_settings (
    account_id      text COLLATE "C" PRIMARY KEY REFERENCES accounts (id) ON DELETE CASCADE,
    -- The circle the pilot wants to hear of new open plans in, or none.
    plans_lat       double precision,
    plans_lon       double precision,
    plans_radius_km integer CHECK (plans_radius_km BETWEEN 1 AND 100),
    -- Whether pilots nearby may ask the pilot for help after landing.
    may_be_asked    boolean NOT NULL DEFAULT false,
    changed_at      timestamptz NOT NULL,
    CHECK ((plans_lat IS NULL) = (plans_lon IS NULL)
       AND (plans_lat IS NULL) = (plans_radius_km IS NULL))
);

-- The circles, by latitude, for the watchers of a new open plan.
CREATE INDEX nearby_settings_plans_idx ON nearby_settings (plans_lat)
    WHERE plans_lat IS NOT NULL;

-- Where the phone of a pilot who may be asked for help was, taken to a
-- grid of 0.05 degrees by the server before it is kept, and kept for an
-- hour. Shown to nobody: it chooses whom a request is sent to.
CREATE TABLE help_positions (
    account_id  text COLLATE "C" PRIMARY KEY REFERENCES accounts (id) ON DELETE CASCADE,
    lat         double precision NOT NULL,
    lon         double precision NOT NULL,
    told_at     timestamptz NOT NULL
);

CREATE INDEX help_positions_lat_idx ON help_positions (lat);
CREATE INDEX help_positions_told_idx ON help_positions (told_at);

-- A pilot's request for help after landing. Its position is shown to its
-- pilot and to those who said they can help, and to nobody else.
CREATE TABLE help_requests (
    id          text COLLATE "C" PRIMARY KEY,
    account_id  text COLLATE "C" NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    lat         double precision NOT NULL,
    lon         double precision NOT NULL,
    note        text,
    created_at  timestamptz NOT NULL,
    ends_at     timestamptz NOT NULL,
    closed_at   timestamptz,
    CHECK (ends_at > created_at)
);

CREATE INDEX help_requests_account_idx ON help_requests (account_id, created_at DESC);
CREATE INDEX help_requests_ends_idx ON help_requests (ends_at);

-- Whom a request was sent to, nearest first, and who can help.
CREATE TABLE help_recipients (
    request_id  text COLLATE "C" NOT NULL REFERENCES help_requests (id) ON DELETE CASCADE,
    account_id  text COLLATE "C" NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    rank        integer NOT NULL,
    -- When the pilot said they can help; NULL while they have not, or
    -- took it back.
    can_help_at timestamptz,
    PRIMARY KEY (request_id, account_id)
);

CREATE INDEX help_recipients_account_idx ON help_recipients (account_id);

-- A request for help can be reported; the report keeps its note as it
-- stood, and no place.
ALTER TABLE reports DROP CONSTRAINT reports_kind_check;
ALTER TABLE reports ADD CONSTRAINT reports_kind_check
    CHECK (kind IN ('flight', 'pilot', 'live_track', 'group', 'plan', 'site', 'comment',
                    'picture', 'open_plan', 'help_request'));
ALTER TABLE reports DROP CONSTRAINT reports_plan_keeps_its_words;
ALTER TABLE reports ADD CONSTRAINT reports_plan_keeps_its_words
    CHECK ((kind IN ('plan', 'open_plan')) = (reported_place IS NOT NULL)
       AND (kind = 'plan') = (group_id IS NOT NULL)
       AND (kind IN ('plan', 'open_plan', 'help_request') OR reported_note IS NULL));
