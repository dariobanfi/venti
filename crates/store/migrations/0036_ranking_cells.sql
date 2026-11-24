-- The ranking at scale (docs/plan.md, the owner's queue, "The ranking at
-- scale"): over 3.6 million public flights a month's ranking summed its
-- month's flights in 0.6 s and a year's ran into the read timeout. Each
-- pilot's best flights are kept here instead, written by the database in
-- the transaction that changes a flight, whatever changes it.
--
-- Written to run again over itself, as the tests of upgrades run it.
--
-- A row is a cell: one pilot's public flights that score, of one period
-- (`m` a calendar month by UTC, `y` a calendar year by UTC, `a` all
-- time, which starts on 1970-01-01), of one glider class or of every
-- class (''), and of one take-off country or of every country (''). A
-- cell keeps how many such flights there are and the best three, by
-- points and by address among flights with as many, as a ranking orders
-- them; `points` is their sum, which the ranking is ordered by.
--
-- The cells of a class and those of a country are of every country and of
-- every class: a ranking filtered by both is summed from the flights, as
-- one by an area or a kind of route is. A flight without a class or a
-- country is in the cells of every class and of every country only.
--
-- The cells of the account '' are everyone's months: how many public
-- flights that score each month has, of every class and country, of each
-- class and of each country, and no best flights. They answer which
-- months have a ranking without reading every pilot's cell. Every change
-- of a public flight that scores writes its month's row, as it takes a
-- number of the public changes (flight_public_change) already.
--
-- A ranking of a span of several months and years reads its cells and
-- takes the best three of their best threes: the best three of a span
-- always lie among the best three of its parts.
CREATE TABLE IF NOT EXISTS ranking_cells (
    account_id      text COLLATE "C" NOT NULL,
    period          text NOT NULL CHECK (period IN ('m', 'y', 'a')),
    starts          date NOT NULL,
    glider_class    text NOT NULL,
    takeoff_country text NOT NULL,
    scored          integer NOT NULL,
    points          float8 NOT NULL,
    best_ids        text[] COLLATE "C" NOT NULL,
    best_points     float8[] NOT NULL,
    PRIMARY KEY (account_id, period, starts, glider_class, takeoff_country)
);

-- A period's ranking from the top, and the months that have one.
CREATE INDEX IF NOT EXISTS ranking_cells_top_idx
    ON ranking_cells (period, glider_class, takeoff_country, starts, points DESC)
    INCLUDE (account_id, scored);

-- The cells a flight that started at `started_at` counts in.
CREATE OR REPLACE FUNCTION ranking_cells_of(started_at timestamptz, flight_class text, flight_country text)
RETURNS TABLE (period text, starts date, glider_class text, takeoff_country text)
LANGUAGE sql STABLE AS $$
    SELECT p.period, p.starts, g.class, g.country
    FROM (VALUES ('m', date_trunc('month', started_at AT TIME ZONE 'UTC')::date),
                 ('y', date_trunc('year', started_at AT TIME ZONE 'UTC')::date),
                 ('a', DATE '1970-01-01')) AS p (period, starts)
    CROSS JOIN (VALUES ('', ''), (flight_class, ''), ('', flight_country)) AS g (class, country)
    WHERE g.class IS NOT NULL AND g.country IS NOT NULL
$$;

-- The best three of a cell, from the flights as they are now: along the
-- pilot's flights by points (flights_public_pilot_score_idx).
CREATE OR REPLACE FUNCTION ranking_cell_best(
    account text, period text, starts date, glider_class text, takeoff_country text
)
RETURNS TABLE (best_ids text[], best_points float8[], points float8)
LANGUAGE sql STABLE AS $$
    SELECT COALESCE(array_agg(best.id ORDER BY best.score_points DESC, best.id DESC), '{}'),
           COALESCE(array_agg(best.score_points ORDER BY best.score_points DESC, best.id DESC), '{}'),
           COALESCE(sum(best.score_points), 0)
    FROM (SELECT f.id, f.score_points
          FROM flights f
          WHERE f.account_id = account
            AND f.visibility = 'public' AND f.score_points IS NOT NULL
            AND f.started_at >= starts::timestamp AT TIME ZONE 'UTC'
            AND f.started_at < CASE period
                    WHEN 'm' THEN (starts + interval '1 month') AT TIME ZONE 'UTC'
                    WHEN 'y' THEN (starts + interval '1 year') AT TIME ZONE 'UTC'
                    ELSE 'infinity'::timestamptz END
            AND (ranking_cell_best.glider_class = '' OR f.glider_class = ranking_cell_best.glider_class)
            AND (ranking_cell_best.takeoff_country = '' OR f.takeoff_country = ranking_cell_best.takeoff_country)
          ORDER BY f.score_points DESC, f.id DESC
          LIMIT 3) best
$$;

-- Follows a statement that changed flights. `moved` lists the flights
-- that came into a cell (+1) or left one (-1): the counts move by that,
-- and every cell they name takes its best three anew. A cell with no
-- flight left goes. A flight counts while it is public and scores; an
-- update moves it when any of what places it in a cell or among the best
-- changed. The statements run here, in the trigger's own function: the
-- tables of the rows the statement changed are seen there only.
CREATE OR REPLACE FUNCTION ranking_cells_follow() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE
    counted constant text := 'visibility = ''public'' AND score_points IS NOT NULL';
    columns constant text := 'account_id, started_at, glider_class, takeoff_country';
    changed constant text :=
        '(o.visibility, o.score_points, o.started_at, o.glider_class, o.takeoff_country,
          o.account_id)
         IS DISTINCT FROM
         (n.visibility, n.score_points, n.started_at, n.glider_class, n.takeoff_country,
          n.account_id)';
    moved text;
