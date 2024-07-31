use std::collections::HashMap;
use std::path::PathBuf;

use crate::numtracker::{GdaNumTracker, NumTracker};
use crate::paths::{PathConstructor, TemplatePathConstructor};
use crate::{BeamlineContext, Instrument, ScanContext, Subdirectory, Visit};

pub struct Controller {
    default: TemplatePathConstructor,
    beamlines: HashMap<Instrument, TemplatePathConstructor>,
    scan_numbers: GdaNumTracker,
}

#[derive(Debug)]
pub struct VisitRequest {
    instrument: String,
    visit: String,
}

#[derive(Debug)]
pub struct ScanRequest {
    instrument: String,
    visit: String,
    subdirectory: Option<String>,
    detectors: Vec<String>,
}

#[derive(Debug)]
pub struct DetectorPath(String, PathBuf);

#[derive(Debug)]
pub struct ScanSpec {
    beamline: Instrument,
    visit: Visit,
    visit_directory: PathBuf,
    scan_number: usize,
    scan_file: PathBuf,
    detector_files: Vec<DetectorPath>,
}

impl VisitRequest {
    pub fn new(instrument: String, visit: String) -> Self {
        Self { instrument, visit }
    }
}
impl ScanRequest {
    pub fn new(
        instrument: String,
        visit: String,
        subdirectory: Option<String>,
        detectors: Vec<String>,
    ) -> Self {
        Self {
            instrument,
            visit,
            subdirectory,
            detectors,
        }
    }
}

impl Controller {
    pub fn visit_directory(&self, req: VisitRequest) -> PathBuf {
        let instrument = Instrument::try_from(req.instrument.as_str()).unwrap();
        let visit = req.visit.parse().unwrap();
        let pc = self.beamlines.get(&instrument).unwrap_or(&self.default);
        pc.visit_directory(&BeamlineContext { instrument, visit })
            .unwrap()
    }

    pub fn scan_spec(&self, req: ScanRequest) -> ScanSpec {
        let instrument = Instrument::try_from(req.instrument.as_str()).unwrap();
        let visit = req.visit.parse().unwrap();
        let subdirectory = req
            .subdirectory
            .map(Subdirectory::new)
            .unwrap_or(Ok(Subdirectory::default()))
            .unwrap();

        let pc = self.beamlines.get(&instrument).unwrap_or(&self.default);
        let beamline_context = BeamlineContext { instrument, visit };
        let visit = pc.visit_directory(&beamline_context).unwrap();
        let scan_number = self
            .scan_numbers
            .increment_and_get(&beamline_context)
            .unwrap();
        let scan_ctx = ScanContext {
            subdirectory,
            scan_number,
            beamline: &beamline_context,
        };
        let scan_file = pc.scan_file(&scan_ctx).unwrap();
        let mut detector_files = Vec::new();
        for det in req.detectors {
            let ctx = scan_ctx.for_detector(&det);
            let path = pc.detector_file(&ctx).unwrap();
            detector_files.push(DetectorPath(det, path));
        }
        ScanSpec {
            beamline: beamline_context.instrument,
            visit: beamline_context.visit,
            visit_directory: visit,
            scan_number,
            scan_file,
            detector_files,
        }
    }
}

impl Default for Controller {
    fn default() -> Self {
        let default_paths =
            TemplatePathConstructor::new("/tmp/{instrument}/data/{year}/{visit}").unwrap();
        Self {
            default: default_paths,
            beamlines: [(
                "b21".try_into().unwrap(),
                TemplatePathConstructor::new("/tmp/{instrument}/data/{proposal}/{year}/{visit}")
                    .unwrap(),
            )]
            .into(),
            scan_numbers: GdaNumTracker::new("trackers"),
        }
    }
}
