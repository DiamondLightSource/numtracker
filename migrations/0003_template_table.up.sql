-- Add new table for additional templates
CREATE TABLE template (
    id INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL,
    template TEXT NOT NULL,
    beamline INTEGER NOT NULL REFERENCES beamline(id) ON DELETE CASCADE ON UPDATE CASCADE,

    CONSTRAINT duplicate_names UNIQUE (name, beamline) ON CONFLICT REPLACE,

    CONSTRAINT empty_template CHECK (length(template) > 0),
    CONSTRAINT empty_name CHECK (length(name) > 0)
);

CREATE VIEW beamline_template (beamline, name, template) AS
    SELECT
        beamline.name, template.name, template.template
    FROM beamline
        JOIN template
        ON beamline.id = template.beamline;
