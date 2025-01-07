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
use std::error::Error;
use std::fmt::Display;
use std::future::Future;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use async_graphql::extensions::Tracing;
use async_graphql::http::GraphiQLSource;
use async_graphql::registry::{MetaType, MetaTypeId, Registry};
use async_graphql::{
    Context, EmptySubscription, InputObject, InputType, InputValueError, InputValueResult, Object,
    Scalar, ScalarType, Schema, SimpleObject, Value,
};
use async_graphql_axum::{GraphQLRequest, GraphQLResponse};
use auth::{AuthError, PolicyCheck};
use axum::response::{Html, IntoResponse};
use axum::routing::{get, post};
use axum::{Extension, Router};
use axum_extra::headers::authorization::Bearer;
use axum_extra::headers::Authorization;
use axum_extra::TypedHeader;
use chrono::{Datelike, Local};
use tokio::net::TcpListener;
use tracing::{info, instrument, trace, warn};

use crate::cli::ServeOptions;
use crate::db_service::{
    BeamlineConfiguration, BeamlineConfigurationUpdate, SqliteScanPathService,
};
use crate::numtracker::NumTracker;
use crate::paths::{
    BeamlineField, DetectorField, DetectorTemplate, PathSpec, ScanField, ScanTemplate,
    VisitTemplate,
};
use crate::template::{FieldSource, PathTemplate};

mod auth;

pub async fn serve_graphql(db: &Path, opts: ServeOptions) {
    let db = SqliteScanPathService::connect(db)
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
        .route("/status", get(|| async {}))
        .route("/graphql", post(graphql_handler))
        .route("/graphiql", get(graphiql))
        .layer(Extension(schema));
    let listener = TcpListener::bind(addr)
        .await
        .unwrap_or_else(|_| panic!("Port {:?} in use", addr));
    axum::serve(listener, app)
        .await
        .expect("Can't serve graphql endpoint");
}

pub fn graphql_schema() {
    let schema = Schema::new(Query, Mutation, EmptySubscription);
    println!("{}", schema.sdl());
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
    name: String,
    path: String,
}

/// GraphQL type to provide path data for a specific visit
struct VisitPath {
    visit: String,
    info: BeamlineConfiguration,
}

/// GraphQL type to provide path data for the next scan for a given visit
struct ScanPaths {
    visit: VisitPath,
    subdirectory: Subdirectory,
}

/// Error to be returned when a path contains non-unicode characters
#[derive(Debug)]
struct NonUnicodePath;

/// Try and convert a path to a string (via `OsString`), returning a `NonUnicodePath`
/// error if not possible
fn path_to_string(path: PathBuf) -> Result<String, NonUnicodePath> {
    path.into_os_string()
        .into_string()
        .map_err(|_| NonUnicodePath)
}

impl Display for NonUnicodePath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Path contains non-unicode characters")
    }
}

impl Error for NonUnicodePath {}

#[Object]
impl VisitPath {
    #[instrument(skip(self))]
    async fn visit(&self) -> &str {
        &self.visit
    }
    #[instrument(skip(self))]
    async fn beamline(&self) -> &str {
        &self.info.name()
    }
    #[instrument(skip(self))]
    async fn directory(&self) -> async_graphql::Result<String> {
        Ok(path_to_string(self.info.visit()?.render(self))?)
    }
}

impl FieldSource<BeamlineField> for VisitPath {
    fn resolve(&self, field: &BeamlineField) -> Cow<'_, str> {
        match field {
            BeamlineField::Year => Local::now().year().to_string().into(),
            BeamlineField::Visit => self.visit.as_str().into(),
            BeamlineField::Proposal => self
                .visit
                .split('-')
                .next()
                .expect("There is always one section for a split")
                .into(),
            BeamlineField::Instrument => self.info.name().into(),
        }
    }
}

#[Object]
impl ScanPaths {
    /// The visit used to generate this scan information. Should be the same as the visit passed in
    #[instrument(skip(self))]
    async fn visit(&self) -> &VisitPath {
        &self.visit
    }

