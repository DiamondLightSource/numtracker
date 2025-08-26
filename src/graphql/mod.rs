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

use std::any;
use std::borrow::Cow;
use std::collections::HashMap;
use std::future::Future;
use std::io::Write;
use std::path::{Component, PathBuf};

use async_graphql::extensions::Tracing;
use async_graphql::http::GraphiQLSource;
use async_graphql::{
    Context, Description, EmptySubscription, InputObject, InputValueError, InputValueResult,
    Object, Scalar, ScalarType, Schema, SimpleObject, TypeName, Value,
};
use async_graphql_axum::{GraphQLRequest, GraphQLResponse};
use auth::{AuthError, PolicyCheck};
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse};
use axum::routing::{get, post};
use axum::{Extension, Json, Router};
use axum_extra::headers::authorization::Bearer;
use axum_extra::headers::Authorization;
use axum_extra::TypedHeader;
use chrono::{Datelike, Local};
use derive_more::{Display, Error};
use tokio::net::TcpListener;
use tracing::{debug, info, instrument, trace, warn};

use crate::build_info::ServerStatus;
use crate::cli::ServeOptions;
use crate::db_service::{
    InstrumentConfiguration, InstrumentConfigurationUpdate, SqliteScanPathService,
};
use crate::numtracker::NumTracker;
use crate::paths::{
    DetectorField, DetectorTemplate, DirectoryField, DirectoryTemplate, PathSpec, ScanField,
    ScanTemplate,
};
use crate::template::{FieldSource, PathTemplate};

mod auth;

pub async fn serve_graphql(opts: ServeOptions) {
    debug!(?opts, "Starting numtracker service");
    let server_status = Json(ServerStatus::new());
    let db = SqliteScanPathService::connect(&opts.db)
        .await
        .expect("Unable to open DB");
    let directory_numtracker = NumTracker::for_root_directory(opts.root_directory())
        .expect("Could not read external directories");
    info!("Serving graphql endpoints on {:?}", opts.addr());
    let addr = opts.addr();
    let schema = Schema::build(Query, Mutation, EmptySubscription)
        .extension(Tracing)
        .limit_directives(32)
        .data(db)
        .data(directory_numtracker)
        .data(opts.policy.map(PolicyCheck::new))
        .finish();
    let app = Router::new()
        // status check endpoint allows external processes to monitor status of server without
        // making graphql queries
        .route("/status", get(server_status))
        .route("/graphql", post(graphql_handler))
        // make it obvious that /graphql isn't expected to work when visiting from a browser
        .route(
            "/graphql",
            get((
                StatusCode::METHOD_NOT_ALLOWED,
                [("Allow", "POST")],
                Html(include_str!("../../static/get_graphql_warning.html")),
            )),
        )
        // Interactive graphiql playground
        .route("/graphiql", get(graphiql))
        // Make it look less like something is broken when going to any other page
        .fallback((
            StatusCode::NOT_FOUND,
            Html(include_str!("../../static/404.html")),
        ))
        .layer(Extension(schema));
    let listener = TcpListener::bind(addr)
        .await
        .unwrap_or_else(|e| panic!("Could not listen on {:?}:{}: {e}", addr.0, addr.1));
    axum::serve(listener, app)
        .await
        .expect("Can't serve graphql endpoint");
}

pub fn graphql_schema<W: Write>(mut out: W) -> Result<(), std::io::Error> {
    let schema = Schema::new(Query, Mutation, EmptySubscription);
    write!(out, "{}", schema.sdl())
}

async fn graphiql() -> impl IntoResponse {
    Html(GraphiQLSource::build().endpoint("/graphql").finish())
}

#[instrument(skip_all)]
async fn graphql_handler(
    schema: Extension<Schema<Query, Mutation, EmptySubscription>>,
    auth_token: Option<TypedHeader<Authorization<Bearer>>>,
    req: GraphQLRequest,
) -> GraphQLResponse {
    schema
        .execute(req.into_inner().data(auth_token.map(|header| header.0)))
        .await
        .into()
}

/// Read-only API for GraphQL
struct Query;

/// Read-write API for GraphQL
struct Mutation;

/// GraphQL type to mimic a key-value pair from the map type that GraphQL doesn't have
#[derive(SimpleObject)]
struct DetectorPath {
    /// The name of the detector that should use this path
    name: String,
    /// The path where the detector should write its data
    path: String,
}

/// GraphQL type to provide directory data for a specific instrument session
struct DirectoryPath {
    instrument_session: String,
    info: InstrumentConfiguration,
}

/// GraphQL type to provide path data for the next scan for a given instrument session
struct ScanPaths {
    directory: DirectoryPath,
    subdirectory: Subdirectory,
    metadata: HashMap<String, String>,
}

/// GraphQL type to provide current configuration for an instrument
struct CurrentConfiguration {
    db_config: InstrumentConfiguration,
    high_file: Option<u32>,
}

#[derive(Debug, InputObject)]
struct MetaKeyValue {
    key: String,
    value: String,
}

/// Error to be returned when a path contains non-unicode characters
#[derive(Debug, Display, Error)]
#[display("Path contains non-unicode characters")]
struct NonUnicodePath;

/// Try and convert a path to a string (via `OsString`), returning a `NonUnicodePath`
/// error if not possible
fn path_to_string(path: PathBuf) -> Result<String, NonUnicodePath> {
    path.into_os_string()
        .into_string()
        .map_err(|_| NonUnicodePath)
}

