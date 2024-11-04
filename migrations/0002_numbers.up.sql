CREATE TABLE scan_number (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    beamline INTEGER REFERENCES beamline (id),
    last_number INTEGER NOT NULL
);

CREATE VIEW beamline_number (beamline, last_number) AS SELECT
beamline.name as beamline,
scan_number.last_number as last_number
FROM scan_number
JOIN beamline ON
scan_number.beamline = beamline.id;
