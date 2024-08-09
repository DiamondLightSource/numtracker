SELECT template
FROM beamline
JOIN beamline_scan ON beamline.id = beamline_scan.beamline
JOIN scan_template ON scan_template.id = beamline_scan.scan
WHERE beamline.name = ?
ORDER BY modified desc
LIMIT 1
