// Copyright 2024 Diamond Light Source
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use std::str::FromStr;

use async_graphql::{Context, Guard};
use axum_extra::headers::authorization::Bearer;
use axum_extra::headers::Authorization;
use derive_more::{Display, Error, From};
use serde::{Deserialize, Serialize};
use tracing::{info, trace};

use crate::cli::PolicyOptions;

const AUDIENCE: &str = "account";

type Token = Authorization<Bearer>;

#[derive(Debug, Serialize)]
struct Request<T> {
    input: T,
}

#[derive(Debug, Deserialize)]
#[cfg_attr(test, derive(Serialize))]
struct Response {
    result: bool,
}

#[derive(Debug, Serialize)]
pub struct AccessRequest<'a> {
    token: &'a str,
    audience: &'a str,
    proposal: u32,
    // This should stay as visit instead of instrument session number until the
    // rules in the authz service are updated.
    visit: u16,
    // This should stay as beamline instead of instrument until the rules in the authz service are
    // updated to use instrument
    beamline: &'a str,
}

impl<'a> AccessRequest<'a> {
    fn new(
        token: Option<&'a Token>,
        instrument_session: InstrumentSession,
        instrument: &'a str,
    ) -> Result<Self, AuthError> {
        Ok(Self {
            token: token.ok_or(AuthError::Missing)?.token(),
            audience: AUDIENCE,
            proposal: instrument_session.proposal,
            visit: instrument_session.session,
            beamline: instrument,
        })
    }
}

#[derive(Debug, Serialize)]
pub struct AdminRequest<'a> {
    token: &'a str,
    audience: &'a str,
    // This should stay as beamline instead of instrument until the rules in the authz service are
    // updated to use instrument
    #[serde(skip_serializing_if = "Option::is_none")]
    beamline: Option<&'a str>,
}

impl<'r> AdminRequest<'r> {
    fn new(token: Option<&'r Token>, instrument: Option<&'r str>) -> Result<Self, AuthError> {
        Ok(Self {
            token: token.ok_or(AuthError::Missing)?.token(),
            audience: AUDIENCE,
            beamline: instrument,
        })
    }
}

#[derive(Debug)]
struct InvalidInstrumentSession;

#[cfg_attr(test, derive(Debug))]
struct InstrumentSession {
    proposal: u32,
    session: u16,
}
impl FromStr for InstrumentSession {
    type Err = InvalidInstrumentSession;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (code_prop, vis) = s.split_once('-').ok_or(InvalidInstrumentSession)?;
        let prop = code_prop
            .chars()
            .skip_while(|p| !p.is_ascii_digit())
            .collect::<String>();
        let proposal = prop.parse().map_err(|_| InvalidInstrumentSession)?;
        let session = vis.parse().map_err(|_| InvalidInstrumentSession)?;
        Ok(Self { proposal, session })
    }
}

pub(crate) struct PolicyCheck {
    client: reqwest::Client,
    /// Rego query for getting admin rights
    admin: String,
    /// Rego query for getting access rights
    access: String,
}

impl PolicyCheck {
    pub fn new(endpoint: PolicyOptions) -> Self {
        info!(
            "Checking authorization against {:?} using {:?} for admin and {:?} for access",
            endpoint.policy_host, endpoint.admin_query, endpoint.access_query
        );
        Self {
            client: reqwest::Client::new(),
            admin: format!("{}/{}", endpoint.policy_host, endpoint.admin_query),
            access: format!("{}/{}", endpoint.policy_host, &endpoint.access_query),
        }
    }
    pub async fn check_access(
        &self,
        token: Option<&Authorization<Bearer>>,
        instrument: &str,
        instrument_session: &str,
    ) -> Result<(), AuthError> {
        let session: InstrumentSession =
            instrument_session.parse().map_err(|_| AuthError::Failed)?;
        self.authorise(
            &self.access,
            AccessRequest::new(token, session, instrument)?,
        )
        .await
    }

    pub async fn check_admin(
        &self,
        token: Option<&Authorization<Bearer>>,
    ) -> Result<(), AuthError> {
        self.authorise(&self.admin, AdminRequest::new(token, None)?)
            .await
    }

    pub async fn check_instrument_admin(
        &self,
        token: Option<&Authorization<Bearer>>,
        instrument: &str,
    ) -> Result<(), AuthError> {
        self.authorise(&self.admin, AdminRequest::new(token, Some(instrument))?)
            .await
    }

    async fn authorise(&self, query: &str, input: impl Serialize) -> Result<(), AuthError> {
        let response = self
            .client
            .post(query)
            .json(&Request { input })
            .send()
            .await?;
        if response.json::<Response>().await?.result {
            Ok(())
        } else {
            Err(AuthError::Failed)
        }
    }
}

#[derive(Debug)]
pub(crate) enum AuthGuard<'a> {
    Access {
        instrument: &'a str,
        instrument_session: &'a str,
    },
    InstrumentAdmin {
        instrument: &'a str,
    },
    Admin,
}

