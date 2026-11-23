-- What a pilot's logbook shows beside a flight, as data: the pilot's own
-- country and the class of the glider a flight was flown on (the owner,
-- 2026-11-23: "the flag with the country of the pilot and the country
-- where it took off", "what glider it was made on, what class of glider").
--
-- `accounts.country` is the pilot's own optional setting, an ISO 3166-1
-- alpha-2 code, shown with their name wherever their public flights are
-- shown, and only once they set it. `accounts.glider_class` is the class
-- of the glider the pilot flies now; an upload takes it into the flight.
-- `flights.glider_class` is the class of the glider the flight was flown
-- on: the account's at the upload, or what the pilot set later. The
-- country of a flight's take-off is not stored: the server works it out
-- with every answer, as it does the take-off's site.

ALTER TABLE accounts ADD COLUMN country text CHECK (country ~ '^[A-Z]{2}$');
ALTER TABLE accounts ADD COLUMN glider_class text
    CHECK (glider_class IN ('en_a', 'en_b', 'en_c', 'en_d', 'ccc'));
ALTER TABLE flights ADD COLUMN glider_class text
    CHECK (glider_class IN ('en_a', 'en_b', 'en_c', 'en_d', 'ccc'));