#[Object]
/// The path to a data directory and the components used to build it
impl DirectoryPath {
    /// The instrument session for which this is the data directory
    #[instrument(skip(self))]
    async fn instrument_session(&self) -> &str {
        &self.instrument_session
    }
    /// The instrument for which this is the data directory
    #[instrument(skip(self))]
    async fn instrument(&self) -> &str {
        &self.info.name()
    }
    /// The absolute path to the data directory
    #[instrument(skip(self))]
    async fn path(&self) -> async_graphql::Result<String> {
        Ok(path_to_string(self.info.directory()?.render(self))?)
    }
}

impl FieldSource<DirectoryField> for DirectoryPath {
    fn resolve(&self, field: &DirectoryField) -> Cow<'_, str> {
        match field {
            DirectoryField::Year => Local::now().year().to_string().into(),
            DirectoryField::Visit => self.instrument_session.as_str().into(),
            DirectoryField::Proposal => self
                .instrument_session
                .split('-')
                .next()
                .expect("There is always one section for a split")
                .into(),
            DirectoryField::Instrument => self.info.name().into(),
        }
    }
}

#[Object]
/// Paths and values related to a specific scan/data collection for an instrument
impl ScanPaths {
    /// The directory used to generate this scan information.
    #[instrument(skip(self))]
    async fn directory(&self) -> &DirectoryPath {
        &self.directory
    }

    /// The root scan file for this scan. The path has no extension so that the format can be
    /// chosen by the client.
    #[instrument(skip(self))]
    async fn scan_file(&self) -> async_graphql::Result<String> {
        Ok(path_to_string(self.directory.info.scan()?.render(self))?)
    }

    /// The scan number for this scan. This should be unique for the requested instrument.
    #[instrument(skip(self))]
    async fn scan_number(&self) -> u32 {
        self.directory.info.scan_number()
    }

    /// The paths where the given detectors should write their files.
    ///
    /// Detector names are normalised before being used in file names by replacing any
    /// non-alphanumeric characters with '_'. If there are duplicate names in the list
    /// of detectors after this normalisation, there will be duplicate paths in the
    /// results.
    // TODO: The docs here reference the implementation specific behaviour in the normalisation
    #[instrument(skip(self))]
    async fn detectors(&self, names: Vec<Detector>) -> async_graphql::Result<Vec<DetectorPath>> {
        let template = self.directory.info.detector()?;
        Ok(names
            .into_iter()
            .map(|name| {
                path_to_string(template.render(&(name.as_str(), self))).map(|path| DetectorPath {
                    name: name.into_string(),
                    path,
                })
            })
            .collect::<Result<Vec<DetectorPath>, _>>()?)
    }
}

#[Object]
/// The current configuration for an instrument
impl CurrentConfiguration {
    /// The name of the instrument
    pub async fn instrument(&self) -> async_graphql::Result<&str> {
        Ok(self.db_config.name())
    }
    /// The template used to build the path to the data directory for an instrument
    pub async fn directory_template(&self) -> async_graphql::Result<String> {
        Ok(self.db_config.directory()?.to_string())
    }
    /// The template used to build the path of a scan file for a data acquisition, relative to the
    /// root of the data directory.
    pub async fn scan_template(&self) -> async_graphql::Result<String> {
        Ok(self.db_config.scan()?.to_string())
    }
    /// The template used to build the path of a detector's data file for a data acquisition,
    /// relative to the root of the data directory.
    pub async fn detector_template(&self) -> async_graphql::Result<String> {
        Ok(self.db_config.detector()?.to_string())
    }
    /// The latest scan number stored in the DB. This is the last scan number provided by this
    /// service but may not reflect the most recent scan number for an instrument if an external
    /// service (eg GDA) has incremented its own number tracker.
    pub async fn db_scan_number(&self) -> async_graphql::Result<u32> {
        Ok(self.db_config.scan_number())
    }
    /// The highest matching number file for this instrument in the configured tracking directory.
    /// May be null if no directory is available for this instrument or if there are no matching
    /// number files.
    pub async fn file_scan_number(&self) -> async_graphql::Result<Option<u32>> {
        Ok(self.high_file)
    }
    /// The file extension used for the file based tracking, eg using an extension of 'ext'
    /// would create files `1.ext`, `2.ext` etc
    pub async fn tracker_file_extension(&self) -> async_graphql::Result<Option<&str>> {
        Ok(self.db_config.tracker_file_extension())
    }
}

impl CurrentConfiguration {
    async fn for_config(
        db_config: InstrumentConfiguration,
        nt: &NumTracker,
    ) -> async_graphql::Result<Self> {
        let dir = nt
            .for_instrument(db_config.name(), db_config.tracker_file_extension())
            .await?;
        let high_file = dir.prev().await?;
        Ok(CurrentConfiguration {
            db_config,
            high_file,
        })
    }
}

impl FieldSource<ScanField> for ScanPaths {
    fn resolve(&self, field: &ScanField) -> Cow<'_, str> {
        match field {
            ScanField::Subdirectory => self.subdirectory.to_string().into(),
            ScanField::ScanNumber => self.directory.info.scan_number().to_string().into(),
            ScanField::Directory(dir) => self.directory.resolve(dir),
            ScanField::Custom(key) => self
                .metadata
                .get(key)
                .map(|s| s.as_str())
                .unwrap_or("")
                .into(),
        }
    }
}

