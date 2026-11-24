-- Groups as communities with a chat (owner decision 26 of docs/plan.md).
--
-- A group is public, which anyone signed in finds, reads and joins, or
-- private, which its members read and whose owner and admins invite
-- pilots by handle. The owner is the account groups.keeper_id names, as
-- before; admins are members with a mark of their own. A group has a
-- short description, a picture and a home area, by which the groups near
-- a pilot are found. Each group is a chat: messages of text, pictures and
-- links to Venti's records, and the groups' plans as posts in it.
--
-- Messages are conversation, not data (decision 26): no public read of
-- the API, no dump and no tool of the connector hands them out.

ALTER TABLE groups
    ADD COLUMN public boolean NOT NULL DEFAULT false,
    -- Text a pilot typed, cleaned by the server; NULL for none.
    ADD COLUMN description text,
    -- The group's own picture, a row of group_pictures; NULL for none.
    ADD COLUMN picture_id text COLLATE "C",
    -- The home area: a point and a radius, and the place's name in the
    -- owner's words. The point and the radius are both there or neither;
    -- groups made before this migration have none.
    ADD COLUMN home_lat double precision,
    ADD COLUMN home_lon double precision,
    ADD COLUMN home_radius_km double precision,
    ADD COLUMN home_place text,
    ADD CONSTRAINT groups_home_is_whole
        CHECK ((home_lat IS NULL) = (home_lon IS NULL)
           AND (home_lat IS NULL) = (home_radius_km IS NULL)
           AND (home_lat IS NOT NULL OR home_place IS NULL));

-- The public groups, which are found by place and by name.
CREATE INDEX groups_public_home_idx ON groups (home_lat, home_lon) WHERE public;

-- What each member is in a group beyond being in it: an admin or not,
-- whether the group's messages are pushed to their phones, and up to
-- which message (group_messages.number) they read it.
ALTER TABLE group_members
    ADD COLUMN admin boolean NOT NULL DEFAULT false,
    ADD COLUMN muted boolean NOT NULL DEFAULT false,
    ADD COLUMN read_up_to bigint NOT NULL DEFAULT 0;

-- A pilot asked into a private group by its owner or an admin, until
-- they come in or the invite is withdrawn or declined. It goes with the
-- group, with the pilot's account and with the account of who asked.
CREATE TABLE group_invites (
    group_id   text COLLATE "C" NOT NULL REFERENCES groups (id) ON DELETE CASCADE,
    account_id text COLLATE "C" NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    invited_by text COLLATE "C" NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    invited_at timestamptz NOT NULL,
    PRIMARY KEY (group_id, account_id)
);

CREATE INDEX group_invites_account_idx ON group_invites (account_id, invited_at);
CREATE INDEX group_invites_by_idx ON group_invites (invited_by);

-- A picture sent to a group: for a message, or the group's own. The
-- server's own copy, a WebP it wrote anew without the original's
-- metadata. It goes with the group and with the account that sent it.
CREATE TABLE group_pictures (
    id         text COLLATE "C" PRIMARY KEY,
    group_id   text COLLATE "C" NOT NULL REFERENCES groups (id) ON DELETE CASCADE,
    account_id text COLLATE "C" NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    bytes      bytea NOT NULL,
    width      integer NOT NULL CHECK (width > 0),
    height     integer NOT NULL CHECK (height > 0),
    created_at timestamptz NOT NULL
);

CREATE INDEX group_pictures_group_idx ON group_pictures (group_id);
CREATE INDEX group_pictures_account_idx ON group_pictures (account_id, created_at);

ALTER TABLE groups ADD CONSTRAINT groups_picture_is_kept
    FOREIGN KEY (picture_id) REFERENCES group_pictures (id) ON DELETE SET NULL;

-- The numbers of messages: one sequence for every group, so that a
-- number is a message's place in its group and a version says what
-- changed since a cursor.
CREATE SEQUENCE group_message_numbers;

