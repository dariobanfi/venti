-- Airspace: what the airspace sources deliver. A source delivers its whole
-- set with every run, and a healthy run's set replaces the one before.

-- How many airspaces a run delivered. Zero for runs of observation sources.
ALTER TABLE source_runs ADD COLUMN airspaces integer NOT NULL DEFAULT 0;

-- The set stored for each source: one row per source that has one.
CREATE TABLE airspace_sets (
    source        text COLLATE "C" PRIMARY KEY,
    -- When the run that delivered the set started.
    fetched_at    timestamptz NOT NULL,
    airspaces     integer NOT NULL,
    -- The date of the oldest file any airspace of the set was read from.
    file_date     timestamptz,
    -- When the source last changed any of them, if it says.
    newest_change timestamptz
);

-- One row per airspace. An airspace belongs to one source and goes when
-- that source's set goes.
CREATE TABLE airspaces (
    -- The text form of an airspace identifier, 'source:local', sorted by
    -- bytes as the Rust types sort it.
    id     text COLLATE "C" PRIMARY KEY,
    source text COLLATE "C" NOT NULL REFERENCES airspace_sets (source) ON DELETE CASCADE,
    -- The smallest box around the outline, in degrees.
    west   double precision NOT NULL,
    south  double precision NOT NULL,
    east   double precision NOT NULL,
    north  double precision NOT NULL,
    -- The canonical record as JSON, as the API sends it. Kept as text and
    -- not as jsonb, so that every number comes back as it went in.
    record text NOT NULL
);

-- A map asks for what touches a box in longitude and latitude. The index
-- finds the candidates among the airspaces' own boxes; the index stores
-- rounded boxes, so the query compares the four edges exactly as well.
CREATE INDEX airspaces_box_idx
    ON airspaces USING gist (ST_MakeEnvelope(west, south, east, north, 4326));

-- Replacing or listing the set of one source.
CREATE INDEX airspaces_source_idx ON airspaces (source, id);
