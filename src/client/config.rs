use std::io::ErrorKind;
use std::path::Path;

use derive_more::{Display, Error, From};
use serde::Deserialize;
use tokio::fs;
use tracing::debug;
use url::Url;

#[derive(Debug, Deserialize, Default)]
pub struct ClientConfiguration {
    pub host: Option<Url>,
    pub auth: Option<Url>,
}

#[derive(Debug, Display, Error, From)]
pub enum ConfigFileError {
    #[display("Configuration file could not be found")]
    MissingFile,
    #[display("Configuration file could not be read: {_0}")]
    Unreadable(std::io::Error),
    #[display("Configuration file was invalid: {_0}")]
    InvalidContent(toml::de::Error),
}

impl ClientConfiguration {
    pub async fn from_file<P: AsRef<Path>>(path: P) -> Result<Self, ConfigFileError> {
        debug!("Reading client config from {:?}", path.as_ref());
        match fs::read_to_string(path.as_ref()).await {
            Ok(src) => Ok(toml::from_str(&src)?),
            Err(e) => match e.kind() {
                ErrorKind::NotFound | ErrorKind::IsADirectory => Err(ConfigFileError::MissingFile),
                _ => Err(ConfigFileError::Unreadable(e)),
            },
        }
    }

    pub async fn from_default_file() -> Result<Self, ConfigFileError> {
        let default_file = dirs::config_dir()
            .expect("Config dir should be known")
            .join("numtracker")
            .join("config");
        match Self::from_file(default_file).await {
            Err(ConfigFileError::MissingFile) => Ok(Self::default()),
            res => res,
        }
    }

    pub(crate) fn with_host(mut self, host: Option<Url>) -> Self {
        self.host = host.or(self.host);
        self
    }

    pub(crate) fn with_auth(mut self, auth: Option<Url>) -> Self {
        self.auth = auth.or(self.auth);
        self
    }
}
