-- Drop everything in reverse order to prevent key constraint issues
DROP TABLE beamline_visit;
DROP TABLE beamline_scan;
DROP TABLE beamline_detector;

DROP TABLE visit_template;
DROP TABLE scan_template;
DROP TABLE detector_template;

DROP TABLE beamline;