    /// The root scan file for this scan. The path has no extension so that the format can be
    /// chosen by the client.
    #[instrument(skip(self))]
    async fn scan_file(&self) -> async_graphql::Result<String> {
        Ok(path_to_string(self.visit.info.scan()?.render(self))?)
    }

    /// The scan number for this scan. This should be unique for the requested beamline.
    #[instrument(skip(self))]
    async fn scan_number(&self) -> u32 {
        self.visit.info.scan_number()
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
        let template = self.visit.info.detector()?;
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
impl BeamlineConfiguration {
    pub async fn visit_template(&self) -> async_graphql::Result<String> {
        Ok(self.visit()?.to_string())
    }
    pub async fn scan_template(&self) -> async_graphql::Result<String> {
        Ok(self.scan()?.to_string())
    }
    pub async fn detector_template(&self) -> async_graphql::Result<String> {
        Ok(self.detector()?.to_string())
    }
    pub async fn latest_scan_number(&self) -> async_graphql::Result<u32> {
        Ok(self.scan_number())
    }
}

impl FieldSource<ScanField> for ScanPaths {
    fn resolve(&self, field: &ScanField) -> Cow<'_, str> {
        match field {
            ScanField::Subdirectory => self.subdirectory.to_string().into(),
            ScanField::ScanNumber => self.visit.info.scan_number().to_string().into(),
            ScanField::Beamline(bl) => self.visit.resolve(bl),
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
impl Query {
    #[instrument(skip(self, ctx))]
    async fn paths(
        &self,
        ctx: &Context<'_>,
        beamline: String,
        visit: String,
    ) -> async_graphql::Result<VisitPath> {
        let db = ctx.data::<SqliteScanPathService>()?;
        let info = db.current_configuration(&beamline).await?;
        Ok(VisitPath { visit, info })
    }

    #[instrument(skip(self, ctx))]
    async fn configuration(
        &self,
        ctx: &Context<'_>,
        beamline: String,
    ) -> async_graphql::Result<BeamlineConfiguration> {
        check_auth(ctx, |policy, token| policy.check_admin(token, &beamline)).await?;
        let db = ctx.data::<SqliteScanPathService>()?;
        trace!("Getting config for {beamline:?}");
        Ok(db.current_configuration(&beamline).await?)
    }
}

#[Object]
impl Mutation {
    /// Access scan file locations for the next scan
    #[instrument(skip(self, ctx))]
    async fn scan<'ctx>(
        &self,
        ctx: &Context<'ctx>,
        beamline: String,
        visit: String,
        sub: Option<Subdirectory>,
    ) -> async_graphql::Result<ScanPaths> {
        check_auth(ctx, |policy, token| {
            policy.check_access(token, &beamline, &visit)
        })
        .await?;
        let db = ctx.data::<SqliteScanPathService>()?;
        let nt = ctx.data::<NumTracker>()?;
        // There is a race condition here if a process increments the file
        // while the DB is being queried or between the two queries but there
        // isn't much we can do from here.
        let current = db.current_configuration(&beamline).await?;
        let dir = nt.for_beamline(&beamline, current.extension()).await?;

        let next_scan = db
            .next_scan_configuration(&beamline, dir.prev().await?)
            .await?;

        if let Err(e) = dir.set(next_scan.scan_number()).await {
            warn!("Failed to increment fallback tracker directory: {e}");
        }

        Ok(ScanPaths {
            visit: VisitPath {
                visit,
                info: next_scan,
            },
            subdirectory: sub.unwrap_or_default(),
        })
    }

    #[instrument(skip(self, ctx))]
    async fn configure<'ctx>(
        &self,
        ctx: &Context<'ctx>,
        beamline: String,
        config: ConfigurationUpdates,
    ) -> async_graphql::Result<BeamlineConfiguration> {
        check_auth(ctx, |pc, token| pc.check_admin(token, &beamline)).await?;
        let db = ctx.data::<SqliteScanPathService>()?;
        trace!("Configuring: {beamline}: {config:?}");
        let upd = config.into_update(beamline);
        match upd.update_beamline(db).await? {
            Some(bc) => Ok(bc),
            None => Ok(upd.insert_new(db).await?),
        }
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

#[derive(Debug, InputObject)]
struct ConfigurationUpdates {
    visit: Option<InputTemplate<VisitTemplate>>,
    scan: Option<InputTemplate<ScanTemplate>>,
    detector: Option<InputTemplate<DetectorTemplate>>,
    scan_number: Option<u32>,
    extension: Option<String>,
}

impl ConfigurationUpdates {
    fn into_update(self, name: String) -> BeamlineConfigurationUpdate {
        BeamlineConfigurationUpdate {
            name,
            scan_number: self.scan_number,
            visit: self.visit.map(|t| t.0),
            scan: self.scan.map(|t| t.0),
            detector: self.detector.map(|t| t.0),
            extension: self.extension,
        }
    }
}

#[derive(Debug)]
struct InputTemplate<S: PathSpec>(PathTemplate<S::Field>);

impl<S, F> InputType for InputTemplate<S>
where
    F: Send + Sync + TryFrom<String> + Display,
    S: PathSpec<Field = F> + Send + Sync,
{
    type RawValueType = PathTemplate<F>;
    fn parse(value: Option<Value>) -> InputValueResult<Self> {
        match value {
            Some(Value::String(txt)) => match S::new_checked(&txt) {
                Ok(pt) => Ok(Self(pt)),
                Err(e) => Err(InputValueError::custom(e)),
            },
            Some(other) => Err(InputValueError::expected_type(other)),
            None => Err(InputValueError::expected_type(Value::Null)),
        }
    }
    fn to_value(&self) -> Value {
        Value::String(self.0.to_string())
    }

    fn type_name() -> Cow<'static, str> {
        // best effort remove the `numtracker::paths::` prefix
        any::type_name::<S>()
            .split("::")
            .last()
            .expect("There is always a last value for a split")
            .into()
    }

    fn create_type_info(registry: &mut Registry) -> String {
        registry.create_input_type::<Self, _>(MetaTypeId::Scalar, |_| MetaType::Scalar {
            name: Self::type_name().into(),
            description: Some(S::describe().into()),
            is_valid: Some(Arc::new(|v| matches!(v, Value::String(_)))),
            visible: None,
            inaccessible: false,
            tags: vec![],
            specified_by_url: None,
            directive_invocations: vec![],
        })
    }

    fn as_raw_value(&self) -> Option<&Self::RawValueType> {
        Some(&self.0)
    }
}

// Derived Default is OK without validation as empty path is a valid subdirectory
#[derive(Debug, Default)]
pub struct Subdirectory(String);

#[derive(Debug)]
pub enum InvalidSubdirectory {
    InvalidComponent(usize),
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

impl Display for InvalidSubdirectory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InvalidSubdirectory::InvalidComponent(s) => {
                write!(f, "Segment {s} of path is not valid for a subdirectory")
            }
            InvalidSubdirectory::AbsolutePath => f.write_str("Subdirectory cannot be absolute"),
        }
    }
}

impl Error for InvalidSubdirectory {}

impl Display for Subdirectory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

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
    use async_graphql::{InputType as _, Value};

    use super::InputTemplate;
    use crate::paths::{DetectorTemplate, ScanTemplate, VisitTemplate};

    #[test]
    fn valid_visit_template() {
        InputTemplate::<VisitTemplate>::parse(Some(Value::String(
            "/tmp/{instrument}/data/{visit}".into(),
        )))
        .unwrap();
    }

    #[rstest::rstest]
    #[case::relative("tmp/{instrument}/{visit}")]
    #[case::missing_instrument("/tmp/{visit}")]
    #[case::missing_visit("/tmp/{instrument}/data")]
    #[case::invalid_template("/tmp/{nested{placeholder}}")]
    fn invalid_visit_template(#[case] path: String) {
        InputTemplate::<VisitTemplate>::parse(Some(Value::String(path))).unwrap_err();
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
}
