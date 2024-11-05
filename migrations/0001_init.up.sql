CREATE TABLE beamline (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT UNIQUE NOT NULL,
    scan_number INTEGER NOT NULL DEFAULT 0
);

-- Templates for visit directories, scan files and detector files
CREATE TABLE visit_template (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    template TEXT UNIQUE NOT NULl
);
CREATE TABLE scan_template (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    template TEXT UNIQUE NOT NULl
);
CREATE TABLE detector_template (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    template TEXT UNIQUE NOT NULl
);

-- Many-to-many tables for beamline to templates
CREATE TABLE beamline_visit (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    beamline INTEGER REFERENCES beamline (id),
    visit INTEGER REFERENCES visit_template (id),
    modified INTEGER DEFAULT (unixepoch())
);
CREATE TABLE beamline_scan (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    beamline INTEGER REFERENCES beamline (id),
    scan INTEGER REFERENCES scan_template (id),
    modified INTEGER DEFAULT (unixepoch())
);
CREATE TABLE beamline_detector (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    beamline INTEGER REFERENCES beamline (id),
    detector INTEGER REFERENCES detector_template (id),
    modified INTEGER DEFAULT (unixepoch())
);
