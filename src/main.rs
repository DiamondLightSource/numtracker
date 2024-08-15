use std::error::Error;
use std::fmt::Display;
use std::marker::PhantomData;
use std::path::PathBuf;
use std::time::Duration;

use async_graphql::{Context, EmptySubscription, Object, ObjectType, Schema, SimpleObject};
use async_graphql_axum::{GraphQLRequest, GraphQLResponse};
use axum::routing::post;
use axum::{Extension, Router};
use numtracker::db_service::SqliteScanPathService;
use numtracker::fallback::FallbackScanNumbering;
use numtracker::numtracker::GdaNumTracker;
use numtracker::{
    BeamlineContext, PathTemplateBackend, ScanNumberBackend, ScanService, Subdirectory,
    VisitService,
};
use opentelemetry::trace::TracerProvider as _;
use opentelemetry::{global, KeyValue};
use opentelemetry_sdk::metrics::reader::{DefaultAggregationSelector, DefaultTemporalitySelector};
use opentelemetry_sdk::metrics::{MeterProviderBuilder, PeriodicReader, SdkMeterProvider};
use opentelemetry_sdk::trace::{BatchConfig, RandomIdGenerator, Sampler, Tracer};
use opentelemetry_sdk::{runtime, Resource};
use opentelemetry_semantic_conventions::resource::{
    DEPLOYMENT_ENVIRONMENT, SERVICE_NAME, SERVICE_VERSION,
};
use opentelemetry_semantic_conventions::SCHEMA_URL;
use tokio::net::TcpListener;
use tracing::{debug, instrument, Level};
use tracing_opentelemetry::{MetricsLayer, OpenTelemetryLayer};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt as _;

type SqliteGda = FallbackScanNumbering<SqliteScanPathService, GdaNumTracker>;

fn resource() -> Resource {
    Resource::from_schema_url(
        [
            KeyValue::new(SERVICE_NAME, env!("CARGO_PKG_NAME")),
            KeyValue::new(SERVICE_VERSION, env!("CARGO_PKG_VERSION")),
            KeyValue::new(DEPLOYMENT_ENVIRONMENT, "dev"),
        ],
        SCHEMA_URL,
    )
}

fn init_metrics() -> SdkMeterProvider {
    let exporter = opentelemetry_otlp::new_exporter()
        .tonic()
        .build_metrics_exporter(
            Box::new(DefaultAggregationSelector::new()),
            Box::new(DefaultTemporalitySelector::new()),
        )
        .unwrap();
    let reader = PeriodicReader::builder(exporter, runtime::Tokio)
        .with_interval(Duration::from_secs(30))
        .build();
    let stdout_reader = PeriodicReader::builder(
        opentelemetry_stdout::MetricsExporter::default(),
        runtime::Tokio,
    )
    .build();

    let meter_provider = MeterProviderBuilder::default()
        .with_resource(resource())
        .with_reader(reader)
        .with_reader(stdout_reader)
        .build();
    global::set_meter_provider(meter_provider.clone());
    meter_provider
}

fn init_tracer() -> Tracer {
    let provider = opentelemetry_otlp::new_pipeline()
        .tracing()
        .with_trace_config(
            opentelemetry_sdk::trace::Config::default()
                .with_sampler(Sampler::ParentBased(Box::new(Sampler::TraceIdRatioBased(
                    1.0,
                ))))
                .with_id_generator(RandomIdGenerator::default())
                .with_resource(resource()),
        )
        .with_batch_config(BatchConfig::default())
        .with_exporter(opentelemetry_otlp::new_exporter().tonic())
        .install_batch(runtime::Tokio)
        .unwrap();
    global::set_tracer_provider(provider.clone());
    provider.tracer("visit-service")
}

fn init_tracing_subscriber() -> OtelGuard {
    let tracer = init_tracer();
    let metrics = init_metrics();
    tracing_subscriber::registry()
        .with(tracing_subscriber::filter::LevelFilter::from_level(
            Level::DEBUG,
        ))
        .with(tracing_subscriber::fmt::layer())
        .with(OpenTelemetryLayer::new(tracer))
        .with(MetricsLayer::new(metrics.clone()))
        .init();
    OtelGuard(metrics)
}

struct OtelGuard(SdkMeterProvider);

impl Drop for OtelGuard {
    fn drop(&mut self) {
        if let Err(err) = self.0.shutdown() {
            eprintln!("{err:?}");
        }
        opentelemetry::global::shutdown_tracer_provider();
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let _otel = init_tracing_subscriber();
    let backend = SqliteScanPathService::connect("./demo.db").await.unwrap();
    debug!("Using backend: {backend:?}");

    let schema = Schema::build(
        Query::<SqliteGda>::default(),
        Mutation::<SqliteGda>::default(),
        EmptySubscription,
    )
    .data(FallbackScanNumbering {
        primary: backend,
        secondary: GdaNumTracker::new("trackers"),
    })
    .finish();

    let app = Router::new()
        .route("/graphql", post(graphql_handler::<SqliteGda, SqliteGda>))
        .layer(Extension(schema));

    let listener = TcpListener::bind("127.0.0.1:8000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
    Ok(())
}

async fn graphql_handler<Q, M>(
    schema: Extension<Schema<Query<Q>, Mutation<M>, EmptySubscription>>,
    req: GraphQLRequest,
) -> GraphQLResponse
where
    Q: 'static,
    M: 'static,
    Query<Q>: ObjectType,
    Mutation<M>: ObjectType,
{
    schema.execute(req.into_inner()).await.into()
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
    #[instrument(skip(self))]
    async fn visit(&self) -> &str {
        self.service.visit()
    }
    #[instrument(skip(self))]
    async fn beamline(&self) -> &str {
        self.service.beamline()
    }
    #[instrument(skip(self))]
    async fn directory(&self) -> async_graphql::Result<String> {
        let visit_directory = self.service.visit_directory().await?;
        Ok(NonUnicodePath::check(visit_directory)?)
    }
}

#[Object]
impl<B: PathTemplateBackend> ScanPaths<B> {
    /// The visit used to generate this scan information
    /// Should be the same as the visit passed in
    #[instrument(skip(self))]
    async fn visit(&self) -> &str {
        self.service.visit()
    }

    /// The root scan file for this scan. The path has no extension so that the format can be
    /// chosen by the client.
    #[instrument(skip(self))]
    async fn scan_file(&self) -> async_graphql::Result<String> {
        Ok(NonUnicodePath::check(self.service.scan_file().await?)?)
    }

    /// The scan number for this scan. This should be unique for the requested beamline.
    #[instrument(skip(self))]
    async fn scan_number(&self) -> usize {
        self.service.scan_number()
    }

    /// The beamline used to generate this scan information
    /// Should be the same as the beamline passed in.
    #[instrument(skip(self))]
    async fn beamline(&self) -> &str {
        self.service.beamline()
    }

    /// The root visit directory for the given visit/beamline.
    ///
    /// This is not necessarily the directory where data should be written if subdirectories are
    /// being used, or if detectors should be writing their files to a new directory for each scan.
    /// Use `scan_file` and `detectors` to determine where specific files should be written.
    #[instrument(skip(self))]
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
    #[instrument(skip(self))]
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
    #[instrument(skip(self, ctx))]
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
    #[instrument(skip(self, ctx))]
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