impl FieldSource<DetectorField> for (&str, &ScanPaths) {
    fn resolve(&self, field: &DetectorField) -> Cow<'_, str> {
        match field {
            DetectorField::Detector => self.0.into(),
            DetectorField::Scan(s) => self.1.resolve(s),
        }
    }
}

#[Object]
/// Queries relating to numtracker configurations that have no side-effects
impl Query {
    /// Get the data directory information for the given instrument and instrument session.
    /// This information is not scan specific
    #[instrument(skip(self, ctx))]
    async fn paths(
        &self,
        ctx: &Context<'_>,
        instrument: String,
        instrument_session: String,
    ) -> async_graphql::Result<DirectoryPath> {
        let db = ctx.data::<SqliteScanPathService>()?;
        let info = db.current_configuration(&instrument).await?;
        Ok(DirectoryPath {
            instrument_session,
            info,
        })
    }

    /// Get the current configuration for the given instrument
    #[instrument(skip(self, ctx))]
    async fn configuration(
        &self,
        ctx: &Context<'_>,
        instrument: String,
    ) -> async_graphql::Result<CurrentConfiguration> {
        check_auth(ctx, |policy, token| {
            policy.check_instrument_admin(token, &instrument)
        })
        .await?;
        let db = ctx.data::<SqliteScanPathService>()?;
        let nt = ctx.data::<NumTracker>()?;
        trace!("Getting config for {instrument:?}");
        let conf = db.current_configuration(&instrument).await?;
        CurrentConfiguration::for_config(conf, nt).await
    }

    /// Get the configurations for all available instruments
    /// Can be filtered to provide one or more specific instruments
    #[instrument(skip(self, ctx))]
    async fn configurations(
        &self,
        ctx: &Context<'_>,
        instrument_filters: Option<Vec<String>>,
    ) -> async_graphql::Result<Vec<CurrentConfiguration>> {
        check_auth(ctx, |policy, token| policy.check_admin(token)).await?;
        let db = ctx.data::<SqliteScanPathService>()?;
        let nt = ctx.data::<NumTracker>()?;
        let configurations = match instrument_filters {
            Some(filters) => db.configurations(filters).await?,
            None => db.all_configurations().await?,
        };

        futures::future::join_all(
            configurations
                .into_iter()
                .map(|cnf| CurrentConfiguration::for_config(cnf, nt)),
        )
        .await
        .into_iter()
        .collect()
    }
}

#[Object]
/// Queries that modify the state of the numtracker configuration in some way
impl Mutation {
    /// Generate scan file locations for the next scan
    #[instrument(skip(self, ctx))]
    async fn scan<'ctx>(
        &self,
        ctx: &Context<'ctx>,
        instrument: String,
        instrument_session: String,
        sub: Option<Subdirectory>,
        meta: Option<Vec<MetaKeyValue>>,
    ) -> async_graphql::Result<ScanPaths> {
        check_auth(ctx, |policy, token| {
            policy.check_access(token, &instrument, &instrument_session)
        })
        .await?;
        let db = ctx.data::<SqliteScanPathService>()?;
        let nt = ctx.data::<NumTracker>()?;
        // There is a race condition here if a process increments the file
        // while the DB is being queried or between the two queries but there
        // isn't much we can do from here.
        let current = db.current_configuration(&instrument).await?;
        let dir = nt
            .for_instrument(&instrument, current.tracker_file_extension())
            .await?;

        let next_scan = db
            .next_scan_configuration(&instrument, dir.prev().await?)
            .await?;

        if let Err(e) = dir.set(next_scan.scan_number()).await {
            warn!("Failed to increment tracker file: {e}");
        }

        let metadata = meta
            .into_iter()
            .flatten()
            .map(|kv| (kv.key, kv.value))
            .collect();

        Ok(ScanPaths {
            directory: DirectoryPath {
                instrument_session,
                info: next_scan,
            },
            metadata,
            subdirectory: sub.unwrap_or_default(),
        })
    }

    /// Add or modify the stored configuration for an instrument
    #[instrument(skip(self, ctx))]
    async fn configure<'ctx>(
        &self,
        ctx: &Context<'ctx>,
        instrument: String,
        config: ConfigurationUpdates,
    ) -> async_graphql::Result<CurrentConfiguration> {
        check_auth(ctx, |pc, token| {
            pc.check_instrument_admin(token, &instrument)
        })
        .await?;
        let db = ctx.data::<SqliteScanPathService>()?;
        let nt = ctx.data::<NumTracker>()?;
        trace!("Configuring: {instrument}: {config:?}");
        let upd = config.into_update(&instrument);
        let db_config = match upd.update_instrument(db).await? {
            Some(bc) => bc,
            None => upd.insert_new(db).await?,
        };
        CurrentConfiguration::for_config(db_config, nt).await
    }
}

async fn check_auth<'ctx, Check, R>(ctx: &Context<'ctx>, check: Check) -> async_graphql::Result<()>
where
    Check: Fn(&'ctx PolicyCheck, Option<&'ctx Authorization<Bearer>>) -> R,
    R: Future<Output = Result<(), AuthError>>,
{
    if let Some(policy) = ctx.data::<Option<PolicyCheck>>()? {
        trace!("Auth enabled: checking token");
        let token = ctx.data::<Option<Authorization<Bearer>>>()?;
        check(policy, token.as_ref())
            .await
            .inspect_err(|e| info!("Authorization failed: {e:?}"))
            .map_err(async_graphql::Error::from)
    } else {
        trace!("No authorization configured");
        Ok(())
    }
}

