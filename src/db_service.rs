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

use std::fmt;
use std::marker::PhantomData;
use std::path::Path;

pub use error::ConfigurationError;
use error::NewConfigurationError;
use sqlx::sqlite::{SqliteConnectOptions, SqliteRow};
use sqlx::{query_as, FromRow, QueryBuilder, Row, Sqlite, SqlitePool};
use tracing::{info, instrument, trace};

use crate::paths::{
    BeamlineField, DetectorField, DetectorTemplate, InvalidPathTemplate, PathSpec, ScanField,
    ScanTemplate, VisitTemplate,
};
use crate::template::PathTemplate;

type SqliteTemplateResult<F> = Result<PathTemplate<F>, InvalidPathTemplate>;

#[derive(Clone)]
pub struct SqliteScanPathService {
    pool: SqlitePool,
}

#[derive(Debug, PartialEq, Eq)]
pub struct NumtrackerConfig {
    pub directory: String,
    pub extension: String,
}

#[derive(Debug, PartialEq, Eq)]
struct RawPathTemplate<F>(String, PhantomData<F>);

impl<Spec> RawPathTemplate<Spec>
where
    Spec: PathSpec,
{
    fn as_template(&self) -> SqliteTemplateResult<Spec::Field> {
        Spec::new_checked(&self.0)
    }
}

impl<F> From<String> for RawPathTemplate<F> {
    fn from(value: String) -> Self {
        Self(value, PhantomData)
    }
}

impl<F> From<&str> for RawPathTemplate<F> {
    fn from(value: &str) -> Self {
        value.to_string().into()
    }
}

/// The current configuration for a beamline
#[derive(Debug, PartialEq, Eq)]
pub struct BeamlineConfiguration {
    name: String,
    scan_number: u32,
    visit: RawPathTemplate<VisitTemplate>,
    scan: RawPathTemplate<ScanTemplate>,
    detector: RawPathTemplate<DetectorTemplate>,
    tracker_file_extension: Option<String>,
}

impl BeamlineConfiguration {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn scan_number(&self) -> u32 {
        self.scan_number
    }

    pub fn visit(&self) -> SqliteTemplateResult<BeamlineField> {
        self.visit.as_template()
    }

    pub fn scan(&self) -> SqliteTemplateResult<ScanField> {
        self.scan.as_template()
    }

    pub fn detector(&self) -> SqliteTemplateResult<DetectorField> {
        self.detector.as_template()
    }

    pub fn tracker_file_extension(&self) -> Option<&str> {
        self.tracker_file_extension.as_deref()
    }
}

impl<'r> FromRow<'r, SqliteRow> for BeamlineConfiguration {
    fn from_row(row: &'r SqliteRow) -> Result<Self, sqlx::Error> {
        Ok(DbBeamlineConfig {
            id: None,
            name: row.try_get("name")?,
            scan_number: row.try_get("scan_number")?,
            visit: row.try_get::<String, _>("visit")?,
            scan: row.try_get::<String, _>("scan")?,
            detector: row.try_get::<String, _>("detector")?,
            tracker_file_extension: row.try_get::<Option<String>, _>("tracker_file_extension")?,
        }
        .into())
    }
}

#[derive(Debug)]
pub struct BeamlineConfigurationUpdate {
    pub name: String,
    pub scan_number: Option<u32>,
    pub visit: Option<PathTemplate<BeamlineField>>,
    pub scan: Option<PathTemplate<ScanField>>,
    pub detector: Option<PathTemplate<DetectorField>>,
    pub tracker_file_extension: Option<String>,
}

impl BeamlineConfigurationUpdate {
    fn is_empty(&self) -> bool {
        self.scan_number.is_none()
            && self.visit.is_none()
            && self.scan.is_none()
            && self.detector.is_none()
            && self.tracker_file_extension.is_none()
    }

