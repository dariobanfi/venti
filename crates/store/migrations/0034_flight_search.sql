-- Everyone's flights searched as XContest searches them, at the size of a
-- busy year (docs/plan.md, the owner's queue, "Flights filtered by area
-- and more" and "Flight search that holds at scale").
--
-- The country a flight took off in becomes a column. Until here it was
-- worked out with every answer only (0028), which a filter cannot ask by:
-- the server writes it with what it works out from the file, by the same
-- outlines it names the country with in an answer, and the refresh of an
-- older analysis fills it in for the flights from before.
ALTER TABLE flights ADD COLUMN IF NOT EXISTS takeoff_country text CHECK (takeoff_country ~ '^[A-Z]{2}$');

-- The indexes of everyone's flights, measured over 3.6 million made-up
-- public flights (`crates/store/tests/scale.rs`, 2026-11-24).
--
-- A page is found in two steps: the addresses of its flights first, from
-- an index alone, then their rows. The two indexes that order the list
-- therefore carry every column a filter reads, so that a filter that
-- leaves out most flights is tested on the index and not on a row of a
-- kilobyte read from the table for each flight it leaves out. They take
-- the place of the two of 0008, which carried none.
CREATE INDEX IF NOT EXISTS flights_public_newest_cover_idx ON flights (started_at, id)
    INCLUDE (account_id, score_points, score_distance_m, score_kind, glider_class,
             takeoff_country, takeoff_lat, takeoff_lon, takeoff)
    WHERE visibility = 'public';
CREATE INDEX IF NOT EXISTS flights_public_score_cover_idx ON flights (score_points, id)
    INCLUDE (account_id, started_at, score_distance_m, score_kind, glider_class,
             takeoff_country, takeoff_lat, takeoff_lon, takeoff)
    WHERE visibility = 'public' AND score_points IS NOT NULL;
DROP INDEX IF EXISTS flights_public_newest_idx;
DROP INDEX IF EXISTS flights_public_score_idx;

-- A pilot's best flights: their page sorted by points, and the ranking.
CREATE INDEX IF NOT EXISTS flights_public_pilot_score_idx ON flights (account_id, score_points, id)
    INCLUDE (started_at)
    WHERE visibility = 'public' AND score_points IS NOT NULL;

-- A country with few flights would otherwise be looked for along the
-- whole list.
CREATE INDEX IF NOT EXISTS flights_public_country_newest_idx ON flights (takeoff_country, started_at, id)
    WHERE visibility = 'public' AND takeoff_country IS NOT NULL;
CREATE INDEX IF NOT EXISTS flights_public_country_score_idx ON flights (takeoff_country, score_points, id)
    WHERE visibility = 'public' AND takeoff_country IS NOT NULL AND score_points IS NOT NULL;

-- The long flights, which are few: "at least 300 km" without them reads
-- every public flight to find none.
CREATE INDEX IF NOT EXISTS flights_public_distance_idx ON flights (score_distance_m)
    WHERE visibility = 'public' AND score_distance_m IS NOT NULL;
