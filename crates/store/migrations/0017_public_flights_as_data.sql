-- Public flights as data: the moment a flight became public, one count of
-- changes across all accounts for what the public can see of flights, and
-- a record of every flight that is public no more.
--
-- Until here a flight had a change number of its account only (0002), so
-- that nobody but its pilot could ask what changed, and nothing said when
-- a flight became public or that it had stopped being so. Whoever copied
-- the public flights kept the ones their pilots withdrew
-- (docs/data-platform.md, "What in today's server works against it", 3).

-- The count of changes to what the public sees of flights: one row, which
-- holds the largest number in use. Whatever makes a flight public, changes
-- a public flight or takes one from the public locks this row first, takes
-- the next number and commits. So the numbers are committed in the order
-- in which they were taken, and whoever asks what changed since a number
-- never misses a lower one that is committed later. The row is locked
-- after the account's row, by every writer, so no two wait for each other.
CREATE TABLE flight_public_change (
    only_row boolean PRIMARY KEY DEFAULT true CHECK (only_row),
    latest   bigint NOT NULL
);

ALTER TABLE flights
    -- Since when the flight is public, without a break; NULL while it is
    -- not. A flight that is withdrawn and made public again gets the
    -- moment of the second time: the terms a flight is read under are
    -- those of the day its pilot made it public.
    ADD COLUMN published_at  timestamptz,
    -- The number of the last change to what the public sees of the
    -- flight; NULL while it is not public.
    ADD COLUMN public_change bigint;

-- The flights that are public today. The table never kept when a flight's
-- visibility changed, so the moment it became public is not known. What
-- it has is `uploaded_at`, and a flight cannot have been public before
-- that: every such flight gets it as its `published_at`, which is
-- therefore the earliest it can have been and not the moment it was.
-- Their numbers follow the clock, as every later one does, in the order
-- of their upload.
WITH numbered AS (
    SELECT id,
           (extract(epoch FROM now()) * 1000000)::bigint
               + row_number() OVER (ORDER BY uploaded_at, id) AS number
    FROM flights
    WHERE visibility = 'public'
)
UPDATE flights f
SET published_at = f.uploaded_at, public_change = numbered.number
FROM numbered
WHERE f.id = numbered.id;

INSERT INTO flight_public_change (only_row, latest)
SELECT true, COALESCE(max(public_change), 0) FROM flights;

-- Held by the database: a public flight has its moment and its number,
-- and a flight that is not public has neither. A write that changes the
-- visibility and forgets the two is refused.
ALTER TABLE flights
    ADD CONSTRAINT flights_public_has_its_moment
        CHECK ((visibility = 'public') = (published_at IS NOT NULL)),
    ADD CONSTRAINT flights_public_has_its_number
        CHECK ((visibility = 'public') = (public_change IS NOT NULL));

-- "What changed since", of all public flights and of one pilot's.
CREATE UNIQUE INDEX flights_public_change_idx ON flights (public_change)
    WHERE visibility = 'public';
CREATE INDEX flights_public_change_of_account_idx ON flights (account_id, public_change)
    WHERE visibility = 'public';

-- One row per flight that was public and is not: made private or
-- unlisted by its pilot, taken down, unlisted with its account's
-- publishing, deleted, or gone with its account. Nothing of the flight is
-- here but its address. A flight that is made public again loses its row.
--
-- The rows outlive the flight and the account, so that whoever copied the
-- public flights learns of a withdrawal however long ago it was: nothing
-- removes a row but the flight's return.
CREATE TABLE flight_withdrawals (
    flight_id    text COLLATE "C" PRIMARY KEY,
    -- Whose flight it was, so that the withdrawals of one pilot can be
    -- asked for. NULL once the account is gone.
    account_id   text COLLATE "C" REFERENCES accounts (id) ON DELETE SET NULL,
    -- The number the withdrawal took in the count above.
    change       bigint NOT NULL UNIQUE,
    -- When. NULL for a flight that went with its account: the store is
    -- not told when an account goes.
    withdrawn_at timestamptz
);

CREATE INDEX flight_withdrawals_of_account_idx ON flight_withdrawals (account_id, change)
    WHERE account_id IS NOT NULL;