/// Changes that should be made to an instrument's configuration
#[derive(Debug, InputObject)]
struct ConfigurationUpdates {
    /// New template used to determine the root data directory
    directory: Option<InputTemplate<DirectoryTemplate>>,
    /// New template used to determine the relative path to the main scan file for a collection
    scan: Option<InputTemplate<ScanTemplate>>,
    /// New template used to determine the relative path for detector data files
    detector: Option<InputTemplate<DetectorTemplate>>,
    /// The highest scan number to have been allocated. The next scan files generated will use the
    /// next number.
    scan_number: Option<u32>,
    /// The extension of the files used to track scan numbers by GDA's numtracker facility
    tracker_file_extension: Option<String>,
}

impl ConfigurationUpdates {
    fn into_update<S: Into<String>>(self, name: S) -> InstrumentConfigurationUpdate {
        InstrumentConfigurationUpdate {
            name: name.into(),
            scan_number: self.scan_number,
            directory: self.directory.map(|t| t.0),
            scan: self.scan.map(|t| t.0),
            detector: self.detector.map(|t| t.0),
            tracker_file_extension: self.tracker_file_extension,
        }
    }
}

#[derive(Debug, Display)]
#[display("{_0}")]
struct InputTemplate<S: PathSpec>(PathTemplate<S::Field>);

impl<S: PathSpec> Description for InputTemplate<S> {
    fn description() -> &'static str {
        S::describe()
    }
}

#[Scalar(use_type_description, name_type)]
impl<S: PathSpec> ScalarType for InputTemplate<S> {
    fn parse(value: Value) -> InputValueResult<Self> {
        match value {
            Value::String(txt) => match S::new_checked(&txt) {
                Ok(pt) => Ok(Self(pt)),
                Err(e) => Err(InputValueError::custom(e)),
            },
            other => Err(InputValueError::expected_type(other)),
        }
    }
    fn to_value(&self) -> Value {
        Value::String(self.0.to_string())
    }
}

impl<S: PathSpec> TypeName for InputTemplate<S> {
    fn type_name() -> Cow<'static, str> {
        // best effort remove the `numtracker::paths::` prefix
        any::type_name::<S>()
            .split("::")
            .last()
            .expect("There is always a last value for a split")
            .into()
    }
}

/// Name of subdirectory within data directory where data should be written.
/// Can be nested (eg foo/bar) but cannot include links to parent directories (eg ../foo).
// Derived Default is OK without validation as empty path is a valid subdirectory
#[derive(Debug, Display, Default)]
pub struct Subdirectory(String);

#[derive(Debug, Display, Error)]
pub enum InvalidSubdirectory {
    #[display("Segment {_0} of path is not valid for a subdirectory")]
    InvalidComponent(#[error(ignore)] usize),
    #[display("Subdirectory cannot be absolute")]
    AbsolutePath,
}

#[Scalar]
impl ScalarType for Subdirectory {
    fn parse(value: Value) -> InputValueResult<Self> {
        if let Value::String(path) = value {
            let path = PathBuf::from(&path);
            let mut new_sub = PathBuf::new();
            for (i, comp) in path.components().enumerate() {
                let err = match comp {
                    Component::CurDir => continue,
                    Component::Normal(seg) => {
                        new_sub.push(seg);
                        continue;
                    }
                    Component::RootDir => InvalidSubdirectory::AbsolutePath,
                    Component::Prefix(_) | Component::ParentDir => {
                        InvalidSubdirectory::InvalidComponent(i)
                    }
                };
                return Err(InputValueError::custom(err));
            }
            // path was created from string so shouldn't actually be lossy conversion
            Ok(Self(new_sub.to_string_lossy().to_string()))
        } else {
            Err(InputValueError::expected_type(value))
        }
    }
    fn to_value(&self) -> Value {
        Value::String(self.0.to_string())
    }
}

/// Detector name
#[derive(Debug)]
pub struct Detector(String);

#[Scalar]
impl ScalarType for Detector {
    fn parse(value: Value) -> InputValueResult<Self> {
        if let Value::String(name) = value {
            Ok(if name.contains(Self::INVALID) {
                Self(
                    name.split(Self::INVALID)
                        .filter(|s| !s.is_empty())
                        .collect::<Vec<_>>()
                        .join("_"),
                )
            } else {
                Self(name)
            })
        } else {
            Err(InputValueError::expected_type(value))
        }
    }
    fn to_value(&self) -> Value {
        Value::String(self.0.clone())
    }
}

impl Detector {
    const INVALID: fn(char) -> bool = |c| !c.is_ascii_alphanumeric();
    fn into_string(self) -> String {
        self.0
    }
    fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

#[cfg(test)]
mod tests {
    use std::error::Error;
    use std::fs;

    use async_graphql::{EmptySubscription, InputType as _, Request, Schema, SchemaBuilder, Value};
    use axum::http::HeaderValue;
    use axum_extra::headers::authorization::{Bearer, Credentials};
    use axum_extra::headers::Authorization;
    use httpmock::MockServer;
    use rstest::{fixture, rstest};
    use serde_json::json;
    use tempfile::TempDir;

