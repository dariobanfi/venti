-- The pressure reduced to sea level, for the stations whose provider sends
-- it so (South Tyrol's `LD.RED`). `pressure_hpa` stays the pressure at the
-- station's own height: the two mean different things and are never put
-- into one column. The foehn reading compares pressures at one level and
-- reads this column first (docs/architecture.md, "Foehn").

ALTER TABLE observations ADD COLUMN pressure_sea_level_hpa double precision;