    pub async fn update_beamline(
        &self,
        db: &SqliteScanPathService,
    ) -> Result<Option<BeamlineConfiguration>, sqlx::Error> {
        if self.is_empty() {
            return match db.current_configuration(&self.name).await {
                Ok(bc) => Ok(Some(bc)),
                Err(ConfigurationError::MissingBeamline(_)) => Ok(None),
                Err(ConfigurationError::Db(e)) => Err(e),
            };
        }
        let mut q: QueryBuilder<Sqlite> = QueryBuilder::new("UPDATE beamline SET ");
        let mut fields = q.separated(", ");
        if let Some(num) = self.scan_number {
            fields.push("scan_number=");
            fields.push_bind_unseparated(num);
        }
        if let Some(visit) = &self.visit {
            fields.push("visit=");
            fields.push_bind_unseparated(visit.to_string());
        }
        if let Some(scan) = &self.scan {
            fields.push("scan=");
            fields.push_bind_unseparated(scan.to_string());
        }
        if let Some(detector) = &self.detector {
            fields.push("detector=");
            fields.push_bind_unseparated(detector.to_string());
        }
        if let Some(ext) = &self.tracker_file_extension {
            if ext != &self.name {
                // extension defaults to beamline name
                fields.push("tracker_file_extension=");
                fields.push_bind_unseparated(ext);
            }
        }
        q.push(" WHERE name = ");
        q.push_bind(&self.name);
        q.push(" RETURNING *");

        trace!(
            beamline = self.name,
            query = q.sql(),
            "Updating beamline configuration",
        );

        q.build_query_as().fetch_optional(&db.pool).await
    }
    pub async fn insert_new(
        self,
        db: &SqliteScanPathService,
    ) -> Result<BeamlineConfiguration, NewConfigurationError> {
        let dbc = DbBeamlineConfig {
            id: None,
            name: self.name,
            scan_number: i64::from(self.scan_number.unwrap_or(0)),
            visit: self.visit.ok_or("visit")?.to_string(),
            scan: self.scan.ok_or("scan")?.to_string(),
            detector: self.detector.ok_or("detector")?.to_string(),
            tracker_file_extension: self.tracker_file_extension,
        };
        Ok(dbc.insert_into(db).await?)
    }
    #[cfg(test)]
    fn empty(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            scan_number: None,
            visit: None,
            scan: None,
            detector: None,
            tracker_file_extension: None,
        }
    }
    #[cfg(test)]
    fn with_scan_number(self, number: u32) -> Self {
        Self {
            scan_number: Some(number),
            ..self
        }
    }
    #[cfg(test)]
    fn with_extension(self, ext: &str) -> Self {
        Self {
            tracker_file_extension: Some(ext.into()),
            ..self
        }
    }
}

#[derive(Debug)]
struct DbBeamlineConfig {
    #[allow(unused)] // unused but allows use of 'SELECT * ...' queries
    id: Option<i64>,
    name: String,
    scan_number: i64,
    visit: String,
    scan: String,
    detector: String,
    tracker_file_extension: Option<String>,
}

impl DbBeamlineConfig {
    pub async fn insert_into(
        self,
        db: &SqliteScanPathService,
    ) -> sqlx::Result<BeamlineConfiguration> {
        let bc = query_as!(
            DbBeamlineConfig,
            "INSERT INTO beamline
                (name, scan_number, visit, scan, detector, tracker_file_extension)
            VALUES
                (?,?,?,?,?,?)
            RETURNING *",
            self.name,
            self.scan_number,
            self.visit,
            self.scan,
            self.detector,
            self.tracker_file_extension
        )
        .fetch_one(&db.pool)
        .await?;
        Ok(bc.into())
    }
}

impl From<DbBeamlineConfig> for BeamlineConfiguration {
    fn from(value: DbBeamlineConfig) -> Self {
        Self {
            name: value.name,
            scan_number: u32::try_from(value.scan_number).expect("Out of scan numbers"),
            visit: value.visit.into(),
            scan: value.scan.into(),
            detector: value.detector.into(),
            tracker_file_extension: value.tracker_file_extension,
        }
    }
}