    use super::auth::PolicyCheck;
    use super::{ConfigurationUpdates, InputTemplate, Mutation, Query};
    use crate::cli::PolicyOptions;
    use crate::db_service::{ConfigurationError, SqliteScanPathService};
    use crate::graphql::graphql_schema;
    use crate::numtracker::TempTracker;

    type NtSchema = Schema<Query, Mutation, EmptySubscription>;
    type NtBuilder = SchemaBuilder<Query, Mutation, EmptySubscription>;

    struct TestEnv {
        schema: NtSchema,
        dir: TempDir,
        db: SqliteScanPathService,
    }

    struct TestAuthEnv {
        schema: NtSchema,
        dir: TempDir,
        db: SqliteScanPathService,
        server: MockServer,
    }

    fn updates(
        directory: Option<&str>,
        scan: Option<&str>,
        det: Option<&str>,
        num: Option<u32>,
        ext: Option<&str>,
    ) -> ConfigurationUpdates {
        ConfigurationUpdates {
            directory: directory
                .map(|d| InputTemplate::parse(Some(Value::String(d.into()))).unwrap()),
            scan: scan.map(|s| InputTemplate::parse(Some(Value::String(s.into()))).unwrap()),
            detector: det.map(|d| InputTemplate::parse(Some(Value::String(d.into()))).unwrap()),
            scan_number: num,
            tracker_file_extension: ext.map(|e| e.into()),
        }
    }

    /// Helper for creating graphql values from literals
    macro_rules! value {
        ($tree:tt) => {
            Value::from_json(json!($tree)).unwrap()
        };
    }

    #[fixture]
    async fn db() -> SqliteScanPathService {
        let db = SqliteScanPathService::memory().await;
        let cfg = updates(
            Some("/tmp/{instrument}/data/{visit}/"),
            Some("{subdirectory}/{instrument}-{scan_number}"),
            Some("{subdirectory}/{instrument}-{scan_number}-{detector}"),
            Some(122),
            None,
        );
        cfg.into_update("i22").insert_new(&db).await.unwrap();
        let cfg = updates(
            Some("/tmp/{instrument}/data/{visit}/"),
            Some("{subdirectory}/{instrument}-{scan_number}"),
            Some("{subdirectory}/{scan_number}/{instrument}-{scan_number}-{detector}"),
            Some(621),
            Some("b21_ext"),
        );
        cfg.into_update("b21").insert_new(&db).await.unwrap();
        db
    }

    #[fixture]
    async fn components(
        #[future(awt)] db: SqliteScanPathService,
    ) -> (NtBuilder, TempDir, SqliteScanPathService) {
        let TempTracker(nt, dir) = TempTracker::new(|p| {
            fs::create_dir(p.join("i22"))?;
            fs::File::create_new(p.join("i22").join("122.i22"))?;
            fs::create_dir(p.join("b21"))?;
            fs::File::create_new(p.join("b21").join("211.b21_ext"))?;
            Ok(())
        });
        (
            Schema::build(Query, Mutation, EmptySubscription)
                .data(db.clone())
                .data(nt),
            dir,
            db,
        )
    }

    #[fixture]
    async fn env(
        #[future(awt)] components: (NtBuilder, TempDir, SqliteScanPathService),
    ) -> TestEnv {
        TestEnv {
            schema: components.0.data(Option::<PolicyCheck>::None).finish(),
            dir: components.1,
            db: components.2,
        }
    }

    #[fixture]
    async fn auth_env(
        #[future(awt)] components: (NtBuilder, TempDir, SqliteScanPathService),
    ) -> TestAuthEnv {
        let server = MockServer::start();
        let check = PolicyCheck::new(PolicyOptions {
            policy_host: server.url(""),
            access_query: "demo/access".into(),
            admin_query: "demo/admin".into(),
        });
        TestAuthEnv {
            schema: components.0.data(Some(check)).finish(),
            dir: components.1,
            db: components.2,
            server,
        }
    }

