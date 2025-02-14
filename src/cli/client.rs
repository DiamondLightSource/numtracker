use std::sync::LazyLock;

use clap::{Parser, Subcommand};
use url::Url;

static FALLBACK_HOST: LazyLock<Url> =
    LazyLock::new(|| Url::parse("http://localhost:8000").expect("Static URL is valid"));

#[derive(Debug, Parser)]
pub struct ClientOptions {
    #[clap(flatten)]
    pub connection: ConnectionOptions,
    #[clap(subcommand)]
    pub command: ClientCommand,
}

#[derive(Debug, Parser)]
pub struct ConnectionOptions {
    #[clap(long, short = 'H')]
    host: Option<Url>,
}
impl ConnectionOptions {
    pub(crate) fn host(&self) -> &Url {
        self.host.as_ref().unwrap_or(&FALLBACK_HOST)
    }
}

#[derive(Debug, Subcommand)]
pub enum ClientCommand {
    /// Query existing configurations
    Configuration {
        #[clap(short)]
        beamline: Option<Vec<String>>,
    },
    /// Update or add new configurations
    Configure {
        beamline: String,
        #[clap(flatten)]
        config: ConfigurationOptions,
    },
    /// Query for templated data
    Paths { beamline: String, visit: String },
    /// Request scan information
    Scan {
        beamline: String,
        visit: String,
        #[clap(short, long)]
        subdirectory: Option<String>,
        #[clap(short, long)]
        detectors: Option<Vec<String>>,
    },
}

#[derive(Debug, Parser)]
pub struct ConfigurationOptions {
    #[clap(long)]
    pub visit: Option<String>,
    #[clap(long)]
    pub scan: Option<String>,
    #[clap(long, alias = "det")]
    pub detector: Option<String>,
    #[clap(long = "number")]
    pub scan_number: Option<i64>,
    #[clap(long, alias = "ext")]
    pub tracker_file_extension: Option<String>,
}
