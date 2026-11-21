-- Pilots among each other: who follows whom, the cheers and the comments
-- on public flights, and what a pilot was told of them.
--
-- Who may read (docs/data-platform.md, principle 5): that an account
-- follows another, that an account cheers a flight and what an account
-- wrote under a flight are public while they stand, and a cheer and a
-- comment are shown only while their flight is public
-- (flights.visibility = 'public'). The notices are private to their
-- account.
--
-- A follow, a cheer and a comment are their account's and go with it; a
-- cheer and a comment are on a flight and go with it (ON DELETE CASCADE
-- both ways). A flight that is made private keeps them, unseen.

CREATE TABLE follows (
    follower_id  text COLLATE "C" NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    followed_id  text COLLATE "C" NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    followed_at  timestamptz NOT NULL,
    PRIMARY KEY (follower_id, followed_id),
    CONSTRAINT follows_another CHECK (follower_id <> followed_id)
);

-- Who follows an account, the newest first.
CREATE INDEX follows_followed_idx ON follows (followed_id, followed_at DESC, follower_id DESC);
-- Whom an account follows, the newest first.
CREATE INDEX follows_follower_idx ON follows (follower_id, followed_at DESC, followed_id DESC);

CREATE TABLE flight_cheers (
    flight_id   text COLLATE "C" NOT NULL REFERENCES flights (id) ON DELETE CASCADE,
    account_id  text COLLATE "C" NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    cheered_at  timestamptz NOT NULL,
    PRIMARY KEY (flight_id, account_id)
);

-- Who cheered a flight, the newest first.
CREATE INDEX flight_cheers_flight_idx ON flight_cheers (flight_id, cheered_at DESC, account_id DESC);
-- What an account cheered, for its archive.
CREATE INDEX flight_cheers_account_idx ON flight_cheers (account_id, cheered_at);

CREATE TABLE flight_comments (
    id          text COLLATE "C" PRIMARY KEY,
    flight_id   text COLLATE "C" NOT NULL REFERENCES flights (id) ON DELETE CASCADE,
    author_id   text COLLATE "C" NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    -- Plain text as the server checked it: at most 500 characters.
    body        text NOT NULL,
    created_at  timestamptz NOT NULL
);

-- A flight's comments, the oldest first.
CREATE INDEX flight_comments_flight_idx ON flight_comments (flight_id, created_at, id);
-- What an account wrote, for its archive.
CREATE INDEX flight_comments_author_idx ON flight_comments (author_id, created_at, id);

-- The flights whose pilot takes no more comments. A row is the setting;
-- no row is the usual case. What was written before stays and is shown.
CREATE TABLE flight_comments_closed (
    flight_id  text COLLATE "C" PRIMARY KEY REFERENCES flights (id) ON DELETE CASCADE,
    closed_at  timestamptz NOT NULL
);

-- What an account was told: somebody follows it, cheered a flight of
-- its, commented on one. Kept for a while, for the pilot's apps to
-- fetch. A row goes with the account it is for and with the account
-- that caused it.
CREATE TABLE notices (
    account_id  text COLLATE "C" NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    -- Per account; only grows. The cursor of GET /v1/account/notifications.
    number      bigint NOT NULL,
    actor_id    text COLLATE "C" NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    at          timestamptz NOT NULL,
    -- What, as JSON text. The server writes it and reads it.
    record      text NOT NULL,
    PRIMARY KEY (account_id, number)
);

-- The old ones, to remove them.
CREATE INDEX notices_at_idx ON notices (at);
-- Those an account caused, which go with it.
CREATE INDEX notices_actor_idx ON notices (actor_id);

-- The largest number an account's notices have used. The row is also
-- what a write locks, so that two notices of one account take two
-- numbers.
CREATE TABLE notice_numbers (
    account_id  text COLLATE "C" PRIMARY KEY REFERENCES accounts (id) ON DELETE CASCADE,
    latest      bigint NOT NULL
);

-- Reports about a comment. Like a plan's words, a comment can be gone a
-- moment after it was reported, so the report keeps the words as they
-- stood, and the flight they were written under. about_account_id is
-- the comment's author.
ALTER TABLE reports DROP CONSTRAINT reports_kind_check;
ALTER TABLE reports ADD CONSTRAINT reports_kind_check
    CHECK (kind IN ('flight', 'pilot', 'live_track', 'group', 'plan', 'site', 'comment'));

ALTER TABLE reports ADD COLUMN flight_id text COLLATE "C";
ALTER TABLE reports ADD COLUMN reported_comment text;

ALTER TABLE reports ADD CONSTRAINT reports_comment_keeps_its_words
    CHECK ((kind = 'comment') = (reported_comment IS NOT NULL)
       AND (kind = 'comment') = (flight_id IS NOT NULL));
