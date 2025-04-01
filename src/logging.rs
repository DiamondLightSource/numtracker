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

pub fn init(logging: Option<Level>, tracing: &TracingOptions) -> Result<(), ExporterBuildError> {
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
