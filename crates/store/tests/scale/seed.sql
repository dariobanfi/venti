-- Made-up pilots and public flights in bulk, for measuring how the queries
-- of everyone's flights hold at the size of a year of a busy Venti
-- (docs/plan.md, the owner's queue, "Flight search that holds at scale").
--
-- `seed_flights(pilots, flights, private_flights)` adds that many accounts
-- and flights to the tables of the schema it is called in, made in the
-- database itself with generate_series: no row crosses the connection,
-- which is quicker than COPY. Nothing in a flight is a recording: the file
-- is empty, the outline and the profile too. What the queries read is
-- realistic: a year of starts that follow the seasons and the daylight,
-- take-offs clustered at a few hundred sites, most of them in the Alps,
-- some elsewhere in Europe and in the rest of the world, a few pilots who
-- fly a lot and many who fly little, the kinds of route, the distances
-- and the points as an online contest has them, and the glider classes.
--
-- The randomness is seeded, so two runs make the same rows.

CREATE OR REPLACE FUNCTION seed_flights(pilots int, flights int, private_flights int)
RETURNS void LANGUAGE plpgsql AS $seed$
DECLARE
    sites constant int := 400;
    year_end constant timestamptz := '2026-11-23T00:00:00Z';
BEGIN
    PERFORM setseed(0.4217);

    CREATE TEMPORARY TABLE seed_sites ON COMMIT DROP AS
    SELECT n AS site,
           CASE WHEN r < 0.7 THEN 45.6 + random() * 2.2
                WHEN r < 0.85 THEN 37.0 + random() * 13.0
                ELSE -35.0 + random() * 70.0 END AS lat,
           CASE WHEN r < 0.7 THEN 6.0 + random() * 8.5
                WHEN r < 0.85 THEN -6.0 + random() * 30.0
                ELSE -120.0 + random() * 240.0 END AS lon,
           CASE WHEN r < 0.7 THEN (ARRAY['AT','AT','CH','CH','FR','FR','IT','IT','DE','SI'])[1 + floor(random() * 10)::int]
                WHEN r < 0.85 THEN (ARRAY['ES','ES','GR','PT','HR','PL','CZ','GB','NO','BG'])[1 + floor(random() * 10)::int]
                ELSE NULL END AS country,
           800 + floor(random() * 1800)::int AS altitude_m
    FROM (SELECT n, random() AS r FROM generate_series(1, sites) n) s;

    INSERT INTO accounts (id, handle, display_name, created_at, country, glider_class)
    SELECT 'seedacc' || lpad(n::text, 8, '0'),
           'seed' || n,
           'Seed Pilot ' || n,
           year_end - interval '400 days',
           (ARRAY['AT','CH','DE','FR','IT', NULL])[1 + floor(random() * 6)::int],
           (ARRAY['en_a','en_b','en_b','en_c','en_d','ccc'])[1 + floor(random() * 6)::int]
    FROM generate_series(1, pilots) n;

    -- The ranking's cells are made in one pass after the flights, which is
    -- quicker than following each insert (0036).
    ALTER TABLE flights DISABLE TRIGGER flights_ranking_insert;
    INSERT INTO flights (
        id, account_id, identity, visibility, uploaded_at, change, file, file_bytes,
        analysis_version, started_at, airtime_s,
        takeoff_at, takeoff_lat, takeoff_lon, takeoff_altitude_m,
        landing_at, landing_lat, landing_lon, landing_altitude_m,
        track_length_m, max_altitude_m, max_climb_ms,
        score_kind, score_closed, score_distance_m, score_points,
        glider, outline, profile, published_at, public_change,
        glider_class, takeoff_country)
    SELECT id, account_id, id, visibility, started_at + interval '3 hours', n, ''::bytea, 0,
           2, started_at, airtime_s,
           started_at, lat, lon, altitude_m,
           started_at + airtime_s * interval '1 second', lat - 0.02, lon + 0.03, 600,
           distance_m * 1.4 + 1000, altitude_m + 1200, 2.5 + random() * 3,
           kind, kind <> 'free_flight', CASE WHEN kind IS NULL THEN NULL ELSE distance_m END,
           CASE WHEN kind IS NULL THEN NULL
                ELSE round((distance_m / 1000 * CASE kind WHEN 'free_flight' THEN 1.0
                                                         WHEN 'free_triangle' THEN 1.2
                                                         ELSE 1.4 END)::numeric, 2)::float8 END,
           'Seed Glider', '{}', '{}',
           CASE WHEN visibility = 'public' THEN started_at + interval '3 hours' END,
           CASE WHEN visibility = 'public' THEN n END,
           glider_class, country
    FROM (
        SELECT n,
               substr(md5('seed flight ' || n), 1, 26) AS id,
               'seedacc' || lpad((1 + floor(power(random(), 2.5) * pilots))::int::text, 8, '0') AS account_id,
               CASE WHEN n > flights THEN 'private' ELSE 'public' END AS visibility,
               -- A day of the year, more of them in summer, a start between
               -- nine and four.
               date_trunc('day', year_end - interval '365 days'
                   + (182 + (random() + random() + random() - 1.5) * 150) * interval '1 day')
                   + (9 * 3600 + floor(random() * 7 * 3600)) * interval '1 second' AS started_at,
               (600 + floor(power(random(), 1.5) * 25000))::bigint AS airtime_s,
               s.lat + (random() - 0.5) * 0.01 AS lat,
               s.lon + (random() - 0.5) * 0.01 AS lon,
               s.altitude_m, s.country,
               CASE WHEN k < 0.12 THEN NULL
                    WHEN k < 0.70 THEN 'free_flight'
                    WHEN k < 0.90 THEN 'free_triangle'
                    ELSE 'fai_triangle' END AS kind,
               1000 + floor(exp(ln(15000) + (random() + random() + random() - 1.5) * 1.6))::float8 AS distance_m,
               (ARRAY['en_a','en_b','en_b','en_b','en_c','en_c','en_d','ccc', NULL])[1 + floor(random() * 9)::int] AS glider_class
        FROM (SELECT n, random() AS k,
                     1 + floor(power(random(), 2.2) * sites)::int AS site
              FROM generate_series(1, flights + private_flights) n) picked
        JOIN seed_sites s ON s.site = picked.site
    ) made;

    ALTER TABLE flights ENABLE TRIGGER flights_ranking_insert;
    PERFORM ranking_cells_rebuild();

    UPDATE accounts a
    SET flight_change = counted.latest
    FROM (SELECT account_id, max(change) AS latest FROM flights GROUP BY account_id) counted
    WHERE a.id = counted.account_id;
    UPDATE flight_public_change SET latest = (SELECT COALESCE(max(public_change), 0) FROM flights);
END
$seed$;
