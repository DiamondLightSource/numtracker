use opentelemetry::trace::{TraceError, TracerProvider as _};
use opentelemetry::{global, KeyValue};
use opentelemetry_otlp::{new_pipeline, WithExportConfig};
use opentelemetry_sdk::trace::Config;
use opentelemetry_sdk::{runtime, Resource};
use opentelemetry_semantic_conventions::resource::{
    DEPLOYMENT_ENVIRONMENT, SERVICE_NAME, SERVICE_VERSION,
};
use opentelemetry_semantic_conventions::SCHEMA_URL;
use tracing::{Level, Subscriber};
use tracing_opentelemetry::OpenTelemetryLayer;
use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::util::SubscriberInitExt as _;
use tracing_subscriber::{EnvFilter, Layer};
use url::Url;

use crate::cli::TracingOptions;

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

fn init_stdout<S>(level: Option<Level>) -> impl Layer<S>
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    level.map(|lvl| {
        tracing_subscriber::fmt::layer()
            .with_writer(std::io::stderr)
            .with_filter(LevelFilter::from_level(lvl))
    })
}

fn init_tracing<S>(endpoint: Option<Url>, level: Level) -> Result<impl Layer<S>, TraceError>
where
    S: Subscriber + for<'s> LookupSpan<'s>,
{
    if let Some(endpoint) = endpoint {
        let provider = new_pipeline()
            .tracing()
            .with_trace_config(Config::default().with_resource(resource()))
            .with_exporter(
                opentelemetry_otlp::new_exporter()
                    .tonic()
                    .with_endpoint(endpoint),
            )
            .install_batch(runtime::Tokio)?;
        global::set_tracer_provider(provider.clone());
        let tracer = provider.tracer("visit-service");
        Ok(Some(
            OpenTelemetryLayer::new(tracer).with_filter(LevelFilter::from_level(level)),
        ))
    } else {
        Ok(None)
    }
}

pub fn init_logging(logging: Option<Level>, tracing: &TracingOptions) -> Result<(), TraceError> {
    let log_layer = init_stdout(logging);
    let trace_layer = init_tracing(tracing.tracing_url(), tracing.level())?;

    // Whatever level is set for logging/tracing, ignore the noise from the low-level libraries
    let filter = EnvFilter::new("trace") // let everything through
        .add_directive("h2=info".parse().expect("Static string is valid")) // except http,
        .add_directive("tower=info".parse().expect("Static string is valid")) // middleware
        .add_directive("tonic=debug".parse().expect("Static string is valid")); // and grpc

    tracing_subscriber::registry()
        .with(filter)
        .with(trace_layer)
        .with(log_layer)
        .init();
    Ok(())
}