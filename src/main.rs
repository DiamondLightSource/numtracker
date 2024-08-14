use std::error::Error;
use std::fmt::Display;
use std::marker::PhantomData;
use std::path::PathBuf;

use async_graphql::{Context, EmptySubscription, Object, Schema, SimpleObject};
use numtracker::db_service::SqliteScanPathService;
use numtracker::{
    BeamlineContext, PathTemplateBackend, ScanNumberBackend, ScanService, Subdirectory,
    VisitService,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let backend = SqliteScanPathService::connect("./demo.db").await.unwrap();
    let schema = Schema::build(
        Query::<SqliteScanPathService>::default(),
        Mutation::<SqliteScanPathService>::default(),
        EmptySubscription,
    )
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

/// Read-only API for GraphQL
///
/// Generic type is only required so the type of service to be retrieved from the context can be
/// accessed.
struct Query<B>(PhantomData<B>);
impl<B> Default for Query<B> {
    fn default() -> Self {
        Self(Default::default())
    }
}

/// Read-write API for GraphQL
///
/// Generic type is only required so the type of service to be retrieved from the context can be
/// accessed.
struct Mutation<B>(PhantomData<B>);
impl<B> Default for Mutation<B> {
    fn default() -> Self {
        Self(Default::default())
    }
}

/// GraphQL type to mimic a key-value pair from the map type that GraphQL doesn't have
#[derive(SimpleObject)]
struct DetectorPath {
    name: String,
    path: String,
}

/// GraphQL type to provide path data for a specific visit
struct VisitPath<B: PathTemplateBackend> {
    service: VisitService<B>,
}

/// GraphQL type to provide path data for the next scan for a given visit
struct ScanPaths<B> {
    service: ScanService<B>,
}

#[derive(Debug)]
struct NonUnicodePath;

impl NonUnicodePath {
    /// Try and convert a path to a string (via OsString), returning a NonUnicodePath
    /// error if not possible
    fn check(path: PathBuf) -> Result<String, NonUnicodePath> {
        path.into_os_string()
            .into_string()
            .map_err(|_| NonUnicodePath)
    }
}

impl Display for NonUnicodePath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Path contains non-unicode characters")
    }
}

impl Error for NonUnicodePath {}

#[Object]
impl<B: PathTemplateBackend> VisitPath<B> {
    async fn visit(&self) -> &str {
        self.service.visit()
    }
    async fn beamline(&self) -> &str {
        self.service.beamline()
    }
    async fn directory(&self) -> async_graphql::Result<String> {
        let visit_directory = self.service.visit_directory().await?;
        Ok(NonUnicodePath::check(visit_directory)?)
    }
}

#[Object]
impl<B: PathTemplateBackend> ScanPaths<B> {
    /// The visit used to generate this scan information
    /// Should be the same as the visit passed in
    async fn visit(&self) -> &str {
        self.service.visit()
    }

    /// The root scan file for this scan. The path has no extension so that the format can be
    /// chosen by the client.
    async fn scan_file(&self) -> async_graphql::Result<String> {
        Ok(NonUnicodePath::check(self.service.scan_file().await?)?)
    }

    /// The scan number for this scan. This should be unique for the requested beamline.
    async fn scan_number(&self) -> usize {
        self.service.scan_number()
    }

    /// The beamline used to generate this scan information
    /// Should be the same as the beamline passed in.
    async fn beamline(&self) -> &str {
        self.service.beamline()
    }

    /// The root visit directory for the given visit/beamline.
    ///
    /// This is not necessarily the directory where data should be written if subdirectories are
    /// being used, or if detectors should be writing their files to a new directory for each scan.
    /// Use `scan_file` and `detectors` to determine where specific files should be written.
    async fn directory(&self) -> async_graphql::Result<String> {
        Ok(NonUnicodePath::check(
            self.service.visit_directory().await?,
        )?)
    }

    /// The paths where the given detectors should write their files.
    ///
    /// Detector names are normalised before being used in file names by replacing any
    /// non-alphanumeric characters with '_'. If there are duplicate names in the list
    /// of detectors after this normalisation, there will be duplicate paths in the
    /// results.
    // TODO: The docs here reference the implementation specific behaviour in the normalisation
    async fn detectors(&self, names: Vec<String>) -> async_graphql::Result<Vec<DetectorPath>> {
        Ok(self
            .service
            .detector_files(&names)
            .await?
            .into_iter()
            .map(|(det, path)| {
                NonUnicodePath::check(path).map(|path| DetectorPath {
                    name: det.into(),
                    path,
                })
            })
            .collect::<Result<_, _>>()?)
    }
}

#[Object]
impl<B: PathTemplateBackend + 'static> Query<B> {
    async fn paths(
        &self,
        ctx: &Context<'_>,
        beamline: String,
        visit: String,
    ) -> async_graphql::Result<VisitPath<B>> {
        let db = ctx.data::<B>()?;
        let service = VisitService::new(db.clone(), BeamlineContext::new(beamline, visit));
        Ok(VisitPath { service })
    }
}

#[Object]
impl<B: PathTemplateBackend + ScanNumberBackend + 'static> Mutation<B> {
    async fn scan<'ctx>(
        &self,
        ctx: &Context<'ctx>,
        beamline: String,
        visit: String,
        sub: Option<String>,
    ) -> async_graphql::Result<ScanPaths<B>> {
        let db = ctx.data::<B>()?;
        let service = VisitService::new(db.clone(), BeamlineContext::new(beamline, visit));
        let sub = Subdirectory::new(sub.unwrap_or_default())?;
        let service = service.new_scan(sub).await?;
        Ok(ScanPaths { service })
    }
}
