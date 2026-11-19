-- Forecasts: which run each weather model of a forecast source was at when
-- the server last looked. The forecasts themselves are not stored; they
-- stay with the forecast service, which is asked when a pilot asks.

CREATE TABLE forecast_models (
    source       text COLLATE "C" NOT NULL,
    -- The project's name for the model, such as 'icon-d2'.
    model        text COLLATE "C" NOT NULL,
    -- When the newest run the source held began.
    run          timestamptz NOT NULL,
    -- When the source had that run ready, if it said.
    available_at timestamptz,
    -- When the server looked: the start of the run that found this out.
    -- The same for every model of a source.
    checked_at   timestamptz NOT NULL,
    PRIMARY KEY (source, model)
);
