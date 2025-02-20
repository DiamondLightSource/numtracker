use std::path::PathBuf;

use derive_more::{Display, Error};
use openidconnect::OAuth2TokenResponse;
use tokio::fs;
use tokio::io::AsyncWriteExt as _;
use tracing::{debug, trace, warn};
use url::Url;

use super::pkce_auth::AuthHandler;

#[derive(Debug, Display, Error)]
pub enum AuthError {
    Failed,
}

async fn token_file() -> Option<PathBuf> {
    cache_directory()
        .await
        .map(|cache| cache.join("refresh.token"))
}

/// Get the numtracker cache directory, ensuring it exists
async fn cache_directory() -> Option<PathBuf> {
    let cache = dirs::cache_dir()?.join("numtracker");
    let Ok(_) = fs::create_dir_all(&cache).await else {
        warn!("Couldn't create cache directory");
        return None;
    };
    trace!("Using cache directory: {cache:?}");
    Some(cache)
}

/// Save token to local directory - ignores errors
async fn save_refresh_token(token: &str) {
    trace!("Saving refresh token");
    let Some(dest) = token_file().await else {
        warn!("Cache directory not available");
        return;
    };

    if let Ok(mut file) = fs::File::create(&dest).await {
        _ = file.write(token.as_bytes()).await;
    }
}

async fn retrieve_refresh_token() -> Option<String> {
    trace!("Retrieving refresh token");
    fs::read_to_string(&token_file().await?).await.ok()
}

/// Retrieve a saved refresh token if there is one and use it to request a new access token
/// If a new access token is acquired, replace the saved refresh token as well
async fn refresh_access_token(auth: &AuthHandler) -> Option<String> {
    debug!("Trying to get access token via refresh");
    let refresh = retrieve_refresh_token().await?;
    let tokens = auth.refresh_flow(refresh).await?;
    if let Some(refr) = tokens.refresh_token() {
        save_refresh_token(refr.secret()).await;
    }
    Some(tokens.access_token().clone().into_secret())
}

/// Get a new access token from the auth server via the device flow.
/// If successful, cache the refresh token to prevent needing to log in next time
pub(crate) async fn get_access_token(h: &Url) -> Result<String, AuthError> {
    debug!("Getting new access token");
    let handler = AuthHandler::new(h.clone()).await.unwrap();
    if let Some(token) = refresh_access_token(&handler).await {
        return Ok(token);
    }
    let token = handler.device_flow().await;
    if let Some(refr) = token.refresh_token() {
        save_refresh_token(refr.secret()).await;
    }
    Ok(token.access_token().clone().into_secret())
}
