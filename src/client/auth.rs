// Create config from file if present
// Override with CLI options
// Read token if present
// Request token if not
//
// Exchange refresh token for real token
// ???
// Profit

use std::future::Future;
use std::io;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use axum_extra::headers::authorization::Bearer;
use axum_extra::headers::Authorization;
use serde::Deserialize;
use tokio::fs::{self, File, OpenOptions};
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use url::Url;

struct AuthController {
    auth_host: Url,
    token_file: PathBuf,
}

struct AccessToken {
    token: String,
    expiration: Instant,
}

impl AuthController {
    /// Read refresh token from file if present
    async fn read_refresh_token(&self) -> Option<String> {
        let mut token = String::new();
        File::open(&self.token_file)
            .await
            .ok()?
            .read_to_string(&mut token)
            .await
            .ok()?;
        Some(token)
    }

    /// Write refresh token to file, creating directories if required (and possible)
    async fn write_refresh_token(&self, token: &str) -> Result<(), io::Error> {
        let parent = self.token_file.parent().unwrap();
        fs::create_dir_all(&parent).await?;
        let mut out = OpenOptions::new()
            .create(true)
            .open(&self.token_file)
            .await?;
        out.write(token.as_bytes()).await?;
        Ok(())
    }

    /// Exchange refresh token (if one is present) for an access token
    async fn exchange_refresh_token(&self) -> Result<AccessToken, ()> {
        let client = reqwest::Client::new();
        let refresh = self.read_refresh_token().await.ok_or(())?;
        let params = [
            ("client_id", "numtracker"),
            ("grant_type", "refresh_token"),
            ("refresh_token", &refresh),
        ];
        let resp = client
            .post(self.auth_host.join("token").unwrap())
            .form(&params)
            .send()
            .await
            .unwrap();
        let detail = resp.json::<TokenResponse>().await.unwrap();
        let expiry = Instant::now() + Duration::from_secs(detail.expires_in);
        // try and save the new refresh token but ignore errors as we can deal with them next time
        let _ = self.write_refresh_token(&detail.refresh_token).await;
        Ok(AccessToken {
            token: detail.access_token,
            expiration: expiry,
        })
    }

    async fn init_login_flow(&self) -> Result<AccessToken, ()> {
        todo!()
    }

    pub async fn access_token(&self) -> Result<AccessToken, ()> {
        if let Ok(token) = self.exchange_refresh_token().await {
            return Ok(token);
        }
        // either no refresh token, or it has expired or some kind of network error
        self.init_login_flow().await
    }
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    expires_in: u64,
    refresh_token: String,
    refresh_expires_in: u64,
    token_type: String,
}

impl AccessToken {
    pub fn new(token: String, expires_in: u64) -> Self {
        Self {
            token,
            expiration: Instant::now() + Duration::from_secs(expires_in),
        }
    }
    pub fn is_alive(&self) -> bool {
        Instant::now() < self.expiration
    }
    pub fn as_header(&self) -> Result<Authorization<Bearer>, ()> {
        todo!()
    }
}
