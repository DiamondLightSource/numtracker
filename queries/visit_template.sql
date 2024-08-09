SELECT template
FROM beamline
JOIN beamline_visit ON beamline.id = beamline_visit.beamline
JOIN visit_template ON visit_template.id = beamline_visit.visit
WHERE beamline.name = ?
ORDER BY modified desc
LIMIT 1
