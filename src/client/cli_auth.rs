use std::path::PathBuf;

use derive_more::{Display, Error, From};
use openidconnect::core::{
    CoreAuthDisplay, CoreAuthPrompt, CoreClaimName, CoreClaimType, CoreClient,
    CoreClientAuthMethod, CoreDeviceAuthorizationResponse, CoreErrorResponseType, CoreGenderClaim,
    CoreGrantType, CoreJsonWebKey, CoreJweContentEncryptionAlgorithm,
    CoreJweKeyManagementAlgorithm, CoreJwsSigningAlgorithm, CoreResponseMode, CoreResponseType,
    CoreRevocableToken, CoreSubjectIdentifierType, CoreTokenType,
};
use openidconnect::{
    AdditionalProviderMetadata, AuthType, ClientId, DeviceAuthorizationUrl,
    DeviceCodeErrorResponseType, DiscoveryError, EmptyAdditionalClaims, EmptyExtraTokenFields,
    EndpointMaybeSet, EndpointNotSet, EndpointSet, HttpClientError, IdTokenFields, IssuerUrl,
    OAuth2TokenResponse, ProviderMetadata, RefreshToken, RequestTokenError,
    RevocationErrorResponseType, StandardErrorResponse, StandardTokenIntrospectionResponse,
    StandardTokenResponse,
};
use reqwest::redirect::Policy;
use serde::{Deserialize, Serialize};
use tokio::fs;
use tokio::io::AsyncWriteExt as _;
use tracing::{debug, trace, warn};
use url::Url;

#[derive(Clone, Debug, Deserialize, Serialize)]
struct DeviceEndpointProviderMetadata {
    device_authorization_endpoint: DeviceAuthorizationUrl,
}

impl AdditionalProviderMetadata for DeviceEndpointProviderMetadata {}

/// Metadata provided by well-known oidc endpoint, including fields required for device flow
// This is ludicrous
type DeviceProviderMetadata = ProviderMetadata<
    DeviceEndpointProviderMetadata,
    CoreAuthDisplay,
    CoreClientAuthMethod,
    CoreClaimName,
    CoreClaimType,
    CoreGrantType,
    CoreJweContentEncryptionAlgorithm,
    CoreJweKeyManagementAlgorithm,
    CoreJsonWebKey,
    CoreResponseMode,
    CoreResponseType,
    CoreSubjectIdentifierType,
>;

/// OIDC client capable of supporting the device flow for authentication
// This is ridiculous
type DeviceFlowClient = openidconnect::Client<
    EmptyAdditionalClaims,
    CoreAuthDisplay,
    CoreGenderClaim,
    CoreJweContentEncryptionAlgorithm,
    CoreJsonWebKey,
    CoreAuthPrompt,
    StandardErrorResponse<CoreErrorResponseType>,
    StandardTokenResponse<
        IdTokenFields<
            EmptyAdditionalClaims,
            EmptyExtraTokenFields,
            CoreGenderClaim,
            CoreJweContentEncryptionAlgorithm,
            CoreJwsSigningAlgorithm,
        >,
        CoreTokenType,
    >,
    StandardTokenIntrospectionResponse<EmptyExtraTokenFields, CoreTokenType>,
    CoreRevocableToken,
    StandardErrorResponse<RevocationErrorResponseType>,
    EndpointSet,
    EndpointSet,
    EndpointNotSet,
    EndpointNotSet,
    EndpointMaybeSet,
    EndpointMaybeSet,
>;

type HttpError = HttpClientError<reqwest::Error>;

pub struct AuthHandler {
    http: reqwest::Client,
    auth: DeviceFlowClient,
}

#[derive(Debug, Display, Error, From)]
pub enum AuthError {
    Http(reqwest::Error),
    Discovery(DiscoveryError<HttpError>),
    DeviceFlowInit(RequestTokenError<HttpError, StandardErrorResponse<CoreErrorResponseType>>),
    AccessRequest(RequestTokenError<HttpError, StandardErrorResponse<DeviceCodeErrorResponseType>>),
    Oidc(openidconnect::ConfigurationError),
    NoVerificationUrl,
}

impl AuthHandler {
    pub async fn new(host: impl Into<Url>) -> Result<Self, AuthError> {
        let http_client = reqwest::ClientBuilder::new()
            .redirect(Policy::none())
            .build()?;
        let meta_provider =
            DeviceProviderMetadata::discover_async(IssuerUrl::from_url(host.into()), &http_client)
                .await?;
        let device_authorization_url = meta_provider
            .additional_metadata()
            .device_authorization_endpoint
            .clone();
        let client = CoreClient::from_provider_metadata(
            meta_provider,
            ClientId::new("numtracker".to_string()),
            None,
        )
        .set_device_authorization_url(device_authorization_url)
        .set_auth_type(AuthType::RequestBody);
        Ok(Self {
            http: http_client,
            auth: client,
        })
    }

    pub async fn device_flow(&self) -> Result<impl OAuth2TokenResponse, AuthError> {
        let details: CoreDeviceAuthorizationResponse = self
            .auth
            .exchange_device_code()
            .request_async(&self.http)
            .await?;

        println!(
            "Visit: {}",
            details
                .verification_uri_complete()
                .ok_or(AuthError::NoVerificationUrl)?
                .secret()
        );

        let token = self
            .auth
            .exchange_device_access_token(&details)?
            .request_async(&self.http, tokio::time::sleep, None)
            .await?;

        Ok(token)
    }

    pub async fn refresh_flow(&self, token: String) -> Option<impl OAuth2TokenResponse> {
        self.auth
            .exchange_refresh_token(&RefreshToken::new(token))
            .ok()?
            .request_async(&self.http)
            .await
            .ok()
    }
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
    let handler = AuthHandler::new(h.clone()).await?;
    if let Some(token) = refresh_access_token(&handler).await {
        return Ok(token);
    }
    let token = handler.device_flow().await?;
    if let Some(refr) = token.refresh_token() {
        save_refresh_token(refr.secret()).await;
    }
    Ok(token.access_token().clone().into_secret())
}
