use std::fmt::Display;
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
    const DEFAULT_CLIENT: &str = "numtracker";

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
        let Some(file) = dirs::config_dir().map(|cnf| cnf.join("numtracker").join("config")) else {
            debug!("Unable to determine default file location - using default config");
            return Ok(Self::default());
        };

        match Self::from_file(&file).await {
            Err(ConfigFileError::MissingFile) => {
                debug!("Config file {file:?} not present - using default config");
                Ok(Self::default())
            }
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

impl Display for ClientConfiguration {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ClientConfiguration(host: ")?;
        match self.host {
            Some(ref h) => write!(f, "{h}")?,
            None => write!(f, "None")?,
        }
        write!(f, ", auth: ")?;
        match self.auth {
            Some(ref a) => write!(f, "{a}")?,
            None => write!(f, "None")?,
        }
        write!(f, ")")?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::fs::File;
    use std::io::Write;

    use tempfile::TempDir;

    use super::*;

    const HOST: &str = "http://numtracker.example.com";
    const AUTH: &str = "https://auth.example.com";
    const CLIENT_ID: &str = "custom_client";

    #[tokio::test]
    async fn load_from_file() {
        let dir = TempDir::new().unwrap();
        let cfg_file = dir.as_ref().join("config.toml");
        let mut file = File::create_new(&cfg_file).unwrap();
        write!(file, "host={HOST:?}\n").unwrap();

        let cfg = ClientConfiguration::from_file(cfg_file).await.unwrap();
        assert_eq!(cfg.host, Some(Url::parse(HOST).unwrap()));
        assert_eq!(cfg.auth, None);
        assert_eq!(cfg.client_id, None);

        assert_eq!(cfg.auth_config(), None);
    }

    #[tokio::test]
    async fn load_from_file_with_auth() {
        let dir = TempDir::new().unwrap();
        let cfg_file = dir.as_ref().join("config.toml");
        let mut file = File::create_new(&cfg_file).unwrap();
        write!(file, "host={HOST:?}\nauth={AUTH:?}\n").unwrap();

        let cfg = ClientConfiguration::from_file(cfg_file).await.unwrap();
        assert_eq!(cfg.host, Some(Url::parse(HOST).unwrap()));
        assert_eq!(cfg.auth, Some(Url::parse(AUTH).unwrap()));
        assert_eq!(cfg.client_id, None);

        assert_eq!(
            cfg.auth_config(),
            Some((&Url::parse(AUTH).unwrap(), "numtracker"))
        );
    }

    #[tokio::test]
    async fn load_from_file_with_client_id() {
        let dir = TempDir::new().unwrap();
        let cfg_file = dir.as_ref().join("config.toml");
        let mut file = File::create_new(&cfg_file).unwrap();
        write!(
            file,
            "host={HOST:?}\nauth={AUTH:?}\nclient_id={CLIENT_ID:?}\n"
        )
        .unwrap();

        let cfg = ClientConfiguration::from_file(cfg_file).await.unwrap();
        assert_eq!(cfg.host, Some(Url::parse(HOST).unwrap()));
        assert_eq!(cfg.auth, Some(Url::parse(AUTH).unwrap()));
        assert_eq!(cfg.client_id, Some(CLIENT_ID.into()));

        assert_eq!(
            cfg.auth_config(),
            Some((&Url::parse(AUTH).unwrap(), CLIENT_ID))
        );
    }
}
