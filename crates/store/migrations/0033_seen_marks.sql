-- Up to where the pilot saw what the account was told and what its
-- alarms fired, the same on every device: the mark behind
-- GET /v1/account/unread. One row per account, the numbers of the two
-- lists (notices.number, alerts_fired.number) it saw up to. No row: the
-- pilot saw nothing yet. It is the account's own and goes with it.

CREATE TABLE seen_marks (
    account_id    text COLLATE "C" PRIMARY KEY REFERENCES accounts (id) ON DELETE CASCADE,
    notices_seen  bigint NOT NULL DEFAULT 0,
    fired_seen    bigint NOT NULL DEFAULT 0,
    -- When the mark last moved.
    seen_at       timestamptz NOT NULL
);

-- The accounts there are begin with everything up to now seen: until now
-- each device kept what it had shown, and a pilot is not told again of
-- what came before the mark existed.
INSERT INTO seen_marks (account_id, notices_seen, fired_seen, seen_at)
SELECT accounts.id,
       COALESCE(notice_numbers.latest, 0),
       COALESCE(alerts_fired_numbers.latest, 0),
       now()
FROM accounts
LEFT JOIN notice_numbers ON notice_numbers.account_id = accounts.id
LEFT JOIN alerts_fired_numbers ON alerts_fired_numbers.account_id = accounts.id
WHERE notice_numbers.latest IS NOT NULL OR alerts_fired_numbers.latest IS NOT NULL;
