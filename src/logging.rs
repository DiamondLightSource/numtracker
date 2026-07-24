// Copyright 2024 Diamond Light Source
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use std::time::Duration;

use opentelemetry::trace::TracerProvider as _;
use opentelemetry::{global, KeyValue};
use opentelemetry_otlp::{ExporterBuildError, SpanExporter, WithExportConfig as _};
use opentelemetry_sdk::trace::SdkTracerProvider;
use opentelemetry_sdk::Resource;
use opentelemetry_semantic_conventions::resource::{SERVICE_NAME, SERVICE_VERSION};
use opentelemetry_semantic_conventions::SCHEMA_URL;
use tracing::{warn, Level, Subscriber};
use tracing_gelf::Logger;
use tracing_opentelemetry::OpenTelemetryLayer;
use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::util::SubscriberInitExt as _;
use tracing_subscriber::{EnvFilter, Layer};
use url::Url;

use crate::build_info;
use crate::cli::{GraylogOptions, TracingOptions};

fn resource() -> Resource {
    Resource::builder()
        .with_schema_url(
            [
                KeyValue::new(SERVICE_NAME, env!("CARGO_PKG_NAME")),
                KeyValue::new(SERVICE_VERSION, env!("CARGO_PKG_VERSION")),
            ],
            SCHEMA_URL,
        )
        .build()
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

fn init_tracing<S>(endpoint: Option<Url>, level: Level) -> Result<impl Layer<S>, ExporterBuildError>
where
    S: Subscriber + for<'s> LookupSpan<'s>,
{
    if let Some(endpoint) = endpoint {
        let provider = SdkTracerProvider::builder()
            .with_batch_exporter(
                SpanExporter::builder()
                    .with_tonic()
                    .with_endpoint(endpoint)
                    .build()?,
            )
            .with_resource(resource())
            .build();
        global::set_tracer_provider(provider.clone());
        let tracer = provider.tracer("visit-service");
        Ok(Some(
            OpenTelemetryLayer::new(tracer).with_filter(LevelFilter::from_level(level)),
        ))
    } else {
        Ok(None)
    }
}

fn init_graylog<S>(opts: &GraylogOptions) -> Option<impl Layer<S>>
where
    S: Subscriber + for<'s> LookupSpan<'s>,
{
    let address = opts.address()?;
    let level = opts.level();

    match Logger::builder()
        .additional_field("version", build_info::PKG_VERSION)
        .additional_field(
            "build",
            build_info::GIT_COMMIT_HASH_SHORT.unwrap_or("unknown"),
        )
        .connect_tcp(address)
    {
        Ok((logger, mut handle)) => {
            tokio::spawn(async move {
                loop {
                    // This seems odd but the connection should remain open for the life
                    // of the process. If it returns with errors it means the connection failed
                    // or was closed. If there are no errors, it means there were no attempts
                    // to connect - most likely because the DNS lookup failed.
                    let errors = handle.connect().await;
                    if errors.0.is_empty() {
                        warn!(
                            "Graylog DNS lookup failed for {:?} - no addresses resolved",
                            handle.address()
                        );
                    } else {
                        for (addr, err) in &errors.0 {
                            warn!("Graylog connection to {addr} failed: {err}");
                        }
                    }
                    tokio::time::sleep(Duration::from_secs(5)).await;
                }
            });
            Some(logger.with_filter(LevelFilter::from_level(level)))
        }
        Err(e) => {
            // print instead of warn in case graylog is only logging configured and logging would
            // be dropped.
            eprintln!("Couldn't create graylog logger: {e}");
            // Don't return an error as graylog being unavailable should not prevent the numtracker
            // from starting/running
            None
        }
    }
}

pub fn init(
    logging: Option<Level>,
    tracing: &TracingOptions,
    graylog: &GraylogOptions,
) -> Result<(), ExporterBuildError> {
    let log_layer = init_stdout(logging);
    let trace_layer = init_tracing(tracing.tracing_url(), tracing.level())?;
    let graylog_layer = init_graylog(graylog);
    // Whatever level is set for logging/tracing, ignore the noise from the low-level libraries
    let filter = EnvFilter::new("trace") // let everything through
        .add_directive("h2=info".parse().expect("Static string is valid")) // except http,
        .add_directive("tower=info".parse().expect("Static string is valid")) // middleware
        .add_directive("tonic=debug".parse().expect("Static string is valid")); // and grpc

    tracing_subscriber::registry()
        .with(filter)
        .with(trace_layer)
        .with(log_layer)
        .with(graylog_layer)
        .init();
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::net::TcpListener;

    use tracing::Level;
    use tracing_subscriber::layer::SubscriberExt as _;
    use tracing_subscriber::Registry;
    use url::Url;

    use super::{init_graylog, init_stdout, init_tracing};
    use crate::cli::GraylogOptions;

    #[test]
    fn no_graylog_endpoint_returns_none() {
        let opts = GraylogOptions {
            graylog_host: None,
            graylog_port: 12201,
            graylog_level: Level::INFO,
        };
        let result = init_graylog::<Registry>(&opts);
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn graylog_with_endpoint_returns_layer() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind local port");
        let port = listener.local_addr().expect("local addr").port();
        let opts = GraylogOptions {
            graylog_host: Some("127.0.0.1".to_string()),
            graylog_port: port,
            graylog_level: Level::INFO,
        };
        let result = init_graylog::<Registry>(&opts);
        assert!(result.is_some());
    }

    #[test]
    fn init_stdout_none_when_no_level() {
        let layer = init_stdout::<Registry>(None);
        // Should not panic when composed into a subscriber with no output configured.
        let _ = tracing_subscriber::registry().with(layer);
    }

    #[test]
    fn init_stdout_some_when_level_given() {
        let layer = init_stdout::<Registry>(Some(Level::INFO));
        let _ = tracing_subscriber::registry().with(layer);
    }

    #[test]
    fn init_tracing_none_when_no_endpoint() {
        let layer = init_tracing::<Registry>(None, Level::INFO).expect("no exporter to build");
        let _ = tracing_subscriber::registry().with(layer);
    }

    #[tokio::test]
    async fn init_tracing_some_when_endpoint_given() {
        let url = Url::parse("http://127.0.0.1:4317").expect("valid url");
        let layer =
            init_tracing::<Registry>(Some(url), Level::INFO).expect("valid exporter config");
        let _ = tracing_subscriber::registry().with(layer);
    }
}
