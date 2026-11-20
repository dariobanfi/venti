-- Groups of pilots, and the plans their members make for a flying day.
--
-- A group is seen by its members and by nobody else. What a pilot puts
-- into one hangs on the row that makes them a member of it, and goes when
-- that row goes: when they leave, when the keeper removes them, when their
-- account is deleted. The database holds every writer to that.

CREATE TABLE groups (
    id          text COLLATE "C" PRIMARY KEY,
    -- Text a pilot typed, cleaned to one line by the server.
    name        text NOT NULL,
    created_at  timestamptz NOT NULL,
    -- The member who keeps the group. That it is a member is held by the
    -- constraint below, once the table of members exists.
    keeper_id   text COLLATE "C" NOT NULL,
    -- The SHA-256 hash of the invite code that lets a pilot in, and NULL
    -- while no code does. The code itself is not stored: whoever reads
    -- this table cannot come into a group with it.
    invite_hash bytea UNIQUE
);

-- The groups an account keeps, for when the account goes.
CREATE INDEX groups_keeper_idx ON groups (keeper_id);

-- One row for each member of a group, the keeper included.
CREATE TABLE group_members (
    group_id   text COLLATE "C" NOT NULL REFERENCES groups (id) ON DELETE CASCADE,
    account_id text COLLATE "C" NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    joined_at  timestamptz NOT NULL,
    PRIMARY KEY (group_id, account_id)
);

-- A group's members in the order in which they came in: the order they
-- are listed in, and the order in which the keeping passes on.
CREATE INDEX group_members_joined_idx ON group_members (group_id, joined_at, account_id);
-- An account's groups in the order in which it came into them; and its
-- rows, for when the account goes.
CREATE INDEX group_members_account_idx ON group_members (account_id, joined_at, group_id);

-- A group has a keeper, and the keeper is a member. Looked at when a
-- transaction ends, so that a group and its first member are stored
-- together, and a keeper who leaves is replaced and removed in one step.
-- A step that would leave a group without such a keeper fails as a whole.
ALTER TABLE groups ADD CONSTRAINT groups_keeper_is_a_member
    FOREIGN KEY (id, keeper_id) REFERENCES group_members (group_id, account_id)
    DEFERRABLE INITIALLY DEFERRED;

-- One row for each plan. A plan belongs to one group and to the member
-- who posted it, and goes when that member is no longer one.
CREATE TABLE group_plans (
    id         text COLLATE "C" PRIMARY KEY,
    group_id   text COLLATE "C" NOT NULL,
    author_id  text COLLATE "C" NOT NULL,
    -- The day as a calendar at the place shows it. No time zone goes with
    -- it: the server does not know where the place is.
    day        date NOT NULL,
    -- The place in the author's words. It is not looked up anywhere and
    -- names no record of a flying site.
    place      text NOT NULL,
    -- Where the place is, if the author marked it: both or neither.
    lat        double precision,
    lon        double precision,
    -- When to meet or to start, as a clock at the place shows it.
    meet_at    time,
    note       text,
    created_at timestamptz NOT NULL,
    changed_at timestamptz NOT NULL,
    FOREIGN KEY (group_id, author_id)
        REFERENCES group_members (group_id, account_id) ON DELETE CASCADE,
    -- For the rows of who is in, which name the plan with its group.
    UNIQUE (id, group_id),
    CHECK ((lat IS NULL) = (lon IS NULL))
);

-- A group's plans from a day on, and how many it holds.
CREATE INDEX group_plans_group_day_idx ON group_plans (group_id, day);
-- The plans of days gone by, for removing them.
CREATE INDEX group_plans_day_idx ON group_plans (day);
-- The plans a member posted, for when the member goes.
CREATE INDEX group_plans_author_idx ON group_plans (group_id, author_id);

-- Who is in on a plan: one row for each member who said so. A row goes
-- with its plan and with the pilot's place in the plan's group.
CREATE TABLE group_plan_pilots (
    plan_id    text COLLATE "C" NOT NULL,
    group_id   text COLLATE "C" NOT NULL,
    account_id text COLLATE "C" NOT NULL,
    -- Since when.
    since      timestamptz NOT NULL,
    PRIMARY KEY (plan_id, account_id),
    FOREIGN KEY (plan_id, group_id)
        REFERENCES group_plans (id, group_id) ON DELETE CASCADE,
    FOREIGN KEY (group_id, account_id)
        REFERENCES group_members (group_id, account_id) ON DELETE CASCADE
);

-- The plans a member is in on, for when the member goes.
CREATE INDEX group_plan_pilots_member_idx ON group_plan_pilots (group_id, account_id);