    #[rstest]
    #[tokio::test]
    async fn missing_config(#[future(awt)] env: TestEnv) {
        let result = env
            .schema
            .execute(r#"{paths(instrument: "i11", instrumentSession: "cm1234-5") {path}}"#)
            .await;

        assert_eq!(result.data, Value::Null);
        println!("{result:?}");
        assert_eq!(
            result.errors[0].message,
            r#"No configuration available for instrument "i11""#
        );
    }

    #[rstest]
    #[tokio::test]
    async fn paths(#[future(awt)] env: TestEnv) {
        let result = env
            .schema
            .execute(r#"{paths(instrument: "i22", instrumentSession: "cm12345-3") {path instrumentSession}}"#)
            .await;
        println!("{result:#?}");
        let exp = value!({"paths": {"instrumentSession": "cm12345-3", "path": "/tmp/i22/data/cm12345-3"}});
        assert_eq!(result.errors, &[]);
        assert_eq!(result.data, exp);
    }

    #[rstest]
    #[tokio::test]
    async fn scan(#[future(awt)] env: TestEnv) {
        let query = r#"mutation {
            scan(instrument: "i22", instrumentSession: "cm12345-3", sub: "foo/bar") {
                directory { instrument path instrumentSession } scanFile scanNumber
                detectors(names: ["det_one", "det_two"]) { name path }
            }
        }"#;
        let result = env.schema.execute(query).await;

        println!("{result:#?}");
        assert_eq!(result.errors, &[]);
        let exp = value!({
        "scan": {
            "directory": {"instrumentSession": "cm12345-3", "instrument": "i22", "path": "/tmp/i22/data/cm12345-3"},
            "scanFile": "foo/bar/i22-123",
            "scanNumber": 123,
            "detectors": [
                {"path": "foo/bar/i22-123-det_one", "name": "det_one"},
                {"path": "foo/bar/i22-123-det_two", "name": "det_two"}
            ]
        }});
        assert_eq!(result.data, exp);
    }

    #[rstest]
    #[tokio::test]
    async fn configuration(#[future(awt)] env: TestEnv) {
        let query = r#"{
        configuration(instrument: "i22") {
            instrument directoryTemplate scanTemplate detectorTemplate dbScanNumber trackerFileExtension
        }}"#;
        let result = env.schema.execute(query).await;
        let exp = value!({
            "configuration": {
                "instrument":"i22",
                "directoryTemplate": "/tmp/{instrument}/data/{visit}",
                "scanTemplate": "{subdirectory}/{instrument}-{scan_number}",
                "detectorTemplate": "{subdirectory}/{instrument}-{scan_number}-{detector}",
                "dbScanNumber": 122,
                "trackerFileExtension": Value::Null
            }
        });
        assert_eq!(result.errors, &[]);
        assert_eq!(result.data, exp);
    }

    #[rstest]
    #[tokio::test]
    async fn configurations(#[future(awt)] env: TestEnv) {
        let query = r#"{
        configurations(instrumentFilters: ["i22"]) {
            instrument directoryTemplate scanTemplate detectorTemplate dbScanNumber fileScanNumber trackerFileExtension
        }}"#;
        let result = env.schema.execute(query).await;
        let exp = value!({
            "configurations": [
                {
                    "instrument": "i22",
                    "directoryTemplate": "/tmp/{instrument}/data/{visit}",
                    "scanTemplate": "{subdirectory}/{instrument}-{scan_number}",
                    "detectorTemplate": "{subdirectory}/{instrument}-{scan_number}-{detector}",
                    "dbScanNumber": 122,
                    "fileScanNumber": 122,
                    "trackerFileExtension": Value::Null,
                }
            ]
        });
        assert_eq!(result.errors, &[]);
        assert_eq!(result.data, exp);
    }

    #[rstest]
    #[tokio::test]
    async fn configurations_all(#[future(awt)] env: TestEnv) {
        let query = r#"{
        configurations {
            instrument directoryTemplate scanTemplate detectorTemplate dbScanNumber fileScanNumber trackerFileExtension
        }}"#;
        let result = env.schema.execute(query).await;
        let exp = value!({
            "configurations": [
                {
                    "instrument": "i22",
                    "directoryTemplate": "/tmp/{instrument}/data/{visit}",
                    "scanTemplate": "{subdirectory}/{instrument}-{scan_number}",
                    "detectorTemplate": "{subdirectory}/{instrument}-{scan_number}-{detector}",
                    "dbScanNumber": 122,
                    "fileScanNumber": 122,
                    "trackerFileExtension": Value::Null,
                },
                {
                    "instrument": "b21",
                    "directoryTemplate": "/tmp/{instrument}/data/{visit}",
                    "scanTemplate": "{subdirectory}/{instrument}-{scan_number}",
                    "detectorTemplate": "{subdirectory}/{scan_number}/{instrument}-{scan_number}-{detector}",
                    "dbScanNumber": 621,
                    "fileScanNumber": 211,
                    "trackerFileExtension": "b21_ext",
                },
            ]
        });
        assert_eq!(result.errors, &[]);
        assert_eq!(result.data, exp);
    }

    #[rstest]
    #[tokio::test]
    async fn custom_tracker_file_extension(#[future(awt)] env: TestEnv) {
        let result = env
            .schema
            .execute(r#"{configuration(instrument: "b21") { trackerFileExtension }}"#)
            .await;
        assert_eq!(result.errors, &[]);
        assert_eq!(
            result.data,
            value!({ "configuration": { "trackerFileExtension": "b21_ext" } })
        );
    }

    #[rstest]
    #[tokio::test]
    async fn configuration_with_mismatched_numbers(
        #[future(awt)] env: TestEnv,
    ) -> Result<(), Box<dyn Error>> {
        tokio::fs::File::create_new(env.dir.as_ref().join("i22").join("5678.i22"))
            .await
            .unwrap();
        let query = r#"{
            configuration(instrument: "i22") {
                dbScanNumber
                fileScanNumber
            }
        }"#;
        let result = env.schema.execute(query).await;
        let exp = value!({
            "configuration": {
                "dbScanNumber": 122,
                "fileScanNumber": 5678
            }
        });
        assert_eq!(result.errors, &[]);
        assert_eq!(result.data, exp);

        let db_num = env.db.current_configuration("i22").await?.scan_number();
        let file_num = env.dir.as_ref().join("i22").join("5678.i22");
        let next_file_num = env.dir.as_ref().join("i22").join("5679.i22");
        assert_eq!(db_num, 122);
        assert!(file_num.exists());
        assert!(!next_file_num.exists());
        Ok(())
    }

