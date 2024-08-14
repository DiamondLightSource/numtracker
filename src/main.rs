use std::error::Error;

use async_graphql::{Context, EmptySubscription, Object, Schema, SimpleObject};
use numtracker::db_service::SqliteScanPathService;
use numtracker::{BeamlineContext, ScanService, Subdirectory, VisitService, VisitServiceBackend};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let backend = SqliteScanPathService::connect("sqlite://./demo.db")
        .await
        .unwrap();
    let schema = Schema::build(Query, Mutation, EmptySubscription)
        .data(backend)
        .finish();
    let res = schema
        .execute(
            r#"
            mutation {
                scan(beamline: "i22", visit: "cm1234-3", sub: "foo/bar") {
                    directory
                    beamline
                    visit
                    scanFile
                    scanNumber
                    detectors(names: ["one", "two"]) {
                        name
                        path
                    }
                }
            }"#,
        )
        .await;
    println!("{}", res.data);
    let res = schema
        .execute(
            r#"
            {
                paths(beamline: "i22", visit: "cm1234-2") {
                    directory
                    beamline
                    visit
                }
            }"#,
        )
        .await;
    println!("{}", res.data);

    Ok(())
}

struct Query;

struct Mutation;

#[derive(SimpleObject)]
struct DetectorPath {
    name: String,
    path: String,
}

struct VisitPath<B: VisitServiceBackend> {
    service: VisitService<B>,
}

struct ScanPaths<B> {
    service: ScanService<B>,
}

#[Object]
impl<B: VisitServiceBackend> VisitPath<B> {
    async fn visit(&self) -> &str {
        self.service.visit()
    }
    async fn beamline(&self) -> &str {
        self.service.beamline()
    }
    async fn directory(&self) -> async_graphql::Result<String> {
        let visit_directory = self.service.visit_directory().await?;
        Ok(visit_directory.to_string_lossy().to_string())
    }
}

#[Object]
impl<B: VisitServiceBackend> ScanPaths<B> {
    async fn visit(&self) -> &str {
        self.service.visit()
    }
    async fn scan_file(&self) -> async_graphql::Result<String> {
        Ok(self
            .service
            .scan_file()
            .await?
            .to_string_lossy()
            .to_string())
    }
    async fn scan_number(&self) -> usize {
        self.service.scan_number()
    }
    async fn beamline(&self) -> &str {
        self.service.beamline()
    }
    async fn directory(&self) -> async_graphql::Result<String> {
        Ok(self
            .service
            .visit_directory()
            .await?
            .to_string_lossy()
            .to_string())
    }
    async fn detectors(&self, names: Vec<String>) -> async_graphql::Result<Vec<DetectorPath>> {
        Ok(self
            .service
            .detector_files(&names)
            .await?
            .into_iter()
            .map(|(det, path)| DetectorPath {
                name: det.into(),
                path: path.to_string_lossy().into(),
            })
            .collect())
    }
}

#[Object]
impl Query {
    async fn paths(
        &self,
        ctx: &Context<'_>,
        beamline: String,
        visit: String,
    ) -> async_graphql::Result<VisitPath<SqliteScanPathService>> {
        let db = ctx.data::<SqliteScanPathService>()?;
        let service = VisitService::new(db.clone(), BeamlineContext::new(beamline, visit));
        Ok(VisitPath { service })
    }
}

#[Object]
impl Mutation {
    async fn scan<'ctx>(
        &self,
        ctx: &Context<'ctx>,
        beamline: String,
        visit: String,
        sub: Option<String>,
    ) -> async_graphql::Result<ScanPaths<SqliteScanPathService>> {
        let db = ctx.data::<SqliteScanPathService>()?;
        let service = VisitService::new(db.clone(), BeamlineContext::new(beamline, visit));
        let sub = Subdirectory::new(sub.unwrap_or_default())?;
        let new_scan = service.new_scan(sub).await?;
        Ok(ScanPaths { service: new_scan })
    }
}