-- One row for each message. `number` is its place in the chat and never
-- changes; `version` is a number of the same sequence, new at each change
-- (posted, removed, a plan's card changed), which "what is new since"
-- reads. A removed message keeps its row and loses its words, pictures
-- and links.
CREATE TABLE group_messages (
    id         text COLLATE "C" PRIMARY KEY,
    group_id   text COLLATE "C" NOT NULL REFERENCES groups (id) ON DELETE CASCADE,
    number     bigint NOT NULL UNIQUE,
    version    bigint NOT NULL UNIQUE,
    -- A message goes with its author's account, and stays when the
    -- author leaves the group.
    author_id  text COLLATE "C" NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    kind       text NOT NULL CHECK (kind IN ('text', 'plan')),
    -- Text a pilot typed, cleaned by the server; empty for none.
    body       text NOT NULL DEFAULT '',
    -- The identifiers of its pictures, rows of group_pictures.
    pictures   text[] NOT NULL DEFAULT '{}',
    -- Links to Venti's records, each `kind:identifier`.
    links      text[] NOT NULL DEFAULT '{}',
    -- The plan a message of kind plan carries; NULL once the plan is gone.
    plan_id    text COLLATE "C" REFERENCES group_plans (id) ON DELETE SET NULL,
    created_at timestamptz NOT NULL,
    removed_at timestamptz,
    -- Who removed it: its author, an admin or the owner, or whoever runs
    -- the server.
    removed_by text CHECK (removed_by IN ('author', 'admin', 'operator')),
    CHECK ((removed_at IS NULL) = (removed_by IS NULL))
);

-- A group's messages by place, for pages; by version, for what is new.
CREATE INDEX group_messages_group_number_idx ON group_messages (group_id, number);
CREATE INDEX group_messages_group_version_idx ON group_messages (group_id, version);
-- An author's messages, for when the account goes; a plan's post.
CREATE INDEX group_messages_author_idx ON group_messages (author_id);
CREATE INDEX group_messages_plan_idx ON group_messages (plan_id) WHERE plan_id IS NOT NULL;

-- The groups there are stay private, with their members and their keeper
-- as owner; every plan they hold becomes a post of kind plan in its
-- group's chat, in the order the plans were posted.
INSERT INTO group_messages (id, group_id, number, version, author_id, kind, plan_id, created_at)
SELECT p.id, p.group_id, p.n, p.n, p.author_id, 'plan', p.id, p.created_at
FROM (SELECT id, group_id, author_id, created_at,
             row_number() OVER (ORDER BY created_at, id) AS n
      FROM group_plans) AS p;

SELECT setval('group_message_numbers',
              COALESCE((SELECT max(number) FROM group_messages), 0) + 1, false);

-- What was there before the chat counts as read: a member is not told of
-- the plans they saw on the board already.
UPDATE group_members m
SET read_up_to = COALESCE((SELECT max(number) FROM group_messages g
                           WHERE g.group_id = m.group_id), 0);

-- A message and a picture of a group can be reported. The report keeps
-- the group's identifier and the message's words as they stood.
ALTER TABLE reports DROP CONSTRAINT reports_kind_check;
ALTER TABLE reports ADD CONSTRAINT reports_kind_check
    CHECK (kind IN ('flight', 'pilot', 'live_track', 'group', 'plan', 'site', 'comment',
                    'picture', 'open_plan', 'help_request', 'group_message',
                    'group_picture'));
ALTER TABLE reports DROP CONSTRAINT reports_plan_keeps_its_words;
ALTER TABLE reports ADD CONSTRAINT reports_plan_keeps_its_words
    CHECK ((kind IN ('plan', 'open_plan')) = (reported_place IS NOT NULL)
       AND (kind IN ('plan', 'group_message', 'group_picture')) = (group_id IS NOT NULL)
       AND (kind IN ('plan', 'open_plan', 'help_request') OR reported_note IS NULL));
ALTER TABLE reports DROP CONSTRAINT reports_comment_keeps_its_words;
ALTER TABLE reports ADD CONSTRAINT reports_comment_keeps_its_words
    CHECK ((kind IN ('comment', 'group_message')) = (reported_comment IS NOT NULL)
       AND (kind = 'comment') = (flight_id IS NOT NULL));