    #[rstest]
    #[tokio::test]
    async fn empty_configure_for_existing(#[future(awt)] env: TestEnv) {
        let query = r#"mutation {
            configure(instrument: "i22", config: {}) {
                directoryTemplate scanTemplate detectorTemplate dbScanNumber
            }
        }"#;
        let result = env.schema.execute(query).await;
        let exp = value!({
            "configure": {
                "directoryTemplate": "/tmp/{instrument}/data/{visit}",
                "scanTemplate": "{subdirectory}/{instrument}-{scan_number}",
                "detectorTemplate": "{subdirectory}/{instrument}-{scan_number}-{detector}",
                "dbScanNumber": 122
            }
        });
        println!("{result:#?}");
        assert_eq!(result.errors, &[]);
        assert_eq!(result.data, exp);
    }

    #[rstest]
    #[tokio::test]
    async fn configure_template_for_existing(
        #[future(awt)] env: TestEnv,
    ) -> Result<(), Box<dyn Error>> {
        let query = r#"mutation {
        configure(instrument: "i22", config: { scan: "{instrument}-{scan_number}"}) {
            scanTemplate
        }}"#;
        let result = env.schema.execute(query).await;
        let exp = value!({"configure": { "scanTemplate": "{instrument}-{scan_number}"}});
        println!("{result:#?}");
        assert_eq!(result.errors, &[]);
        assert_eq!(result.data, exp);
        let new = env
            .db
            .current_configuration("i22")
            .await?
            .scan()?
            .to_string();
        assert_eq!(new, "{instrument}-{scan_number}");
        Ok(())
    }

    #[rstest]
    #[tokio::test]
    async fn configure_new_instrument(#[future(awt)] env: TestEnv) {
        assert_matches::assert_matches!(
            env.db.current_configuration("i16").await,
            Err(ConfigurationError::MissingInstrument(bl)) if bl == "i16"
        );

        let result = env
            .schema
            .execute(
                r#"mutation {
                    configure(instrument: "i16", config: {
                        directory: "/tmp/{instrument}/{year}/{visit}"
                        scan: "{instrument}-{scan_number}"
                        detector: "{scan_number}-{detector}"
                    }) {
                        scanTemplate directoryTemplate detectorTemplate dbScanNumber
                    }
                }"#,
            )
            .await;
        let exp = value!({ "configure": {
                "directoryTemplate": "/tmp/{instrument}/{year}/{visit}",
                "scanTemplate": "{instrument}-{scan_number}",
                "detectorTemplate": "{scan_number}-{detector}",
                "dbScanNumber": 0
            } });
        assert_eq!(result.errors, &[]);
        assert_eq!(result.data, exp);
        _ = env.db.current_configuration("i16").await.unwrap();
    }

    #[rstest]
    #[tokio::test]
    async fn unauthorised_scan_request(#[future(awt)] auth_env: TestAuthEnv) {
        let query =
            r#"mutation {scan(instrument: "i22", instrumentSession: "cm12345-3") { scanNumber }}"#;
        let result = auth_env
            .schema
            .execute(Request::new(query).data(Option::<Authorization<Bearer>>::None))
            .await;

        println!("{result:#?}");
        assert_eq!(
            result.errors[0].message,
            "No authentication token was provided"
        );
        assert_eq!(result.data, Value::Null);
    }

    #[rstest]
    #[tokio::test]
    async fn denied_scan_request(#[future(awt)] auth_env: TestAuthEnv) {
        let query =
            r#"mutation{ scan(instrument: "i22", instrumentSession: "cm12345-3") { scanNumber }}"#;
        let token = Some(Authorization(
            Bearer::decode(&HeaderValue::from_str("Bearer token_value").unwrap()).unwrap(),
        ));
        let auth = auth_env
            .server
            .mock_async(|when, then| {
                when.method("POST").path("/demo/access");
                then.status(200).body(r#"{"result": false}"#);
            })
            .await;
        let result = auth_env
            .schema
            .execute(Request::new(query).data(token))
            .await;
        auth.assert();

        println!("{result:#?}");
        assert_eq!(result.errors[0].message, "Authentication failed");
        assert_eq!(result.data, Value::Null);

        // Ensure that the number wasn't incremented
        assert_eq!(
            auth_env
                .db
                .current_configuration("i22")
                .await
                .unwrap()
                .scan_number(),
            122
        );
    }

    #[rstest]
    #[tokio::test]
    async fn authorized_scan_request(#[future(awt)] auth_env: TestAuthEnv) {
        let query =
            r#"mutation{ scan(instrument: "i22", instrumentSession: "cm12345-3") { scanNumber }}"#;
        let token = Some(Authorization(
            Bearer::decode(&HeaderValue::from_str("Bearer token_value").unwrap()).unwrap(),
        ));
        let auth = auth_env
            .server
            .mock_async(|when, then| {
                when.method("POST").path("/demo/access");
                then.status(200).body(r#"{"result": true}"#);
            })
            .await;
        let result = auth_env
            .schema
            .execute(Request::new(query).data(token))
            .await;
        auth.assert();

        println!("{result:#?}");
        assert_eq!(result.errors, &[]);
        assert_eq!(result.data, value!({"scan": {"scanNumber": 123}}));
        // Ensure that the number was incremented
        assert_eq!(
            auth_env
                .db
                .current_configuration("i22")
                .await
                .unwrap()
                .scan_number(),
            123
        );
        assert!(
            tokio::fs::try_exists(auth_env.dir.as_ref().join("i22").join("123.i22"))
                .await
                .unwrap()
        );
    }

    #[rstest]
    #[tokio::test]
    async fn scan_numbers_synced_with_external(#[future(awt)] env: TestEnv) {
        tokio::fs::File::create_new(env.dir.as_ref().join("i22").join("5678.i22"))
            .await
            .unwrap();
        let query =
            r#"mutation { scan(instrument: "i22", instrumentSession:"cm12345-3") { scanNumber }}"#;
        let result = env.schema.execute(query).await;
        let exp = value!({"scan": {"scanNumber": 5679}});

        assert_eq!(result.data, exp);

        // DB number has been updated
        assert_eq!(
            env.db
                .current_configuration("i22")
                .await
                .unwrap()
                .scan_number(),
            5679
        );

        // File has been updated
        assert!(
            tokio::fs::try_exists(env.dir.as_ref().join("i22").join("5679.i22"))
                .await
                .unwrap()
        );
    }

    /// Ensure that the schema has not changed unintentionally. Might end up being a pain to
    /// maintain but should hopefully be fairly stable once the API has stabilised.
    #[test]
    fn schema_sdl() {
        let mut buf = Vec::new();
        graphql_schema(&mut buf).unwrap();
        assert_eq!(
            String::from_utf8(buf).unwrap(),
            include_str!("../../static/service_schema.graphql")
        );
    }
}
#[cfg(test)]
mod subdirectory_tests {
    use async_graphql::{InputType as _, InputValueResult, Number, Value};

    use super::Subdirectory;
    fn parse_str(sub: &str) -> InputValueResult<Subdirectory> {
        Subdirectory::parse(Some(Value::String(sub.into())))
    }

    #[test]
    fn valid_subdirectory() {
        parse_str("valid/subdirectory").unwrap();
    }

    #[test]
    fn invalid_subdirectory() {
        parse_str("../parent").unwrap_err();
        parse_str("/absolute/path").unwrap_err();
        Subdirectory::parse(Some(Value::Number(Number::from_f64(42f64).unwrap()))).unwrap_err();
    }

    #[test]
    fn back_to_value() {
        let sub = parse_str("./subdirectory").unwrap();
        assert_eq!(sub.to_value(), Value::String("subdirectory".into()))
    }
}

#[cfg(test)]
mod detector_tests {
    use async_graphql::{InputType as _, Number, Value};

    use super::Detector;

    #[rstest::rstest]
    #[case::unchanged("camera", "camera")]
    #[case::punctuation("foo+bar", "foo_bar")]
    #[case::multiple_punctuation("foo+-?!bar", "foo_bar")]
    fn normalised_name(#[case] input: &str, #[case] output: &str) {
        let det = Detector::parse(Some(Value::String(input.into()))).unwrap();
        let value = det.to_value();
        let Value::String(s) = value else {
            panic!("Unexpected value from detector: {value}");
        };
        assert_eq!(s, output);
        assert_eq!(det.as_str(), output);
        assert_eq!(det.into_string(), output);
    }

    #[test]
    fn invalid_value() {
        Detector::parse(Number::from_f64(42f64).map(Value::Number)).unwrap_err();
        Detector::parse(None).unwrap_err();
    }
}

#[cfg(test)]
mod input_template_tests {
    use async_graphql::{InputType, Value};

    use super::InputTemplate;
    use crate::paths::{DetectorTemplate, DirectoryTemplate, ScanTemplate};

    #[test]
    fn valid_directory_template() {
        let template = InputTemplate::<DirectoryTemplate>::parse(Some(Value::String(
            "/tmp/{instrument}/data/{visit}".into(),
        )))
        .unwrap();
        assert_eq!(
            template.as_raw_value().unwrap().to_string(),
            "/tmp/{instrument}/data/{visit}"
        );
        assert_eq!(
            template.to_value(),
            Value::String("/tmp/{instrument}/data/{visit}".into())
        )
    }

    #[rstest::rstest]
    #[case::relative("tmp/{instrument}/{visit}")]
    #[case::invalid_template("/tmp/{nested{placeholder}}")]
    fn invalid_directory_template(#[case] path: String) {
        InputTemplate::<DirectoryTemplate>::parse(Some(Value::String(path))).unwrap_err();
    }

    #[rstest::rstest]
    #[case::absolute("/tmp/{instrument}")]
    #[case::missing_scan_number("scan_file")]
    #[case::invalid_template("tmp/{nested{placeholder}}")]
    fn invalid_scan(#[case] path: String) {
        InputTemplate::<ScanTemplate>::parse(Some(Value::String(path))).unwrap_err();
    }

    #[rstest::rstest]
    #[case::relative("tmp/{instrument}/{visit}")]
    #[case::missing_scan_number("{detector}")]
    #[case::missing_detector("{scan_number}")]
    #[case::invalid_template("tmp/{nested{placeholder}}")]
    fn invalid_detector_template(#[case] path: String) {
        InputTemplate::<DetectorTemplate>::parse(Some(Value::String(path))).unwrap_err();
    }

    #[rstest::rstest]
    #[case::integer(Some(Value::Number(42.into())))]
    #[case::list(Some(Value::List(vec![Value::Number(211.into())])))]
    #[case::none(None)]
    fn invalid_value_type(#[case] value: Option<Value>) {
        InputTemplate::<ScanTemplate>::parse(value).unwrap_err();
    }
}
