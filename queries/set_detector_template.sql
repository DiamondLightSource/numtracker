INSERT INTO beamline_detector (beamline, detector)
SELECT id, ?
    FROM beamline
    WHERE name = ?
