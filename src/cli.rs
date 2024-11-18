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

use std::env;
use std::net::Ipv4Addr;
use std::path::PathBuf;

use clap::{Parser, Subcommand};
use clap_verbosity_flag::{InfoLevel, Verbosity};
use tracing::Level;
use url::Url;

#[derive(Debug, Parser)]
pub struct Cli {
    #[clap(short, long, default_value = "numtracker.db", env = "NUMTRACKER_DB")]
    pub(crate) db: PathBuf,
    #[clap(flatten, next_help_heading = "Logging/Debug")]
    verbose: Verbosity<InfoLevel>,
    #[clap(flatten, next_help_heading = "Tracing and Logging")]
    tracing: TracingOptions,
    #[clap(subcommand)]
    pub(crate) command: Command,
}

#[derive(Debug, Parser)]
pub struct TracingOptions {
    /// The URL of the tracing OTLP platform (eg Jaeger)
    #[clap(long = "tracing", env = "NUMTRACKER_TRACING")]
    tracing_url: Option<Url>,
    /// The minimum level of tracing events to send
    #[clap(long, default_value_t = Level::INFO, env = "NUMTRACKER_TRACING_LEVEL")]
    tracing_level: Level,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Run the server to respond to visit and scan path requests
    Serve(ServeOptions),
    /// Generate the graphql schema
    Schema,
}

#[derive(Debug, Parser)]
pub struct ServeOptions {
    /// The IP for this to service to be bound to
    #[clap(short = 'H', long, default_value_t = Ipv4Addr::UNSPECIFIED, env="NUMTRACKER_HOST")]
    host: Ipv4Addr,
    /// The port to open for requests
    #[clap(short, long, default_value_t = 8000, env = "NUMTRACKER_PORT")]
    port: u16,
}

impl Cli {
    pub fn init() -> Self {
        Self::parse()
    }
    pub fn log_level(&self) -> Option<Level> {
        use clap_verbosity_flag::Level as ClapLevel;
        match self.verbose.log_level() {
            Some(lvl) => Some(match lvl {
                ClapLevel::Error => Level::ERROR,
                ClapLevel::Warn => Level::WARN,
                ClapLevel::Info => Level::INFO,
                ClapLevel::Debug => Level::DEBUG,
                ClapLevel::Trace => Level::TRACE,
            }),
            None => Some(
                match env::var("NUMTRACKER_LOG_LEVEL")
                    .map(|lvl| lvl.to_ascii_lowercase())
                    .as_deref()
                {
                    Ok("info") => Level::INFO,
                    Ok("debug") => Level::DEBUG,
                    Ok("trace") => Level::TRACE,
                    Ok("warn") => Level::WARN,
                    Ok("error") => Level::ERROR,
                    _ => return None,
                },
            ),
        }
    }
    pub fn tracing(&self) -> &TracingOptions {
        &self.tracing
    }
}

impl ServeOptions {
    pub(crate) fn addr(&self) -> (Ipv4Addr, u16) {
        (self.host, self.port)
    }
}

impl TracingOptions {
    pub(crate) fn tracing_url(&self) -> Option<Url> {
        self.tracing_url.clone()
    }

    pub(crate) fn level(&self) -> Level {
        self.tracing_level
    }
}
