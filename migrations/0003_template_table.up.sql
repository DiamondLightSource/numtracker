-- Add new table for additional templates
CREATE TABLE templates (
    id INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL CHECK (length(name) > 0),
    template TEXT NOT NULL CHECK (length(name) > 0),
    beamline INTEGER NOT NULL REFERENCES beamline(id) ON DELETE CASCADE ON UPDATE CASCADE
)
