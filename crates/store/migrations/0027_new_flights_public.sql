-- Whether a flight the account uploads is public from the start: the
-- pilot's own setting, on until they switch it off (owner decision 21 in
-- docs/plan.md, 2026-11-22: a pilot's flights are public unless the pilot
-- keeps them private). It is on for every account, those that were there
-- before this migration included. An account whose publishing is
-- suspended, and a flight that comes back taken down, still upload
-- private: the store holds the rules, as it does for one flight's change.

ALTER TABLE accounts ADD COLUMN new_flights_public boolean NOT NULL DEFAULT true;
