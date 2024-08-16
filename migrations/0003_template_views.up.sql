CREATE VIEW beamline_visit_template ( beamline, template ) AS SELECT
beamline.name as beamline,
visit_template.template as template
FROM beamline
JOIN beamline_visit ON beamline.id = beamline_visit.beamline
JOIN visit_template ON visit_template.id = beamline_visit.visit;

CREATE VIEW beamline_scan_template ( beamline, template ) AS SELECT
beamline.name as beamline,
scan_template.template as template
FROM beamline
JOIN beamline_scan ON beamline.id = beamline_scan.beamline
JOIN scan_template ON scan_template.id = beamline_scan.scan;

CREATE VIEW beamline_detector_template ( beamline, template ) AS SELECT
beamline.name as beamline,
detector_template.template as template
FROM beamline
JOIN beamline_detector ON beamline.id = beamline_detector.beamline
JOIN detector_template ON detector_template.id = beamline_detector.detector;
