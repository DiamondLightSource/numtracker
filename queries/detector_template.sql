SELECT template
FROM beamline
JOIN beamline_detector ON beamline.id = beamline_detector.beamline
JOIN detector_template ON detector_template.id = beamline_detector.detector
WHERE beamline.name = ?
ORDER BY modified desc
LIMIT 1
