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

use opentelemetry::trace::TracerProvider as _;
use opentelemetry::{global, KeyValue};
use opentelemetry_otlp::{ExporterBuildError, SpanExporter, WithExportConfig as _};
use opentelemetry_sdk::trace::SdkTracerProvider;
use opentelemetry_sdk::Resource;
use opentelemetry_semantic_conventions::resource::{SERVICE_NAME, SERVICE_VERSION};
use opentelemetry_semantic_conventions::SCHEMA_URL;
use tracing::{error, warn, Level, Subscriber};
use tracing_gelf::Logger;
use tracing_opentelemetry::OpenTelemetryLayer;
use tracing_subscriber::filter::{FilterFn,LevelFilter};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::util::SubscriberInitExt as _;
use tracing_subscriber::{EnvFilter, Layer};
use url::Url;

use crate::cli::{GraylogOptions, TracingOptions};

#[derive(Debug, thiserror::Error)]
pub enum LoggingError {
    #[error("Exporter build error: {0}")]
    Exporter(#[from] ExporterBuildError),
    #[error("Graylog error: {0}")]
    Graylog(#[from] std::io::Error),
    #[error("Graylog configuration error: {0}")]
    Config(String),
}

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

fn init_graylog<S>(opts: &GraylogOptions) -> Result<Option<impl Layer<S>>, LoggingError>
where
    S: Subscriber + for<'s> LookupSpan<'s>,
{
    if let Some(address) = opts.address().map_err(LoggingError::Config)? {
        let level = opts.logging_level;

        match Logger::builder().connect_tcp(address) {
            Ok((logger, mut handle)) => {
                tokio::spawn(async move {
                    loop {
                        let errors = handle.connect().await;
                        if errors.0.is_empty() {
                            error!(
                                "Graylog DNS lookup failed for {:?} - no addresses resolved",
                                handle.address()
                            );
                            break;
                        } else {
                            for (addr, err) in &errors.0 {
                                warn!("Graylog connection to {addr} failed: {err} - reconnecting");
                            }
                        }
                    }
                });
                Ok(Some(
                    logger
                        .with_filter(LevelFilter::from_level(level))
                        .with_filter(FilterFn::new(|m| {
                            m.target().starts_with(env!("CARGO_PKG_NAME"))
                        })),
                ))
            }
            Err(e) => {
                eprintln!("Couldn't create graylog logger: {e}");
                Ok(None)
            }
        }
    } else {
        Ok(None)
    }
}

pub fn init(logging: Option<Level>, tracing: &TracingOptions, graylog: &GraylogOptions) -> Result<(), LoggingError> {
    let log_layer = init_stdout(logging);
    let trace_layer = init_tracing(tracing.tracing_url(), tracing.level())?;
    let graylog_layer = init_graylog(graylog)?;
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
    use tracing_subscriber::Registry;

    use super::init_graylog;
    use crate::cli::GraylogOptions;

    #[test]
    fn no_graylog_endpoint_returns_none() {
        let opts = GraylogOptions { graylog_url: None, logging_level: Level::INFO };
        let result = init_graylog::<Registry>(&opts);
        assert!(matches!(result, Ok(None)));
    }

    #[test]
    fn graylog_url_without_port_is_an_error() {
        let opts = GraylogOptions {
            graylog_url: Some("tcp://graylog.example.com".parse().expect("valid url")),
            logging_level: Level::INFO,
        };
        let result = init_graylog::<Registry>(&opts);
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn graylog_with_endpoint_returns_layer() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind local port");
        let port = listener.local_addr().expect("local addr").port();
        let opts = GraylogOptions {
            graylog_url: Some(format!("tcp://127.0.0.1:{port}").parse().expect("valid url")),
            logging_level: Level::INFO,
        };
        let result = init_graylog::<Registry>(&opts);
        assert!(matches!(result, Ok(Some(_))));
    }
}