impl SqliteScanPathService {
    #[instrument]
    pub async fn connect(filename: &Path) -> Result<Self, sqlx::Error> {
        info!("Connecting to SQLite DB");
        let opts = SqliteConnectOptions::new()
            .create_if_missing(true)
            .filename(filename);
        let pool = SqlitePool::connect_with(opts).await?;
        sqlx::migrate!().run(&pool).await?;
        Ok(Self { pool })
    }

    pub async fn current_configuration(
        &self,
        beamline: &str,
    ) -> Result<BeamlineConfiguration, ConfigurationError> {
        query_as!(
            DbBeamlineConfig,
            "SELECT * FROM beamline WHERE name = ?",
            beamline
        )
        .fetch_optional(&self.pool)
        .await?
        .map(BeamlineConfiguration::from)
        .ok_or(ConfigurationError::MissingBeamline(beamline.into()))
    }

    pub async fn configurations(
        &self,
        filters: Vec<String>,
    ) -> Result<Vec<BeamlineConfiguration>, ConfigurationError> {
        let mut q = QueryBuilder::new("SELECT * FROM beamline WHERE name in (");
        let mut beamlines = q.separated(", ");
        for filter in filters {
            beamlines.push_bind(filter);
        }
        q.push(")");

        let query = q.build_query_as();
        Ok(query.fetch_all(&self.pool).await?)
    }

    pub async fn all_configurations(
        &self,
    ) -> Result<Vec<BeamlineConfiguration>, ConfigurationError> {
        Ok(query_as!(DbBeamlineConfig, "SELECT * FROM beamline")
            .fetch_all(&self.pool)
            .await?
            .into_iter()
            .map(BeamlineConfiguration::from)
            .collect())
    }

    pub async fn next_scan_configuration(
        &self,
        beamline: &str,
        current_high: Option<u32>,
    ) -> Result<BeamlineConfiguration, ConfigurationError> {
        let exp = current_high.unwrap_or(0);
        query_as!(
            DbBeamlineConfig,
            "UPDATE beamline SET scan_number = max(scan_number, ?) + 1 WHERE name = ? RETURNING *",
            exp,
            beamline
        )
        .fetch_optional(&self.pool)
        .await?
        .map(BeamlineConfiguration::from)
        .ok_or(ConfigurationError::MissingBeamline(beamline.into()))
    }

    /// Create a db service from a new empty/schema-less DB
    #[cfg(test)]
    pub(crate) async fn uninitialised() -> Self {
        Self {
            pool: SqlitePool::connect(":memory:").await.unwrap(),
        }
    }

    #[cfg(test)]
    pub(crate) async fn memory() -> Self {
        let pool = SqlitePool::connect(":memory:").await.unwrap();
        sqlx::migrate!().run(&pool).await.unwrap();
        Self { pool }
    }
}

impl fmt::Debug for SqliteScanPathService {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // This is a bit misleading when the 'db' field doesn't exist but is the most useful
        // information when debugging the state of the service
        f.debug_struct("SqliteScanPathService")
            .field("db", &self.pool.connect_options().get_filename())
            .finish()
    }
}

mod error {
    use derive_more::{Display, Error, From};

    #[derive(Debug, Display, Error, From)]
    pub enum ConfigurationError {
        #[display("No configuration available for beamline {_0:?}")]
        MissingBeamline(#[error(ignore)] String),
        #[display("Error reading configuration: {_0}")]
        Db(sqlx::Error),
    }

    #[derive(Debug, Display, From)]
    pub enum NewConfigurationError {
        #[display("Missing field {_0:?} for new configuration")]
        MissingField(String),
        #[from]
        #[display("Error inserting new configuration: {_0}")]
        Db(sqlx::Error),
    }

