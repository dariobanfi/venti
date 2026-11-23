-- Open plans: a pilot's word to everyone flying from an area that they
-- mean to fly from a place on a day, with free seats or looking for
-- company. Unlike a group's plan it belongs to no group, so everyone may
-- read it, and every signed-in pilot may say that they are in. Its author
-- chose that, and so did whoever says they are in: both are named on it.
--
-- An open plan hangs on its author's account and goes with it; a pilot's
-- word on it goes with the pilot's account. Plans of days gone by are
-- removed as a group's plans are.

CREATE TABLE open_plans (
    id         text COLLATE "C" PRIMARY KEY,
    author_id  text COLLATE "C" NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    -- The day as a calendar at the place shows it, with no time zone.
    day        date NOT NULL,
    -- The place in the author's words, and where it is: an open plan is
    -- found by its position, so it always has one.
    place      text NOT NULL,
    lat        double precision NOT NULL,
    lon        double precision NOT NULL,
    -- When to meet or to start, as a clock at the place shows it.
    meet_at    time,
    -- The seats the author has free in a car; NULL when the author is
    -- looking for company.
    seats      smallint CHECK (seats BETWEEN 1 AND 8),
    note       text,
    created_at timestamptz NOT NULL,
    changed_at timestamptz NOT NULL
);

-- The plans near a place for some days: by day, then by latitude.
CREATE INDEX open_plans_day_lat_idx ON open_plans (day, lat);
-- An account's plans, counted against its limit, and for when it goes.
CREATE INDEX open_plans_author_idx ON open_plans (author_id, day);

-- Who is in on an open plan: one row for each pilot who said so.
CREATE TABLE open_plan_pilots (
    plan_id    text COLLATE "C" NOT NULL REFERENCES open_plans (id) ON DELETE CASCADE,
    account_id text COLLATE "C" NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    since      timestamptz NOT NULL,
    PRIMARY KEY (plan_id, account_id)
);

-- The plans a pilot is in on, and for when the pilot goes.
CREATE INDEX open_plan_pilots_account_idx ON open_plan_pilots (account_id);

-- An open plan can be reported as a group's plan can, and the report
-- keeps its place and its note as they stood, and no group.
ALTER TABLE reports DROP CONSTRAINT reports_kind_check;
ALTER TABLE reports ADD CONSTRAINT reports_kind_check
    CHECK (kind IN ('flight', 'pilot', 'live_track', 'group', 'plan', 'site', 'comment',
                    'picture', 'open_plan'));
ALTER TABLE reports DROP CONSTRAINT reports_plan_keeps_its_words;
ALTER TABLE reports ADD CONSTRAINT reports_plan_keeps_its_words
    CHECK ((kind IN ('plan', 'open_plan')) = (reported_place IS NOT NULL)
       AND (kind = 'plan') = (group_id IS NOT NULL)
       AND (kind IN ('plan', 'open_plan') OR reported_note IS NULL));
