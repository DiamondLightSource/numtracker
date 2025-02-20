use config::ClientConfiguration;
use derive_more::{Display, Error, From};
use graphql_client::{GraphQLQuery, Response};
use pkce_auth::AuthError;
use reqwest::Client;
use serde::de::DeserializeOwned;
use serde::Serialize;
use url::Url;

use crate::cli::client::{ClientCommand, ClientOptions, ConfigurationOptions};

mod cli_auth;
mod config;
mod pkce_auth;
#[derive(Debug, Display, Error, From)]
pub enum ClientConfigurationError {
    MissingHost,
    AuthError(AuthError),
}

#[derive(Debug, Display, Error, From)]
pub enum ClientError {
    Config(ClientConfigurationError),
    Auth(AuthError),
    Network(reqwest::Error),
}

pub async fn run_client(options: ClientOptions) {
    let ClientOptions {
        connection,
        command,
    } = options;

    let conf = match ClientConfiguration::from_default_file().await {
        Ok(conf) => conf.with_host(connection.host).with_auth(connection.auth),
        Err(e) => {
            println!("Could not read configuration: {e}");
            return;
        }
    };

    let client = match NumtrackerClient::from_config(conf).await {
        Ok(client) => client,
        Err(e) => {
            println!("Error initialising client: {e}");
            return;
        }
    };

    let result = match command {
        ClientCommand::Configuration { beamline } => client.query_configuration(beamline).await,
        ClientCommand::Configure { beamline, config } => {
            client.configure_beamline(beamline, config).await
        }
        ClientCommand::Paths { beamline, visit } => {
            client.query_visit_directory(beamline, visit).await
        }
    };

    if let Err(e) = result {
        println!("Error querying service: {e}");
    }
}

struct NumtrackerClient {
    auth: Option<String>,
    host: Url,
}

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "static/service_schema.graphql",
    query_path = "queries/configuration.graphql",
    response_derives = "Debug"
)]
struct ConfigurationQuery;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "static/service_schema.graphql",
    query_path = "queries/path.graphql",
    response_derives = "Debug"
)]
struct PathQuery;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "static/service_schema.graphql",
    query_path = "queries/configure.graphql",
    response_derives = "Debug"
)]
struct ConfigureMutation;

impl NumtrackerClient {
    async fn from_config(config: ClientConfiguration) -> Result<Self, ClientError> {
        let host = config.host.ok_or(ClientConfigurationError::MissingHost)?;

        let auth = match config.auth {
            Some(auth) => Some(cli_auth::get_access_token(&auth).await?),
            None => None,
        };
        Ok(NumtrackerClient { auth, host })
    }

    async fn request<Query: Serialize, Data: DeserializeOwned>(
        &self,
        content: Query,
    ) -> Result<Option<Data>, reqwest::Error> {
        let client = Client::new().post(
            self.host
                .join("/graphql")
                .expect("Appending to URL should be fine"),
        );
        let client = match self.auth.as_ref() {
            None => client,
            Some(token) => client.bearer_auth(token),
        };
        let response: Response<Data> = client.json(&content).send().await?.json().await?;
        dbg!(&response.errors);
        Ok(response.data)
    }

    async fn query_configuration(self, beamline: Option<Vec<String>>) -> Result<(), ClientError> {
        let vars = configuration_query::Variables { beamline };
        let request = ConfigurationQuery::build_query(vars);
        let data = self
            .request::<_, configuration_query::ResponseData>(request)
            .await?;
        println!("{data:#?}");
        Ok(())
    }

    async fn query_visit_directory(
        self,
        beamline: String,
        visit: String,
    ) -> Result<(), ClientError> {
        let vars = path_query::Variables { beamline, visit };
        let request = PathQuery::build_query(vars);
        let data = self.request::<_, path_query::ResponseData>(request).await?;

        println!("{data:#?}");
        Ok(())
    }

    async fn configure_beamline(
        self,
        beamline: String,
        config: ConfigurationOptions,
    ) -> Result<(), ClientError> {
        let vars = configure_mutation::Variables {
            beamline,
            scan: config.scan,
            visit: config.visit,
            detector: config.detector,
            scan_number: config.scan_number,
            ext: config.tracker_file_extension,
        };
        let request = ConfigureMutation::build_query(vars);
        let data = self
            .request::<_, configure_mutation::ResponseData>(request)
            .await?;
        println!("{data:#?}");
        Ok(())
    }
}