impl<'a> Guard for AuthGuard<'a> {
    async fn check(&self, ctx: &Context<'_>) -> async_graphql::Result<()> {
        if let Some(policy) = ctx.data::<Option<PolicyCheck>>()? {
            trace!("Auth enabled: checking token");
            let token = ctx.data::<Option<Authorization<Bearer>>>()?;
            let check = match self {
                AuthGuard::Access {
                    instrument,
                    instrument_session,
                } => {
                    policy
                        .check_access(token.as_ref(), instrument, instrument_session)
                        .await
                }
                AuthGuard::InstrumentAdmin { instrument } => {
                    policy
                        .check_instrument_admin(token.as_ref(), instrument)
                        .await
                }
                AuthGuard::Admin => policy.check_admin(token.as_ref()).await,
            };
            check
                .inspect_err(|e| info!("Authorization failed: {e:?}"))
                .map_err(async_graphql::Error::from)
        } else {
            trace!("No authorization configured");
            Ok(())
        }
    }
}

#[derive(Debug, Display, Error, From)]
pub enum AuthError {
    #[display("Invalid authorization configuration")]
    ServerError(reqwest::Error),
    #[display("Authentication failed")]
    Failed,
    #[display("No authentication token was provided")]
    Missing,
}

#[cfg(test)]
mod tests {
    use std::str::FromStr as _;

    use assert_matches::assert_matches;
    use axum::http::HeaderValue;
    use axum_extra::headers::authorization::{Bearer, Credentials};
    use axum_extra::headers::Authorization;
    use httpmock::MockServer;
    use rstest::rstest;
    use serde_json::json;

    use super::{AuthError, InstrumentSession, InvalidInstrumentSession, PolicyCheck};
    use crate::cli::PolicyOptions;

    fn token(name: &'static str) -> Option<Authorization<Bearer>> {
        Some(Authorization(
            Bearer::decode(&HeaderValue::from_str(&format!("Bearer {name}")).unwrap()).unwrap(),
        ))
    }

    #[test]
    fn valid_instrument_session() {
        let session = InstrumentSession::from_str("cm12345-1").unwrap();
        assert_eq!(session.session, 1);
        assert_eq!(session.proposal, 12345);
    }

    #[rstest]
    #[case::no_proposal("cm-3")]
    #[case::no_session("cm12345")]
    #[case::invalid_session("cm12345-abc")]
    #[case::invalid_proposal("cm123abc-12")]
    #[case::negative_session("cm1234--12")]
    fn invalid_instrument_session(#[case] instrument_session: &str) {
        assert_matches!(
            InstrumentSession::from_str(instrument_session),
            Err(InvalidInstrumentSession)
        )
    }

    #[tokio::test]
    async fn successful_check_access() {
        let server = MockServer::start();
        let mock = server
            .mock_async(|when, then| {
                when.method("POST")
                    .path("/demo/access")
                    .json_body_obj(&json!({
                        "input": {
                            "token": "token",
                            "beamline": "i22",
                            "visit": 4,
                            "proposal": 1234,
                            "audience": "account"
                        }
                    }));
                then.status(200).json_body_obj(&json!({"result": true}));
            })
            .await;
        let check = PolicyCheck::new(PolicyOptions {
            policy_host: server.url(""),
            access_query: "demo/access".into(),
            admin_query: "demo/admin".into(),
        });
        check
            .check_access(token("token").as_ref(), "i22", "cm1234-4")
            .await
            .unwrap();
        mock.assert();
    }

    #[tokio::test]
    async fn successful_check_instrument_admin() {
        let server = MockServer::start();
        let mock = server
            .mock_async(|when, then| {
                when.method("POST")
                    .path("/demo/admin")
                    .json_body_obj(&json!({
                        "input": {
                            "token": "token",
                            "beamline": "i22",
                            "audience": "account"
                        }
                    }));
                then.status(200).json_body_obj(&json!({"result": true}));
            })
            .await;
        let check = PolicyCheck::new(PolicyOptions {
            policy_host: server.url(""),
            access_query: "demo/access".into(),
            admin_query: "demo/admin".into(),
        });
        check
            .check_instrument_admin(token("token").as_ref(), "i22")
            .await
            .unwrap();
        mock.assert();
    }

    #[tokio::test]
    async fn successful_check_admin() {
        let server = MockServer::start();
        let mock = server
            .mock_async(|when, then| {
                when.method("POST")
                    .path("/demo/admin")
                    .json_body_obj(&json!({
                        "input": {
                            "token": "token",
                            "audience": "account"
                        }
                    }));
                then.status(200).json_body_obj(&json!({"result": true}));
            })
            .await;
        let check = PolicyCheck::new(PolicyOptions {
            policy_host: server.url(""),
            access_query: "demo/access".into(),
            admin_query: "demo/admin".into(),
        });
        check.check_admin(token("token").as_ref()).await.unwrap();
        mock.assert();
    }

