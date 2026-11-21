-- Reports about a group, and about a plan of a group.
--
-- A flight, a page and a live track are named by a report and looked at
-- where they are. A group's name and a plan's place and note can be
-- changed or be gone a moment after they were reported, so a report about
-- either keeps those words as they stood, beside the reason and the
-- reporter's own words and for exactly as long: until the report is
-- removed. Nothing else of a group or a plan is copied here: no member,
-- no day, no time to meet, no position and nobody who was in.

ALTER TABLE reports DROP CONSTRAINT reports_kind_check;
ALTER TABLE reports ADD CONSTRAINT reports_kind_check
    CHECK (kind IN ('flight', 'pilot', 'live_track', 'group', 'plan'));

-- For a plan: the group it was posted to. Like subject_id it refers to
-- nothing: a report outlives what it is about, as it outlives a flight
-- that was deleted.
ALTER TABLE reports ADD COLUMN group_id text COLLATE "C";
-- For a group: its name when it was reported.
ALTER TABLE reports ADD COLUMN reported_name text;
-- For a plan: its place when it was reported, and its note if it had one.
ALTER TABLE reports ADD COLUMN reported_place text;
ALTER TABLE reports ADD COLUMN reported_note text;

ALTER TABLE reports ADD CONSTRAINT reports_group_keeps_its_name
    CHECK ((kind = 'group') = (reported_name IS NOT NULL));
ALTER TABLE reports ADD CONSTRAINT reports_plan_keeps_its_words
    CHECK ((kind = 'plan') = (reported_place IS NOT NULL)
       AND (kind = 'plan') = (group_id IS NOT NULL)
       AND (kind = 'plan' OR reported_note IS NULL));

-- about_account_id of a report about a group is the account that kept the
-- group when it was reported. A group passes from keeper to keeper and is
-- the same group all the while: one open report per reporter and group,
-- whoever keeps it.
CREATE UNIQUE INDEX reports_open_once_group_idx
    ON reports (reporter_id, subject_id)
    WHERE closed_at IS NULL AND kind = 'group';