BEGIN
    IF TG_OP = 'INSERT' THEN
        moved := format('SELECT %s, 1 AS delta FROM ranking_new WHERE %s', columns, counted);
    ELSIF TG_OP = 'DELETE' THEN
        moved := format('SELECT %s, -1 AS delta FROM ranking_old WHERE %s', columns, counted);
    ELSE
        moved := format(
            'SELECT o.account_id, o.started_at, o.glider_class, o.takeoff_country, -1 AS delta
             FROM ranking_old o JOIN ranking_new n ON n.id = o.id
             WHERE o.visibility = ''public'' AND o.score_points IS NOT NULL AND %1$s
             UNION ALL
             SELECT n.account_id, n.started_at, n.glider_class, n.takeoff_country, 1 AS delta
             FROM ranking_old o JOIN ranking_new n ON n.id = o.id
             WHERE n.visibility = ''public'' AND n.score_points IS NOT NULL AND %1$s',
            changed);
    END IF;
    EXECUTE format($q$
        INSERT INTO ranking_cells AS kept
            (account_id, period, starts, glider_class, takeoff_country,
             scored, points, best_ids, best_points)
        SELECT c.account_id, c.period, c.starts, c.glider_class, c.takeoff_country,
               c.delta, b.points, b.best_ids, b.best_points
        FROM (SELECT x.account_id, k.period, k.starts, k.glider_class, k.takeoff_country,
                     sum(m.delta)::int AS delta
              FROM (%s) AS m
              CROSS JOIN LATERAL ranking_cells_of(m.started_at, m.glider_class, m.takeoff_country) AS k
              CROSS JOIN LATERAL (VALUES (m.account_id), (CASE WHEN k.period = 'm' THEN '' END))
                  AS x (account_id)
              WHERE x.account_id IS NOT NULL
              GROUP BY 1, 2, 3, 4, 5) AS c
        CROSS JOIN LATERAL ranking_cell_best(
            c.account_id, c.period, c.starts, c.glider_class, c.takeoff_country) AS b
        ON CONFLICT (account_id, period, starts, glider_class, takeoff_country) DO UPDATE
        SET scored = kept.scored + EXCLUDED.scored,
            points = EXCLUDED.points,
            best_ids = EXCLUDED.best_ids,
            best_points = EXCLUDED.best_points
    $q$, moved);
    EXECUTE format(
        'DELETE FROM ranking_cells WHERE scored <= 0
         AND (account_id = '''' OR account_id IN (SELECT DISTINCT m.account_id FROM (%s) AS m))',
        moved);
    RETURN NULL;
END
$$;

DROP TRIGGER IF EXISTS flights_ranking_insert ON flights;
CREATE TRIGGER flights_ranking_insert AFTER INSERT ON flights
    REFERENCING NEW TABLE AS ranking_new
    FOR EACH STATEMENT EXECUTE FUNCTION ranking_cells_follow();
DROP TRIGGER IF EXISTS flights_ranking_update ON flights;
CREATE TRIGGER flights_ranking_update AFTER UPDATE ON flights
    REFERENCING OLD TABLE AS ranking_old NEW TABLE AS ranking_new
    FOR EACH STATEMENT EXECUTE FUNCTION ranking_cells_follow();
DROP TRIGGER IF EXISTS flights_ranking_delete ON flights;
CREATE TRIGGER flights_ranking_delete AFTER DELETE ON flights
    REFERENCING OLD TABLE AS ranking_old
    FOR EACH STATEMENT EXECUTE FUNCTION ranking_cells_follow();

-- Every cell from the flights there are, in one pass: for the flights from
-- before this migration, and for a database filled in bulk with the
-- triggers off (`crates/store/tests/scale/seed.sql`).
CREATE OR REPLACE FUNCTION ranking_cells_rebuild() RETURNS void LANGUAGE sql AS $$
    DELETE FROM ranking_cells;
    INSERT INTO ranking_cells
        (account_id, period, starts, glider_class, takeoff_country,
         scored, points, best_ids, best_points)
    SELECT account_id, period, starts, glider_class, takeoff_country,
           count(*)::int,
           COALESCE(sum(score_points) FILTER (WHERE place <= 3), 0),
           array_agg(id ORDER BY place) FILTER (WHERE place <= 3),
           array_agg(score_points ORDER BY place) FILTER (WHERE place <= 3)
    FROM (SELECT f.account_id, k.period, k.starts, k.glider_class, k.takeoff_country,
                 f.id, f.score_points,
                 row_number() OVER (
                     PARTITION BY f.account_id, k.period, k.starts, k.glider_class, k.takeoff_country
                     ORDER BY f.score_points DESC, f.id DESC) AS place
          FROM flights f
          CROSS JOIN LATERAL ranking_cells_of(f.started_at, f.glider_class, f.takeoff_country) AS k
          WHERE f.visibility = 'public' AND f.score_points IS NOT NULL) AS placed
    GROUP BY account_id, period, starts, glider_class, takeoff_country;
    INSERT INTO ranking_cells
        (account_id, period, starts, glider_class, takeoff_country,
         scored, points, best_ids, best_points)
    SELECT '', 'm', starts, glider_class, takeoff_country, sum(scored)::int, 0, '{}', '{}'
    FROM ranking_cells
    WHERE period = 'm'
    GROUP BY starts, glider_class, takeoff_country;
$$;

SELECT ranking_cells_rebuild();