    impl From<&str> for NewConfigurationError {
        fn from(value: &str) -> Self {
            Self::MissingField(value.into())
        }
    }
}

#[cfg(test)]
mod db_tests {
    use rstest::rstest;
    use sqlx::error::{DatabaseError as _, ErrorKind};
    use sqlx::sqlite::SqliteError;
    use tokio::test;

    use super::SqliteScanPathService;
    use crate::db_service::error::{ConfigurationError, NewConfigurationError};
    use crate::db_service::{BeamlineConfiguration, BeamlineConfigurationUpdate};
    use crate::paths::{DetectorTemplate, PathSpec, ScanTemplate, VisitTemplate};

    /// Remove repeated .await.unwrap() noise from tests
    macro_rules! ok {
        ($call:expr) => {
            $call.await.unwrap()
        };
    }
    /// Remove repeated .await.unwrap_err() noise from tests
    /// Await the given future, unwrap the err and match it against the expected pattern
    macro_rules! err {
        ($call:expr) => {
            $call.await.unwrap_err()
        };
        ($exp:path, $call:expr) => {{
            let e = $call.await.unwrap_err();
            let $exp(e) = e else {
                panic!("Unexpected error: {e}");
            };
            e
        }};
    }

    fn update(bl: &str) -> BeamlineConfigurationUpdate {
        BeamlineConfigurationUpdate {
            name: bl.into(),
            scan_number: None,
            visit: VisitTemplate::new_checked("/tmp/{instrument}/data/{year}/{visit}").ok(),
            scan: ScanTemplate::new_checked("{subdirectory}/{instrument}-{scan_number}").ok(),
            detector: DetectorTemplate::new_checked(
                "{subdirectory}/{instrument}-{scan_number}-{detector}",
            )
            .ok(),
            tracker_file_extension: None,
        }
    }

    #[test]
    async fn empty_db_has_no_config() {
        let db = SqliteScanPathService::memory().await;
        let e = err!(
            ConfigurationError::MissingBeamline,
            db.current_configuration("i22")
        );
        assert_eq!(e, "i22")
    }

    #[rstest]
    #[case::visit("visit", |u: &mut BeamlineConfigurationUpdate| u.visit = None)]
    #[case::scan("scan", |u: &mut BeamlineConfigurationUpdate| u.scan = None)]
    #[case::scan("detector", |u: &mut BeamlineConfigurationUpdate| u.detector = None)]
    #[tokio::test]
    async fn new_beamline_with_missing_field(
        #[case] name: &str,
        #[case] init: impl FnOnce(&mut BeamlineConfigurationUpdate),
    ) {
        let db = SqliteScanPathService::memory().await;
        let mut update = update("i22");
        init(&mut update);
        let field = err!(NewConfigurationError::MissingField, update.insert_new(&db));
        assert_eq!(field, name);
    }

    #[rstest]
    #[case::directory(|u: &mut BeamlineConfigurationUpdate| u.tracker_file_extension = None)]
    #[case::scan_number(|u: &mut BeamlineConfigurationUpdate| u.scan_number = None)]
    #[tokio::test]
    async fn new_beamline_without_optional(
        #[case] init: impl FnOnce(&mut BeamlineConfigurationUpdate),
    ) {
        let db = SqliteScanPathService::memory().await;
        let mut update = update("i22");
        init(&mut update);
        let bc = ok!(update.insert_new(&db));
        assert_eq!(bc.name(), "i22");
    }

    #[test]
    async fn uninitialised_db_propagates_errors() {
        let db = SqliteScanPathService::uninitialised().await;
        let e = err!(NewConfigurationError::Db, update("i22").insert_new(&db));
        let e = e.into_database_error().unwrap().downcast::<SqliteError>();
        assert_eq!(e.kind(), ErrorKind::Other);
        // "1" is the magic number for 'Generic Error'
        assert_eq!(e.code(), Some("1".into()));
    }

