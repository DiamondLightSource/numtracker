use cli_auth::AuthError;
use config::ClientConfiguration;
use derive_more::{Display, Error, From};
use graphql_client::{GraphQLQuery, Response};
use reqwest::Client;
use serde::de::DeserializeOwned;
use serde::Serialize;
use url::Url;

use crate::cli::client::{ClientCommand, ClientOptions, ConfigurationOptions};

mod cli_auth;
mod config;

#[derive(Debug, Display, Error, From)]
pub enum ClientError {
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
        ClientCommand::VisitDirectory { beamline, visit } => {
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
        let host = config
            .host
            .unwrap_or(Url::parse("http://localhost:8000").expect("Constant URL is valid"));

        let auth = match config.auth {
            Some(auth) => Some(cli_auth::get_access_token(&auth).await?),
            None => None,
        };
        Ok(NumtrackerClient { auth, host })
    }

    async fn request<Query: Serialize, Data: DeserializeOwned>(
        &self,
        content: Query,
    ) -> Result<Response<Data>, reqwest::Error> {
        let client = Client::new().post(
            self.host
                .join("/graphql")
                .expect("Appending to URL should be fine"),
        );
        let client = match self.auth.as_ref() {
            None => client,
            Some(token) => client.bearer_auth(token),
        };
        client.json(&content).send().await?.json().await
    }

    async fn query_configuration(self, instrument: Option<Vec<String>>) -> Result<(), ClientError> {
        let vars = configuration_query::Variables { instrument };
        let request = ConfigurationQuery::build_query(vars);
        let data = self
            .request::<_, configuration_query::ResponseData>(request)
            .await?;
        print_errors(data.errors.as_deref());
        if let Some(configs) = data.data {
            for conf in configs.configurations {
                println!("Beamline: {}", conf.instrument);
                println!("    Visit Template: {}", conf.directory_template);
                println!("    Scan Template: {}", conf.scan_template);
                println!("    Detector Template: {}", conf.detector_template);
                println!("    DB Scan Number: {}", conf.db_scan_number);
                match conf.file_scan_number {
                    Some(file_num) => println!("    File Scan Number: {file_num}"),
                    None => println!("    File Scan Number: Not Available"),
                }
                println!(
                    "    Tracker File Extension: {}",
                    conf.tracker_file_extension.unwrap_or(conf.instrument)
                );
            }
        }
        Ok(())
    }

    async fn query_visit_directory(
        self,
        instrument: String,
        instrument_session: String,
    ) -> Result<(), ClientError> {
        let vars = path_query::Variables {
            instrument,
            instrument_session,
        };
        let request = PathQuery::build_query(vars);
        let data = self.request::<_, path_query::ResponseData>(request).await?;

        print_errors(data.errors.as_deref());
        match data.data {
            Some(data) => println!("{}", data.paths.path),
            None => println!("No paths returned from server"),
        }
        Ok(())
    }

    async fn configure_beamline(
        self,
        instrument: String,
        config: ConfigurationOptions,
    ) -> Result<(), ClientError> {
        let vars = configure_mutation::Variables {
            instrument,
            scan: config.scan,
            directory: config.directory,
            detector: config.detector,
            scan_number: config.scan_number,
            ext: config.tracker_file_extension,
        };
        let request = ConfigureMutation::build_query(vars);
        let data = self
            .request::<_, configure_mutation::ResponseData>(request)
            .await?;

        print_errors(data.errors.as_deref());
        match data.data {
            Some(data) => {
                let conf = data.configure;
                println!("Visit Template: {}", conf.directory_template);
                println!("Scan Template: {}", conf.scan_template);
                println!("Detector Template: {}", conf.detector_template);
                println!("DB Scan Number: {}", conf.db_scan_number);
                match conf.file_scan_number {
                    Some(file_num) => println!("File Scan Number: {file_num}"),
                    None => println!("File Scan Number: Not Available"),
                }
                println!(
                    "Tracker File Extension: {}",
                    conf.tracker_file_extension.as_deref().unwrap_or("None")
                );
            }
            None => println!("No configuration returned from server"),
        }
        Ok(())
    }
}

fn print_errors(errors: Option<&[graphql_client::Error]>) {
    if let Some(errors) = errors {
        println!("Query returned errors:");
        for err in errors {
            println!("    {err}");
        }
    }
}
