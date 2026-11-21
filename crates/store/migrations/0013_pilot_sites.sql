-- Flying sites that pilots make: the community's own layer, beside the
-- whole sets of the sources (0005).
--
-- A site of a source is a row that goes when its set is replaced. A site
-- a pilot made is a record of its own, with an identifier of the server's
-- making that is never used for another site, and with every earlier
-- state: a change is a new revision, never an overwrite, putting a site
-- back is a revision that names the one it restores, and removing a site
-- is a revision too. No row of the two tables below is ever deleted, and
-- nothing here goes with anything else: there is no ON DELETE CASCADE.
--
-- Two things about these records are the owner's to decide and are not
-- decided (docs/data-platform.md, "For the owner"):
--
-- - The licence they are given under. The server names it in one place
--   and hands it out with every answer; nothing of it is stored here.
-- - What an account's deletion leaves. As built: the sites and their
--   revisions stay and lose their author (ON DELETE SET NULL), because a
--   site given to everyone cannot go without tearing holes into what
--   others built on it.

-- The count of changes to the layer: one row, which holds the largest
-- change number in use. Every write locks this row first, takes the next
-- number and commits. So the numbers are committed in the order in which
-- they were taken, and whoever asks what changed since a number never
-- misses a lower one that is committed later. The lock also makes one
-- write to the layer wait for the one before: of two pilots who put a
-- site at one place at the same moment, the second finds the first.
CREATE TABLE pilot_site_change (
    only_row boolean PRIMARY KEY DEFAULT true CHECK (only_row),
    latest   bigint NOT NULL
);
INSERT INTO pilot_site_change (only_row, latest) VALUES (true, 0);

-- One row per site, as it stands; for a removed site, as it last stood.
CREATE TABLE pilot_sites (
    -- The text form of a site identifier, 'venti:' and the server's own
    -- key, sorted by bytes as the Rust types sort it.
    id         text COLLATE "C" PRIMARY KEY,
    lon        double precision NOT NULL,
    lat        double precision NOT NULL,
    -- The canonical record as JSON, as the API sends it: the record of
    -- the revision the site stands at. Kept as text and not as jsonb, so
    -- that every number comes back as it went in.
    record     text NOT NULL,
    -- The number of the revision the site stands at. The first is 1.
    revision   integer NOT NULL CHECK (revision >= 1),
    created_at timestamptz NOT NULL,
    -- Who made the site: the author of its first revision. NULL once the
    -- account is gone.
    created_by text COLLATE "C" REFERENCES accounts (id) ON DELETE SET NULL,
    -- When the revision the site stands at was made, and by whom. NULL
    -- for a revision of whoever runs the server, and once the account is
    -- gone.
    changed_at timestamptz NOT NULL,
    changed_by text COLLATE "C" REFERENCES accounts (id) ON DELETE SET NULL,
    -- Since when the site is removed, and NULL while it stands. A removed
    -- site keeps its row and its identifier: the address answers that the
    -- site is gone.
    removed_at timestamptz,
    -- Whether whoever runs the server took the site down. Such a site is
    -- removed, and only whoever runs the server brings it back.
    taken_down boolean NOT NULL DEFAULT false,
    -- The change number the site's latest revision took.
    change     bigint NOT NULL UNIQUE,
    CHECK (NOT taken_down OR removed_at IS NOT NULL)
);

-- A map asks for the standing sites in a box in longitude and latitude,
-- and a write asks which standing sites are near a place. The index finds
-- the candidates; it stores rounded boxes, so the queries compare the
-- coordinates, or measure the distance, exactly as well.
CREATE INDEX pilot_sites_position_idx
    ON pilot_sites USING gist (ST_SetSRID(ST_MakePoint(lon, lat), 4326))
    WHERE removed_at IS NULL;

-- The sites an account made or changed last, for when the account goes.
CREATE INDEX pilot_sites_created_by_idx ON pilot_sites (created_by)
    WHERE created_by IS NOT NULL;
CREATE INDEX pilot_sites_changed_by_idx ON pilot_sites (changed_by)
    WHERE changed_by IS NOT NULL;

-- One row per revision of a site: the whole record as it stood from then
-- on, who made the change, when, through what it came in, and a line
-- saying why. A revision is written once and never changed, but for its
-- author, who is forgotten when the account goes.
CREATE TABLE pilot_site_revisions (
    site_id   text COLLATE "C" NOT NULL REFERENCES pilot_sites (id),
    -- 1 for the revision that made the site, one more for each later one.
    number    integer NOT NULL CHECK (number >= 1),
    -- NULL for a revision of whoever runs the server, and once the
    -- account is gone.
    author_id text COLLATE "C" REFERENCES accounts (id) ON DELETE SET NULL,
    at        timestamptz NOT NULL,
    -- Through what the revision came in: a session of the pilot's own
    -- sign-in, or the command line of whoever runs the server. Tokens
    -- that a pilot gives to another program will be a third.
    via       text NOT NULL CHECK (via IN ('session', 'operator')),
    kind      text NOT NULL CHECK (kind IN ('created', 'changed', 'restored', 'removed')),
    -- For a revision that restored an earlier one: that one's number.
    restores  integer,
    -- Text a pilot typed, cleaned to one line by the server.
    comment   text,
    -- The canonical record as JSON, as the site stood from this revision
    -- on; for a revision that removed the site, as it stood until then.
    record    text NOT NULL,
    PRIMARY KEY (site_id, number),
    CHECK ((kind = 'restored') = (restores IS NOT NULL)),
    CHECK (restores IS NULL OR (restores >= 1 AND restores < number))
);

-- The revisions an account made, for when the account goes.
CREATE INDEX pilot_site_revisions_author_idx ON pilot_site_revisions (author_id)
    WHERE author_id IS NOT NULL;