    #[tokio::test]
    async fn denied_check_access() {
        let server = MockServer::start();
        let mock = server
            .mock_async(|when, then| {
                when.method("POST")
                    .path("/demo/access")
                    .json_body_obj(&json!({
                        "input": {
                            "token": "token",
                            "beamline": "i22",
                            "proposal": 1234,
                            "visit": 4,
                            "audience": "account"
                        }
                    }));
                then.status(200).json_body_obj(&json!({"result": false}));
            })
            .await;
        let check = PolicyCheck::new(PolicyOptions {
            policy_host: server.url(""),
            access_query: "demo/access".into(),
            admin_query: "demo/admin".into(),
        });

        let result = check
            .check_access(token("token").as_ref(), "i22", "cm1234-4")
            .await;
        let Err(AuthError::Failed) = result else {
            panic!("Unexpected result from unauthorised check: {result:?}");
        };
        mock.assert();
    }

    #[tokio::test]
    async fn denied_check_instrument_admin() {
        let server = MockServer::start();
        let mock = server
            .mock_async(|when, then| {
                when.method("POST")
                    .path("/demo/admin")
                    .json_body_obj(&json!({
                        "input": {
                            "token": "token",
                            "beamline": "i22",
                            "audience": "account"
                        }
                    }));
                then.status(200).json_body_obj(&json!({"result": false}));
            })
            .await;
        let check = PolicyCheck::new(PolicyOptions {
            policy_host: server.url(""),
            access_query: "demo/access".into(),
            admin_query: "demo/admin".into(),
        });
        let result = check
            .check_instrument_admin(token("token").as_ref(), "i22")
            .await;
        let Err(AuthError::Failed) = result else {
            panic!("Unexpected result from unauthorised check: {result:?}");
        };
        mock.assert();
    }

    #[tokio::test]
    async fn denied_check_admin() {
        let server = MockServer::start();
        let mock = server
            .mock_async(|when, then| {
                when.method("POST")
                    .path("/demo/admin")
                    .json_body_obj(&json!({
                        "input": {
                            "token": "token",
                            "audience": "account"
                        }
                    }));
                then.status(200).json_body_obj(&json!({"result": false}));
            })
            .await;
        let check = PolicyCheck::new(PolicyOptions {
            policy_host: server.url(""),
            access_query: "demo/access".into(),
            admin_query: "demo/admin".into(),
        });
        let result = check.check_admin(token("token").as_ref()).await;
        let Err(AuthError::Failed) = result else {
            panic!("Unexpected result from unauthorised check: {result:?}");
        };
        mock.assert();
    }

    #[tokio::test]
    async fn unauthorised_check_access() {
        let server = MockServer::start();
        let mock = server
            .mock_async(|_, _| {
                // mock that rejects every request
            })
            .await;
        let check = PolicyCheck::new(PolicyOptions {
            policy_host: server.url(""),
            access_query: "demo/access".into(),
            admin_query: "demo/admin".into(),
        });
        let result = check.check_access(None, "i22", "cm1234-4").await;
        let Err(AuthError::Missing) = result else {
            panic!("Unexpected result from unauthorised check: {result:?}");
        };
        mock.assert_calls(0);
    }

    #[tokio::test]
    async fn unauthorised_check_instrument_admin() {
        let server = MockServer::start();
        let mock = server
            .mock_async(|_, _| {
                // mock that rejects every request
            })
            .await;
        let check = PolicyCheck::new(PolicyOptions {
            policy_host: server.url(""),
            access_query: "demo/access".into(),
            admin_query: "demo/admin".into(),
        });
        let result = check.check_instrument_admin(None, "i22").await;
        let Err(AuthError::Missing) = result else {
            panic!("Unexpected result from unauthorised check: {result:?}");
        };
        mock.assert_calls(0);
    }

    #[tokio::test]
    async fn unauthorised_check_admin() {
        let server = MockServer::start();
        let mock = server
            .mock_async(|_, _| {
                // mock that rejects every request
            })
            .await;
        let check = PolicyCheck::new(PolicyOptions {
            policy_host: server.url(""),
            access_query: "demo/access".into(),
            admin_query: "demo/admin".into(),
        });
        let result = check.check_admin(None).await;
        let Err(AuthError::Missing) = result else {
            panic!("Unexpected result from unauthorised check: {result:?}");
        };
        mock.assert_calls(0);
    }

    #[tokio::test]
    async fn server_error() {
        let server = MockServer::start();
        let mock = server
            .mock_async(|when, then| {
                when.method("POST");
                then.status(503);
            })
            .await;
        let check = PolicyCheck::new(PolicyOptions {
            policy_host: server.url(""),
            access_query: "demo/access".into(),
            admin_query: "demo/admin".into(),
        });
        let result = check
            .check_instrument_admin(token("token").as_ref(), "i22")
            .await;
        let Err(AuthError::ServerError(_)) = result else {
            panic!("Unexpected result from unauthorised check: {result:?}");
        };
        mock.assert();
    }
}
