use graphql_client::{GraphQLQuery, Response};

use crate::cli::client::{ClientCommand, ClientOptions, ConfigurationOptions, ConnectionOptions};

mod auth;

pub async fn run_client(options: ClientOptions) {
    let ClientOptions {
        connection,
        command,
    } = options;
    match command {
        ClientCommand::Configuration { beamline } => {
            query_configuration(beamline, connection).await
        }
        ClientCommand::Configure {
            beamline,
            config,
            // visit,
            // scan,
            // detector,
            // scan_number,
            // tracker_file_extension,
        } => {
            configure_beamline(
                beamline, config,
                // visit,
                // scan,
                // detector,
                // scan_number,
                // tracker_file_extension,
                connection,
            )
            .await
        }
        ClientCommand::Paths { beamline, visit } => {
            query_visit_directory(beamline, visit, connection).await
        }
        ClientCommand::Scan {
            beamline,
            visit,
            subdirectory,
            detectors,
        } => todo!(),
    }
}

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "static/service_schema.graphql",
    query_path = "queries/configuration.graphql",
    response_derives = "Debug"
)]
struct ConfigurationQuery;

async fn query_configuration(beamline: Option<Vec<String>>, options: ConnectionOptions) {
    let vars = configuration_query::Variables { beamline };
    let request = ConfigurationQuery::build_query(vars);
    let resp = reqwest::Client::new()
        .post(options.host().join("/graphql").unwrap())
        .json(&request)
        .send()
        .await
        .unwrap();

    let data: Response<configuration_query::ResponseData> = resp.json().await.unwrap();
    println!("{data:#?}");
}

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "static/service_schema.graphql",
    query_path = "queries/path.graphql",
    response_derives = "Debug"
)]
struct PathQuery;

async fn query_visit_directory(beamline: String, visit: String, options: ConnectionOptions) {
    let vars = path_query::Variables { beamline, visit };
    let request = PathQuery::build_query(vars);
    let resp = reqwest::Client::new()
        .post(options.host().join("/graphql").unwrap())
        .json(&request)
        .send()
        .await
        .unwrap();

    let data: Response<path_query::ResponseData> = resp.json().await.unwrap();
    println!("{data:#?}");
}
// use crate::graphql::InputTemplate;
// use crate::paths; //::{DetectorTemplate, ScanTemplate, VisitTemplate};
// type VisitTemplate = InputTemplate<paths::VisitTemplate>;
// type ScanTemplate = InputTemplate<paths::ScanTemplate>;
// type DetectorTemplate = InputTemplate<paths::DetectorTemplate>;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "static/service_schema.graphql",
    query_path = "queries/configure.graphql",
    response_derives = "Debug"
)]
struct ConfigureMutation;

async fn configure_beamline(
    beamline: String,
    config: ConfigurationOptions,
    // visit: Option<String>,
    // scan: Option<String>,
    // detector: Option<String>,
    // scan_number: Option<i64>,
    // tracker_file_extension: Option<String>,
    options: ConnectionOptions,
) {
    let vars = configure_mutation::Variables {
        beamline,
        scan: config.scan,
        visit: config.visit,
        detector: config.detector,
        scan_number: config.scan_number,
        ext: config.tracker_file_extension,
    };
    let request = ConfigureMutation::build_query(vars);
    let resp = reqwest::Client::new()
        .post(options.host().join("/graphql").unwrap())
        .json(&request)
        .send()
        .await
        .unwrap();
    let data: Response<configure_mutation::ResponseData> = resp.json().await.unwrap();
    println!("{data:#?}");
}
