-- The thermal map: what the server judged of the cells of each model of a
-- forecast source, written down each time it made a model's part whole, so
-- that a restarted server still has the hours of today that the model's
-- newest run does not hold. The map itself is made from these and from
-- what the forecast service answers; it is not stored.

CREATE TABLE thermal_map_parts (
    source     text COLLATE "C" NOT NULL,
    -- The project's name for the model, such as 'icon-d2'.
    model      text COLLATE "C" NOT NULL,
    -- The run of the model the part was last made whole from.
    made_from  timestamptz NOT NULL,
    -- When the server wrote the part down.
    written_at timestamptz NOT NULL,
    -- The judged hours of every cell, in the server's own encoding
    -- (compressed JSON), which the database does not read. Some hundreds
    -- of kilobytes for the first region.
    cells      bytea NOT NULL,
    PRIMARY KEY (source, model)
);
