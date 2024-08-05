CREATE TABLE beamline (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT UNIQUE NOT NULL
);

-- Templates for visit directories, scan files and detector files
CREATE TABLE visit_template (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    template TEXT NOT NULl
);
CREATE TABLE scan_template (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    template TEXT NOT NULl
);
CREATE TABLE detector_template (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    template TEXT NOT NULl
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

-- View to simplify access to templates for a given beamline
CREATE VIEW beamline_template (beamline, visit, scan, detector) AS SELECT DISTINCT
    beamline.name AS beamline,
    last_value(visit_template.template) OVER (PARTITION BY beamline_visit.beamline) AS visit,
    last_value(scan_template.template) OVER (PARTITION BY beamline_scan.beamline) AS scan,
    last_value(detector_template.template) OVER (PARTITION BY beamline_detector.beamline) AS detector
from beamline
join beamline_visit ON beamline.id = beamline_visit.beamline
join visit_template ON visit_template.id = beamline_visit.visit
join beamline_scan ON beamline.id = beamline_scan.beamline
join scan_template ON beamline_scan.scan = scan_template.id
join beamline_detector ON beamline.id = beamline_detector.beamline
join detector_template ON detector_template.id = beamline_detector.detector;

-- Simpler view to only access the visit directory
CREATE VIEW beamline_visit_template (beamline, template) AS SELECT DISTINCT
    beamline.name AS beamline,
    last_value(visit_template.template) OVER (PARTITION BY beamline_visit.beamline) AS template
FROM beamline
JOIN beamline_visit ON beamline.id = beamline_visit.beamline
JOIN visit_template ON visit_template.id = beamline_visit.visit;


-- dummy testing data
INSERT INTO beamline (name) VALUES ('i22'),('b21'),('i11'),('i11-1');

INSERT INTO visit_template (template)
VALUES
    ('/tmp/{instrument}/data/{year}/{visit}/'),
    ('/tmp/{instrument}/data/{proposal}/{year}/{visit}');
INSERT INTO scan_template (template)
VALUES
    ('{subdirectory}/{instrument}-{scan_number}'),
    ('{subdirectory}/{scan_number}/{instrument}-{scan_number}');
INSERT INTO detector_template (template)
VALUES
    ('{subdirectory}/{instrument}-{scan_number}-{detector}'),
    ('{subdirectory}/{scan_number}/{instrument}-{scan_number}-{detector}');

INSERT INTO beamline_visit (beamline, visit) VALUES (1,1),(2,1),(2,2),(3,1),(4,1);
INSERT INTO beamline_scan (beamline, scan) VALUES (1,1),(2,1),(2,2),(3,1),(4,2);
INSERT INTO beamline_detector (beamline, detector) VALUES (1,2),(2,2),(3,2),(4,1);
