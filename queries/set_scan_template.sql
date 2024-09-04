INSERT INTO beamline_scan (beamline, scan)
SELECT id, ?
    FROM beamline
    WHERE name = ?
