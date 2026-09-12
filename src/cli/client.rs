use clap::{Parser, Subcommand};
use url::Url;

#[derive(Debug, Parser)]
#[clap(max_term_width = 100)]
pub struct ClientOptions {
    /// The host address of the numtracker service
    ///
    /// This should be the root of the service address including the scheme and
    /// port (if non-standard) but not including the graphql path,
    /// eg https://numtracker.example.com
    #[clap(long, short = 'H', env = "NUMTRACKER_SERVICE_HOST")]
    pub host: Option<Url>,
    #[clap(flatten)]
    pub auth: AuthConfig,
    #[clap(subcommand)]
    pub command: ClientCommand,
}

#[derive(Debug, Parser, PartialEq)]
pub struct AuthConfig {
    /// The host address of the authorisation provider
    ///
    /// This should be the domain that has the .well-known/openid-configuration
    /// endpoint including scheme and port (if non-standard).
    /// eg https://authn.example.com/realms/master
    #[clap(long, env = "NUMTRACKER_AUTH_HOST")]
    pub auth: Option<Url>,
    /// The client ID to use when authenticating
    #[clap(long, env = "NUMTRACKER_AUTHN_CLIENTID")]
    pub client_id: Option<String>,
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
    pub directory: Option<String>,
    #[clap(long)]
    pub scan: Option<String>,
    #[clap(long, alias = "det")]
    pub detector: Option<String>,
    #[clap(long = "number")]
    pub scan_number: Option<i64>,
    #[clap(long, alias = "ext")]
    pub tracker_file_extension: Option<String>,
}
