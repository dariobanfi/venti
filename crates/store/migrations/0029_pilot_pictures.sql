-- A pilot's picture, and whether a pilot is listed among the pilots near a
-- place (docs/plan.md, "After the owner's testing session of 2026-11-22",
-- items 7 and 10).
--
-- One picture per account, the server's own copy: a square WebP the server
-- made itself from what the pilot uploaded or from the picture of the
-- Google account the pilot signs in with, decoded and encoded again, with
-- nothing of the original's metadata. It is kept in the database as a
-- flight's file is. Its identifier is new for every picture, so that an
-- address under /v1/pictures names one picture for good and can be kept
-- by every cache; a picture that is replaced or removed takes its
-- address with it.
CREATE TABLE pictures (
    id         text COLLATE "C" PRIMARY KEY,
    account_id text COLLATE "C" NOT NULL UNIQUE REFERENCES accounts (id) ON DELETE CASCADE,
    bytes      bytea NOT NULL,
    -- 'upload': the pilot's own; 'google': taken from Google at a sign-in.
    origin     text NOT NULL CHECK (origin IN ('upload', 'google')),
    created_at timestamptz NOT NULL
);

-- Whether the server has tried to take the picture of the account's Google
-- account. It does so once, at the first sign-in with Google that finds
-- the account without a picture: a pilot who removes it does not get it
-- back at the next sign-in.
ALTER TABLE accounts ADD COLUMN google_picture_tried boolean NOT NULL DEFAULT false;

-- Whether the account is among the pilots near a place
-- (`GET /v1/pilots/near`), which is made of public flights only. On until
-- its pilot switches it off, as for every account before this migration.
ALTER TABLE accounts ADD COLUMN shown_near_places boolean NOT NULL DEFAULT true;

-- A picture can be reported, as a pilot's page can. The report names the
-- picture it was about; the operator removes the picture with a command.
ALTER TABLE reports DROP CONSTRAINT reports_kind_check;
ALTER TABLE reports ADD CONSTRAINT reports_kind_check
    CHECK (kind IN ('flight', 'pilot', 'live_track', 'group', 'plan', 'site', 'comment',
                    'picture'));
