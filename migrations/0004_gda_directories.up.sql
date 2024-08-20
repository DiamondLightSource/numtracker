CREATE TABLE number_file_directory (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    beamline INTEGER REFERENCES beamline(id) NOT NULL,
    directory TEXT NOT NULL,
    extension TEXT NOT NULL
);

CREATE VIEW beamline_number_file_directory (beamline, directory, extension) AS SELECT
beamline.name as beamline,
number_file_directory.directory as directory,
extension
FROM beamline
JOIN number_file_directory ON beamline.id = number_file_directory.beamline;

-- dummy test data
INSERT INTO number_file_directory (beamline, directory, extension) VALUES
    (1, "trackers/i22", "tmp"),
    (2, "trackers/b21", "b21");
