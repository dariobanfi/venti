-- Everyone's public flights in one list: newest first, the best of a span
-- of days, and those that took off in a part of the map.
--
-- Each index holds the public flights and no others. They are a part of
-- all flights, and a flight that is not public must not be reachable
-- through a list: the queries spell the condition as the indexes do.

-- Newest first, the page after any flight, and the flights that started in
-- a span of time.
CREATE INDEX flights_public_newest_idx ON flights (started_at, id)
    WHERE visibility = 'public';

-- Most points first, and the page after any flight. A flight too short to
-- score is in no such list.
CREATE INDEX flights_public_score_idx ON flights (score_points, id)
    WHERE visibility = 'public' AND score_points IS NOT NULL;

-- Where the glider took off, as a point: the database keeps it in step
-- with the two columns it is made from. A flight whose recording never
-- shows the glider flying has no take-off and no point.
--
-- It is a column and not an expression inside the index, as the position
-- of a site is. A box around one take-off holds a few flights and a box
-- around the Alps holds most, and which index answers best depends on
-- which it is. The planner can tell only for a column: for an expression
-- PostGIS answers a fixed guess of one row in ten thousand whatever the
-- box (seen on 2026-11-20 with PostGIS 3.4.3: a box around all of 15,000
-- flights was taken to hold 2, and every one of them was read and sorted
-- for a page of 50).
--
-- Adding the column writes the table anew. The project had no server when
-- this was written.
ALTER TABLE flights ADD COLUMN takeoff geometry(Point, 4326)
    GENERATED ALWAYS AS (ST_SetSRID(ST_MakePoint(takeoff_lon, takeoff_lat), 4326)) STORED;

-- By where the glider took off, for a box in longitude and latitude. The
-- index finds the candidates; it stores rounded boxes, so the query
-- compares the coordinates exactly as well.
CREATE INDEX flights_public_takeoff_idx ON flights USING gist (takeoff)
    WHERE visibility = 'public';
