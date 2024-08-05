use std::path::PathBuf;

use sqlx::sqlite::SqliteError;
use sqlx::{query_as, query_scalar, FromRow, Pool, Sqlite};

use crate::paths::{PathConstructor, TemplatePathConstructor};
use crate::template::PathTemplate;
use crate::{
    BeamlineContext, DetectorPath, Instrument, ScanPathService, ScanRequest, ScanSpec, Visit,
    VisitRequest,
};

pub struct SqliteScanPathService {
    pub pool: Pool<Sqlite>,
}

// #[derive(Debug, FromRow)]
// pub struct TemplateResult {
//     beamline: String,
//     template: String,
// }

#[derive(Debug, FromRow)]
struct ScanTemplates {
    visit: String,
    scan: String,
    detector: String,
}

impl ScanPathService for SqliteScanPathService {
    type Err = SqliteError;

    async fn visit_directory(&self, req: VisitRequest) -> Result<PathBuf, Self::Err> {
        let template: Result<String, _> = query_scalar!(
            "SELECT template FROM beamline_visit_template WHERE beamline = ?",
            req.instrument
        )
        .fetch_one(&self.pool)
        .await;
        let visit: Visit = req.visit.parse().unwrap();
        let beamline: Instrument = Instrument::try_from(req.instrument.as_str()).unwrap();
        let template = TemplatePathConstructor::new(template.unwrap()).unwrap();
        Ok(template
            .visit_directory(&BeamlineContext::new(req.instrument, visit))
            .unwrap())
    }

    async fn scan_spec(&self, req: ScanRequest) -> Result<ScanSpec, Self::Err> {
        let templates: ScanTemplates = query_as!(
            ScanTemplates,
            "SELECT visit, scan, detector FROM beamline_template WHERE beamline = ?",
            req.instrument
        )
        .fetch_one(&self.pool)
        .await
        .unwrap();
        let visit = req.visit.parse().unwrap();
        let beamline = req.instrument.as_str().try_into().unwrap();
        let template = TemplatePathConstructor::new(templates.visit).unwrap();
        let ctx = BeamlineContext::new(req.instrument, visit);
        let visit_directory = template.visit_directory(&ctx).unwrap();
        let scan_ctx = ctx.next_scan();
        let scan = template.scan_file(&scan_ctx).unwrap();
        let detectors = req
            .detectors
            .into_iter()
            .map(|det| {
                let file = template
                    .detector_file(&scan_ctx.for_detector(&det))
                    .unwrap();
                DetectorPath(det, file)
            })
            .collect();
        let spec = ScanSpec {
            beamline,
            visit: scan_ctx.beamline.visit.clone(),
            visit_directory,

            scan_number: scan_ctx.scan_number,
            scan_file: scan,
            detector_files: detectors,
        };

        Ok(spec)
    }
}
