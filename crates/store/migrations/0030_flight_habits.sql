-- How each flight was flown, in figures: which way its thermals were
-- circled, how they ended, how it climbed and glided (the counts and
-- sums of `venti_flight::habits`). Worked out from the file with the rest
-- of the analysis (analysis version 2), so that the pilot's own "How you
-- fly" can be said over every flight of the account without reading the
-- files again (`GET /v1/account/habits`).
--
-- Kept as the JSON text the server writes, of the flight engine's field
-- names, as the other records of the store are kept: every number comes
-- back as it went in, and the store knows nothing of the figures. NULL for a flight worked out before; the server's refresh of
-- flights of an older analysis version fills them in.

ALTER TABLE flights ADD COLUMN habits text;
