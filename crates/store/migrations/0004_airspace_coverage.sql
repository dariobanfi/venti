-- Where each stored airspace set is whole: the boxes its run collected for
-- and the countries it fetched. A set stored before this says nothing, and
-- whoever reads "nothing" takes it as "not covered" until the next run.

ALTER TABLE airspace_sets
    -- West, south, east and north of each box in turn, in degrees.
    ADD COLUMN covers    double precision[] NOT NULL DEFAULT '{}',
    -- ISO 3166-1 alpha-2 codes.
    ADD COLUMN countries text[] NOT NULL DEFAULT '{}';
