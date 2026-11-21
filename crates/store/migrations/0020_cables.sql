-- Cables in the air, one whole set per source, as for sites.

CREATE TABLE cable_sets (
    source     text COLLATE "C" PRIMARY KEY,
    -- When the run that delivered the set started.
    fetched_at timestamptz NOT NULL,
    cables     integer NOT NULL,
    -- The date the set is of, by the source's own word.
    as_of      timestamptz NOT NULL,
    -- Where the set is whole: west, south, east and north of each box the
    -- run collected for, in turn, in degrees.
    covers     double precision[] NOT NULL DEFAULT '{}'
);

-- One row per cable, with the box around its line. A cable belongs to one
-- source and goes when that source's set goes.
CREATE TABLE cables (
    source text COLLATE "C" NOT NULL REFERENCES cable_sets (source) ON DELETE CASCADE,
    -- The source's own key, such as 'way/37016046'.
    local  text COLLATE "C" NOT NULL,
    west   double precision NOT NULL,
    south  double precision NOT NULL,
    east   double precision NOT NULL,
    north  double precision NOT NULL,
    -- The canonical record as JSON, kept as text so that every number
    -- comes back as it went in.
    record text NOT NULL,
    PRIMARY KEY (source, local)
);

-- A map asks for what touches a box. The index finds the candidates; the
-- query compares the edges exactly as well.
CREATE INDEX cables_box_idx
    ON cables USING gist (ST_MakeEnvelope(west, south, east, north, 4326));
