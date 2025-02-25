use clap::{Parser, Subcommand};
use url::Url;

#[derive(Debug, Parser)]
pub struct ClientOptions {
    #[clap(flatten)]
    pub connection: ConnectionOptions,
    #[clap(subcommand)]
    pub command: ClientCommand,
}

#[derive(Debug, Parser)]
pub struct ConnectionOptions {
    #[clap(long, short = 'H', env = "NUMTRACKER_SERVICE_HOST")]
    pub host: Option<Url>,
    #[clap(long, env = "NUMTRACKER_AUTH_HOST")]
    pub auth: Option<Url>,
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
    VisitDirectory { beamline: String, visit: String },
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
