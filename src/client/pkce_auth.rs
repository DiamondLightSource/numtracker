use openidconnect::core::{
    CoreAuthDisplay, CoreAuthPrompt, CoreClaimName, CoreClaimType, CoreClient,
    CoreClientAuthMethod, CoreDeviceAuthorizationResponse, CoreErrorResponseType, CoreGenderClaim,
    CoreGrantType, CoreJsonWebKey, CoreJweContentEncryptionAlgorithm,
    CoreJweKeyManagementAlgorithm, CoreJwsSigningAlgorithm, CoreResponseMode, CoreResponseType,
    CoreRevocableToken, CoreSubjectIdentifierType, CoreTokenType,
};
use openidconnect::{
    AdditionalProviderMetadata, AuthType, ClientId, DeviceAuthorizationUrl, EmptyAdditionalClaims,
    EmptyExtraTokenFields, EndpointMaybeSet, EndpointNotSet, EndpointSet, IdTokenFields, IssuerUrl,
    OAuth2TokenResponse, ProviderMetadata, RefreshToken, RevocationErrorResponseType,
    StandardErrorResponse, StandardTokenIntrospectionResponse, StandardTokenResponse,
};
use reqwest::redirect::Policy;
use serde::{Deserialize, Serialize};
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

pub struct AuthHandler {
    http: reqwest::Client,
    auth: DeviceFlowClient,
}

impl AuthHandler {
    pub async fn new(host: impl Into<Url>) -> Result<Self, ()> {
        let http_client = reqwest::ClientBuilder::new()
            .redirect(Policy::none())
            .build()
            .unwrap();
        let meta_provider =
            DeviceProviderMetadata::discover_async(IssuerUrl::from_url(host.into()), &http_client)
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
        Ok(Self {
            http: http_client,
            auth: client,
        })
    }
    pub async fn device_flow(&self) -> impl OAuth2TokenResponse {
        let details: CoreDeviceAuthorizationResponse = self
            .auth
            .exchange_device_code()
            .request_async(&self.http)
            .await
            .unwrap();

        println!(
            "Visit: {}",
            details.verification_uri_complete().unwrap().secret()
        );

        let token = self
            .auth
            .exchange_device_access_token(&details)
            .unwrap()
            .request_async(&self.http, tokio::time::sleep, None)
            .await
            .unwrap();

        token
    }

    pub async fn refresh_flow(&self, token: String) -> Option<impl OAuth2TokenResponse> {
        self.auth
            .exchange_refresh_token(&RefreshToken::new(token))
            .unwrap()
            .request_async(&self.http)
            .await
            .ok()
    }
}
