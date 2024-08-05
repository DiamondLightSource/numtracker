use std::path::PathBuf;

use sqlx::sqlite::SqliteError;
use sqlx::{query_as, query_scalar, FromRow, Pool, Sqlite};

use crate::paths::{PathConstructor, TemplatePathConstructor};
use crate::template::PathTemplate;
use crate::{
    BeamlineContext, Instrument, ScanPathService, ScanRequest, ScanSpec, Visit, VisitRequest,
};

pub struct SqliteScanPathService {
    pub pool: Pool<Sqlite>,
}

// #[derive(Debug, FromRow)]
// pub struct TemplateResult {
//     beamline: String,
//     template: String,
// }

impl ScanPathService for SqliteScanPathService {
    type Err = SqliteError;

    async fn visit_directory(&self, req: VisitRequest) -> Result<PathBuf, Self::Err> {
        let template: Result<String, _> = query_scalar!(
            "SELECT template FROM beamline_visit_template WHERE beamline = ?",
            req.instrument
        )
        .fetch_one(&self.pool)
        .await;
        println!("{template:?}");
        let visit: Visit = req.visit.parse().unwrap();
        let beamline: Instrument = Instrument::try_from(req.instrument.as_str()).unwrap();
        let template = TemplatePathConstructor::new(template.unwrap()).unwrap();
        Ok(template
            .visit_directory(&BeamlineContext::new(req.instrument, visit))
            .unwrap())
    }

    async fn scan_spec(&self, req: ScanRequest) -> Result<ScanSpec, Self::Err> {
        todo!()
    }
}
