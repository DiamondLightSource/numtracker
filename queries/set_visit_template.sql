INSERT INTO beamline_visit (beamline, visit)
SELECT id, ?
    FROM beamline
    WHERE name = ?
