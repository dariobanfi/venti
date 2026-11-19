-- Flying sites: what the sources of sites deliver. As for airspace, a source
-- delivers its whole set with every run, and a healthy run's set replaces
-- the one before.

-- The set stored for each source: one row per source that has one.
CREATE TABLE site_sets (
    source     text COLLATE "C" PRIMARY KEY,
    -- When the run that delivered the set started.
    fetched_at timestamptz NOT NULL,
    sites      integer NOT NULL,
    -- The date the set is of, by the source's own word.
    as_of      timestamptz NOT NULL,
    -- Where the set is whole: west, south, east and north of each box the
    -- run collected for, in turn, in degrees.
    covers     double precision[] NOT NULL DEFAULT '{}'
);

-- One row per site. A site belongs to one source and goes when that
-- source's set goes.
CREATE TABLE sites (
    -- The text form of a site identifier, 'source:local', sorted by bytes
    -- as the Rust types sort it.
    id     text COLLATE "C" PRIMARY KEY,
    source text COLLATE "C" NOT NULL REFERENCES site_sets (source) ON DELETE CASCADE,
    lon    double precision NOT NULL,
    lat    double precision NOT NULL,
    -- The canonical record as JSON, as the API sends it. Kept as text and
    -- not as jsonb, so that every number comes back as it went in.
    record text NOT NULL
);

-- A map asks for what lies in a box in longitude and latitude. The index
-- finds the candidates; it stores rounded boxes, so the query compares the
-- coordinates exactly as well.
CREATE INDEX sites_position_idx
    ON sites USING gist (ST_SetSRID(ST_MakePoint(lon, lat), 4326));

-- Replacing or listing the set of one source.
CREATE INDEX sites_source_idx ON sites (source, id);