    #[test]
    async fn duplicate_beamlines() {
        let db = SqliteScanPathService::memory().await;
        ok!(update("i22").insert_new(&db));
        let e = err!(NewConfigurationError::Db, update("i22").insert_new(&db));
        let e = e.into_database_error().unwrap().downcast::<SqliteError>();
        assert_eq!(e.kind(), ErrorKind::UniqueViolation);
    }

    #[test]
    async fn incrementing_scan_numbers() {
        let db = SqliteScanPathService::memory().await;
        ok!(update("i22").insert_new(&db));
        let s1 = ok!(db.next_scan_configuration("i22", None));
        let s2 = ok!(db.next_scan_configuration("i22", None));
        assert_eq!(s1.scan_number() + 1, s2.scan_number());
    }

    #[test]
    async fn overriding_scan_number_updates_db() {
        let db = SqliteScanPathService::memory().await;
        ok!(update("i22").with_scan_number(122).insert_new(&db));
        let s1 = ok!(db.next_scan_configuration("i22", None));
        let s2 = ok!(db.next_scan_configuration("i22", Some(1234)));
        let s3 = ok!(db.next_scan_configuration("i22", None));
        assert_eq!(s1.scan_number(), 123);
        assert_eq!(s2.scan_number(), 1235);
        assert_eq!(s3.scan_number(), 1236);
    }

    #[test]
    async fn lower_scan_override_is_ignored() {
        let db = SqliteScanPathService::memory().await;
        ok!(update("i22").with_scan_number(122).insert_new(&db));
        let s1 = ok!(db.next_scan_configuration("i22", Some(42)));
        assert_eq!(s1.scan_number(), 123);
    }

    #[test]
    async fn incrementing_missing_beamline() {
        let db = SqliteScanPathService::memory().await;
        let e = err!(
            ConfigurationError::MissingBeamline,
            db.next_scan_configuration("b21", None)
        );
        assert_eq!(e, "b21")
    }

    #[test]
    async fn current_configuration() {
        let db = SqliteScanPathService::memory().await;
        ok!(update("i22")
            .with_scan_number(122)
            .with_extension("ext")
            .insert_new(&db));
        let conf = ok!(db.current_configuration("i22"));
        let expected = BeamlineConfiguration {
            name: "i22".into(),
            scan_number: 122,
            visit: "/tmp/{instrument}/data/{year}/{visit}".into(),
            scan: "{subdirectory}/{instrument}-{scan_number}".into(),
            detector: "{subdirectory}/{instrument}-{scan_number}-{detector}".into(),
            tracker_file_extension: Some("ext".into()),
        };
        assert_eq!(conf, expected);
    }

    #[test]
    async fn configurations() {
        let db = SqliteScanPathService::memory().await;
        ok!(update("i22")
            .with_scan_number(122)
            .with_extension("ext")
            .insert_new(&db));
        ok!(update("i11")
            .with_scan_number(111)
            .with_extension("ext")
            .insert_new(&db));

        let mut confs = ok!(db.configurations(vec![
            "i22".to_string(),
            "i11".to_string(),
            "i03".to_string()
        ]));

        // Sort returned list as DB order is not guaranteed
        confs.sort_unstable_by_key(BeamlineConfiguration::scan_number);

        // i03 has not been configured so it will not fetch it.
        let expected = vec![
            BeamlineConfiguration {
                name: "i11".into(),
                scan_number: 111,
                visit: "/tmp/{instrument}/data/{year}/{visit}".into(),
                scan: "{subdirectory}/{instrument}-{scan_number}".into(),
                detector: "{subdirectory}/{instrument}-{scan_number}-{detector}".into(),
                tracker_file_extension: Some("ext".into()),
            },
            BeamlineConfiguration {
                name: "i22".into(),
                scan_number: 122,
                visit: "/tmp/{instrument}/data/{year}/{visit}".into(),
                scan: "{subdirectory}/{instrument}-{scan_number}".into(),
                detector: "{subdirectory}/{instrument}-{scan_number}-{detector}".into(),
                tracker_file_extension: Some("ext".into()),
            },
        ];
        assert_eq!(expected, confs);
    }

