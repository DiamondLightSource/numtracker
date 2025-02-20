use openidconnect::core::{
    CoreAuthDisplay, CoreClaimName, CoreClaimType, CoreClient, CoreClientAuthMethod,
    CoreDeviceAuthorizationResponse, CoreGrantType, CoreJsonWebKey,
    CoreJweContentEncryptionAlgorithm, CoreJweKeyManagementAlgorithm, CoreResponseMode,
    CoreResponseType, CoreSubjectIdentifierType,
};
use openidconnect::{
    AdditionalProviderMetadata, AuthType, ClientId, DeviceAuthorizationUrl, IssuerUrl,
    OAuth2TokenResponse, ProviderMetadata, RefreshToken,
};
use reqwest::redirect::Policy;
use serde::{Deserialize, Serialize};
use url::Url;

#[derive(Clone, Debug, Deserialize, Serialize)]
struct DeviceEndpointProviderMetadata {
    device_authorization_endpoint: DeviceAuthorizationUrl,
}
impl AdditionalProviderMetadata for DeviceEndpointProviderMetadata {}
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

pub async fn device_flow(auth_host: &Url) -> impl OAuth2TokenResponse {
    let http_client = reqwest::ClientBuilder::new()
        .redirect(Policy::none())
        .build()
        .unwrap();
    let meta_provider = DeviceProviderMetadata::discover_async(
        IssuerUrl::from_url(auth_host.clone()),
        &http_client,
    )
    .await
    .unwrap();

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

    let details: CoreDeviceAuthorizationResponse = client
        .exchange_device_code()
        .request_async(&http_client)
        .await
        .unwrap();

    println!(
        "Visit: {}",
        details.verification_uri_complete().unwrap().secret()
    );

    let token = client
        .exchange_device_access_token(&details)
        .unwrap()
        .request_async(&http_client, tokio::time::sleep, None)
        .await
        .unwrap();

    token
}

pub async fn refresh_flow(auth_host: &Url, token: String) -> Option<impl OAuth2TokenResponse> {
    let http_client = reqwest::ClientBuilder::new()
        .redirect(Policy::none())
        .build()
        .unwrap();
    let meta_provider = DeviceProviderMetadata::discover_async(
        IssuerUrl::from_url(auth_host.clone()),
        &http_client,
    )
    .await
    .unwrap();

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

    client
        .exchange_refresh_token(&RefreshToken::new(token))
        .unwrap()
        .request_async(&http_client)
        .await
        .ok()
}
