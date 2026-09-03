-- Add new table for additional templates
CREATE TABLE template (
    id INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL,
    template TEXT NOT NULL,
    instrument INTEGER NOT NULL REFERENCES instrument(id) ON DELETE CASCADE ON UPDATE CASCADE,

    CONSTRAINT duplicate_names UNIQUE (name, instrument) ON CONFLICT REPLACE,

    CONSTRAINT empty_template CHECK (length(template) > 0),
    CONSTRAINT empty_name CHECK (length(name) > 0)
);

CREATE VIEW instrument_template (instrument, name, template) AS
    SELECT
        instrument.name, template.name, template.template
    FROM instrument
        JOIN template
        ON instrument.id = template.instrument;
