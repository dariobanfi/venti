-- Stations, their observations, and the run history of the data sources.

CREATE EXTENSION IF NOT EXISTS postgis WITH SCHEMA public;

CREATE TABLE stations (
    -- The text form of a station identifier, 'source:local'. The "C"
    -- collation sorts by bytes, which is the order the Rust types use, so a
    -- list sorted here is sorted there.
    id            text COLLATE "C" PRIMARY KEY,
    source        text COLLATE "C" NOT NULL,
    name          text NOT NULL,
    position      geography(Point, 4326) NOT NULL,
    elevation_m   double precision,
    -- The first and the latest run of the source that listed the station.
    first_seen_at timestamptz NOT NULL,
    last_seen_at  timestamptz NOT NULL
);

-- A map viewport is a box in longitude and latitude. The geography type
-- measures on the globe, where the straight edges of such a box do not
-- exist, so box queries use the planar view of the position and this index.
-- Add a plain GiST index on "position" with the first query by distance.
CREATE INDEX stations_position_planar_idx
    ON stations USING gist ((position::geometry));

-- One row per station and measuring interval. Partitioned by calendar month
-- (UTC): the server creates a month's partition when its first observation
-- arrives and removes old observations by dropping whole partitions.
CREATE TABLE observations (
    station_id         text COLLATE "C" NOT NULL
                       REFERENCES stations (id) ON DELETE CASCADE,
    time               timestamptz NOT NULL,
    wind_speed_ms      double precision,
    wind_gust_ms       double precision,
    wind_direction_deg double precision,
    gust_direction_deg double precision,
    temperature_c      double precision,
    humidity_pct       double precision,
    pressure_hpa       double precision,
    PRIMARY KEY (station_id, time)
) PARTITION BY RANGE (time);

-- What happened each time a source ran. The health endpoint reads the
-- newest rows per source; the primary key is the index it needs.
CREATE TABLE source_runs (
    source             text COLLATE "C" NOT NULL,
    started_at         timestamptz NOT NULL,
    finished_at        timestamptz NOT NULL,
    healthy            boolean NOT NULL,
    requests           integer NOT NULL,
    failed_requests    integer NOT NULL,
    skipped            integer NOT NULL,
    rejected           integer NOT NULL,
    stations           integer NOT NULL,
    observations       integer NOT NULL,
    newest_observation timestamptz,
    PRIMARY KEY (source, started_at)
);
