-- The alarms a pilot sets on the account, and what they fired.
--
-- An alarm is a fixed rule over data the server has: the wind at a
-- station, or the kind of day the thermal map says a place has. The
-- server checks it where the data arrives and keeps what fired for a
-- while, for the pilot's apps to fetch. Both tables are private to the
-- account: nothing reads them for anybody else, and they go with the
-- account (ON DELETE CASCADE).

CREATE TABLE alerts (
    id          text COLLATE "C" PRIMARY KEY,
    account_id  text COLLATE "C" NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    -- What the pilot calls it.
    name        text NOT NULL,
    -- Whether it is checked.
    enabled     boolean NOT NULL,
    -- 'wind' or 'day', or a kind of a newer server.
    kind        text COLLATE "C" NOT NULL,
    -- For a wind alarm, the station it watches ('dwd-10min:00001').
    station_id  text COLLATE "C",
    -- The rule, as JSON text. The server writes it and reads it.
    rule        text NOT NULL,
    -- What the server noted when it checked the alarm last, as JSON text:
    -- when it fired, which days said so. Empty after a change.
    state       text NOT NULL DEFAULT '',
    created_at  timestamptz NOT NULL,
    changed_at  timestamptz NOT NULL
);

-- An account's alarms, the oldest first.
CREATE INDEX alerts_account_idx ON alerts (account_id, created_at, id);

-- The alarms that watch the stations of a run that just came in.
CREATE INDEX alerts_station_idx ON alerts (station_id) WHERE enabled AND station_id IS NOT NULL;

-- The alarms of a kind, for the checks after a run of the thermal map.
CREATE INDEX alerts_kind_idx ON alerts (kind) WHERE enabled;

-- What alarms fired. A row stays when its alarm is removed, until it is
-- old enough to go.
CREATE TABLE alerts_fired (
    account_id  text COLLATE "C" NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    -- Per account; only grows. The cursor of GET /v1/account/alerts/fired.
    number      bigint NOT NULL,
    alert_id    text COLLATE "C" NOT NULL,
    fired_at    timestamptz NOT NULL,
    -- What fired it, as JSON text.
    record      text NOT NULL,
    PRIMARY KEY (account_id, number)
);

-- The old ones, to remove them.
CREATE INDEX alerts_fired_at_idx ON alerts_fired (fired_at);

-- The largest number an account's fired alarms have used. The row is
-- also what a write locks, so that two alarms of one account that fire
-- at once take two numbers.
CREATE TABLE alerts_fired_numbers (
    account_id  text COLLATE "C" PRIMARY KEY REFERENCES accounts (id) ON DELETE CASCADE,
    latest      bigint NOT NULL
);