    #[test]
    async fn all_configurations() {
        let db = SqliteScanPathService::memory().await;
        ok!(update("i22")
            .with_scan_number(122)
            .with_extension("ext")
            .insert_new(&db));
        ok!(update("i11")
            .with_scan_number(111)
            .with_extension("ext")
            .insert_new(&db));

        let mut confs = ok!(db.all_configurations());

        // Sort returned list as DB order is not guaranteed
        confs.sort_unstable_by_key(BeamlineConfiguration::scan_number);

        // i03 has not been configured so it will not fetch it.
        assert_eq!(confs.len(), 2);
        let expected = vec![
            BeamlineConfiguration {
                name: "i11".into(),
                scan_number: 111,
                visit: "/tmp/{instrument}/data/{year}/{visit}".into(),
                scan: "{subdirectory}/{instrument}-{scan_number}".into(),
                detector: "{subdirectory}/{instrument}-{scan_number}-{detector}".into(),
                tracker_file_extension: Some("ext".into()),
            },
            BeamlineConfiguration {
                name: "i22".into(),
                scan_number: 122,
                visit: "/tmp/{instrument}/data/{year}/{visit}".into(),
                scan: "{subdirectory}/{instrument}-{scan_number}".into(),
                detector: "{subdirectory}/{instrument}-{scan_number}-{detector}".into(),
                tracker_file_extension: Some("ext".into()),
            },
        ];
        assert_eq!(expected, confs);
    }

    type Update = BeamlineConfigurationUpdate;

    #[rstest]
    #[case::visit(
            |u: &mut Update| u.visit = VisitTemplate::new_checked("/new/{instrument}/{proposal}/{visit}").ok(),
            |u: BeamlineConfiguration| assert_eq!(u.visit().unwrap().to_string(), "/new/{instrument}/{proposal}/{visit}"))]
    #[case::scan(
            |u: &mut Update| u.scan = ScanTemplate::new_checked("new-{scan_number}").ok(),
            |u: BeamlineConfiguration| assert_eq!(u.scan().unwrap().to_string(), "new-{scan_number}"))]
    #[case::detector(
            |u: &mut Update| u.detector = DetectorTemplate::new_checked("new-{scan_number}-{detector}").ok(),
            |u: BeamlineConfiguration| assert_eq!(u.detector().unwrap().to_string(), "new-{scan_number}-{detector}"))]
    #[case::scan_number(
            |u: &mut Update| u.scan_number = Some(42),
            |u: BeamlineConfiguration| assert_eq!(u.scan_number(), 42))]
    #[case::extension(
            |u: &mut Update| u.tracker_file_extension = Some("new".into()),
            |u: BeamlineConfiguration| assert_eq!(u.tracker_file_extension.unwrap(), "new"))]
    #[tokio::test]
    async fn update_existing(
        #[case] init: impl FnOnce(&mut BeamlineConfigurationUpdate),
        #[case] check: impl FnOnce(BeamlineConfiguration),
    ) {
        let db = SqliteScanPathService::memory().await;
        ok!(update("i22").insert_new(&db));
        let mut upd = BeamlineConfigurationUpdate::empty("i22");
        init(&mut upd);
        let bc = ok!(upd.update_beamline(&db)).expect("Updated beamline missing");
        check(bc)
    }

    #[tokio::test]
    async fn empty_update() {
        let db = SqliteScanPathService::memory().await;
        let upd = BeamlineConfigurationUpdate::empty("b21");
        assert!(ok!(upd.update_beamline(&db)).is_none());
    }
}
