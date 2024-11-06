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
use std::path::Path;

use error::UpdateSingleError;
use futures::Stream;
use sqlx::prelude::FromRow;
use sqlx::query::{Query, QueryScalar};
use sqlx::sqlite::{SqliteArguments, SqliteConnectOptions};
use sqlx::{query_file, query_file_as, query_file_scalar, query_scalar, Sqlite, SqlitePool};
use tracing::{debug, info, instrument, warn};

pub use self::error::{
    InsertTemplateError, SqliteNumberDirectoryError, SqliteNumberError, SqliteTemplateError,
};
use crate::cli::TemplateKind;
use crate::numtracker;
use crate::paths::{BeamlineField, DetectorField, InvalidKey, ScanField};
use crate::template::PathTemplate;

type SqliteTemplateResult<F> = Result<PathTemplate<F>, SqliteTemplateError>;

#[derive(Clone)]
pub struct SqliteScanPathService {
    pool: SqlitePool,
}

#[derive(Debug, FromRow)]
struct FallbackConfig {
    directory: Option<String>,
    extension: Option<String>,
}

#[derive(Debug)]
pub struct NumtrackerConfig {
    pub directory: String,
    pub extension: String,
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

    /// Execute a prepared query and parse the returned string into a [`PathTemplate`]
    async fn template_from<'bl, F: TryFrom<String, Error = InvalidKey>>(
        &self,
        query: QueryScalar<'bl, Sqlite, Option<String>, SqliteArguments<'bl>>,
    ) -> SqliteTemplateResult<F> {
        let template = query
            .fetch_optional(&self.pool)
            .await?
            .ok_or(SqliteTemplateError::BeamlineNotFound)?
            .ok_or(SqliteTemplateError::TemplateNotSet)?;

        debug!(template = template, "Template from DB");

        Ok(PathTemplate::new(template)?)
    }

    async fn update_single<'q>(
        &self,
        query: Query<'q, Sqlite, SqliteArguments<'q>>,
    ) -> Result<(), UpdateSingleError> {
        let mut trn = self.pool.begin().await?;
        let res = query.execute(&mut *trn).await?;
        match res.rows_affected() {
            0 => Err(UpdateSingleError::ZeroRecords),
            2.. => Err(UpdateSingleError::TooMany),
            _ => {
                trn.commit().await?;
                Ok(())
            }
        }
    }

    pub async fn next_scan_number(&self, beamline: &str) -> Result<usize, SqliteNumberError> {
        let next = self.db_scan_number(beamline).await?;
        let fallback = self.directory_scan_number(beamline).await;
        match fallback {
            Ok(n) if n != next => {
                warn!("Fallback numbering inconsistent. Expected: {next}, found {n}")
            }
            Err(e) => warn!("Error incrementing fallback directory number: {e}"),
            Ok(_) => {}
        }
        Ok(next)
    }

    pub async fn latest_scan_number(&self, beamline: &str) -> Result<usize, SqliteNumberError> {
        let number = query_file_scalar!("queries/latest_scan_number.sql", beamline)
            .fetch_optional(&self.pool)
            .await?
            .ok_or(SqliteNumberError::BeamlineNotFound)?;

        number
            .try_into()
            .map_err(|_| SqliteNumberError::InvalidValue(number))
    }

    /// Increment and return the latest scan number for the given beamline
    async fn db_scan_number(&self, beamline: &str) -> Result<usize, SqliteNumberError> {
        let mut db = self.pool.begin().await?;
        let next = query_file_scalar!("queries/increment_scan_number.sql", beamline)
            .fetch_optional(&mut *db)
            .await?
            .ok_or(SqliteNumberError::BeamlineNotFound)?;
        let next = next
            .try_into()
            .map_err(|_| SqliteNumberError::InvalidValue(next))?;
        debug!("Next scan number: {next}");
        db.commit().await?;
        Ok(next)
    }
    async fn directory_scan_number(
        &self,
        beamline: &str,
    ) -> Result<usize, SqliteNumberDirectoryError> {
        match self.number_tracker_directory(beamline).await? {
            Some(nc) => Ok(numtracker::increment_and_get(nc.directory, &nc.extension).await?),
            None => Err(SqliteNumberDirectoryError::NotConfigured),
        }
    }
    #[instrument]
    pub async fn visit_directory_template(
        &self,
        beamline: &str,
    ) -> SqliteTemplateResult<BeamlineField> {
        self.template_from(query_file_scalar!("queries/visit_template.sql", beamline))
            .await
    }
    #[instrument]
    pub async fn scan_file_template(&self, beamline: &str) -> SqliteTemplateResult<ScanField> {
        self.template_from(query_file_scalar!("queries/scan_template.sql", beamline))
            .await
    }
    #[instrument]
    pub async fn detector_file_template(
        &self,
        beamline: &str,
    ) -> SqliteTemplateResult<DetectorField> {
        self.template_from(query_file_scalar!(
            "queries/detector_template.sql",
            beamline
        ))
        .await
    }

    pub fn beamlines(&self) -> impl Stream<Item = Result<String, sqlx::Error>> + '_ {
        query_file_scalar!("queries/all_beamlines.sql").fetch(&self.pool)
    }

    pub async fn number_tracker_directory(
        &self,
        beamline: &str,
    ) -> Result<Option<NumtrackerConfig>, sqlx::Error> {
        debug!("Getting number_tracker_directory for {beamline}");
        let fallback = query_file_as!(
            FallbackConfig,
            "queries/number_file_directory.sql",
            beamline
        )
        .fetch_optional(&self.pool)
        .await?;
        Ok(fallback.and_then(|fb| match (fb.directory, fb.extension) {
            (None, None) => None,
            (None, Some(_)) => None, // this should be unreachable due to table constraints
            (Some(dir), None) => Some(NumtrackerConfig {
                directory: dir,
                extension: beamline.into(),
            }),
            (Some(dir), Some(ext)) => Some(NumtrackerConfig {
                directory: dir,
                extension: ext,
            }),
        }))
    }

    pub async fn set_scan_number(&self, bl: &str, number: usize) -> Result<(), SqliteNumberError> {
        let number = number as i64;
        debug!(
            beamline = bl,
            scan_number = number,
            "Setting scan number directly"
        );
        match self
            .update_single(query_file!("queries/set_scan_number.sql", number, bl))
            .await
        {
            Ok(_) => Ok(()),
            Err(UpdateSingleError::Db(e)) => Err(e.into()),
            Err(UpdateSingleError::ZeroRecords) => Err(SqliteNumberError::BeamlineNotFound),
            Err(UpdateSingleError::TooMany) => {
                // This should never happen if the schema constraints are set correctly
                Err(SqliteNumberError::DuplicateBeamlineError)
            }
        }
    }

    pub async fn set_beamline_template(
        &self,
        bl: &str,
        kind: TemplateKind,
        template: &str,
    ) -> Result<(), InsertTemplateError> {
        debug!(
            beamline = bl,
            template, "Setting beamline {kind:?} template"
        );
        kind.validate(template)?;
        let update = match kind {
            TemplateKind::Visit => {
                self.update_single(query_file!("queries/set_visit_template.sql", template, bl))
                    .await
            }
            TemplateKind::Scan => {
                self.update_single(query_file!("queries/set_scan_template.sql", template, bl))
                    .await
            }
            TemplateKind::Detector => {
                self.update_single(query_file!(
                    "queries/set_detector_template.sql",
                    template,
                    bl
                ))
                .await
            }
        };
        update.map_err(|_| InsertTemplateError::MissingBeamline(bl.into()))
    }

    pub async fn insert_beamline(&self, beamline: &str) -> Result<(), sqlx::Error> {
        query_file!("queries/insert_beamline.sql", beamline)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn get_templates(&self, kind: TemplateKind) -> sqlx::Result<Vec<String>> {
        let templates = match kind {
            TemplateKind::Visit => {
                query_scalar!("SELECT DISTINCT visit FROM beamline;")
                    .fetch_all(&self.pool)
                    .await
            }
            TemplateKind::Scan => {
                query_scalar!("SELECT DISTINCT scan FROM beamline;")
                    .fetch_all(&self.pool)
                    .await
            }
            TemplateKind::Detector => {
                query_scalar!("SELECT DISTINCT detector FROM beamline;")
                    .fetch_all(&self.pool)
                    .await
            }
        };
        templates.map(|t| t.into_iter().flatten().collect::<Vec<_>>())
    }

    #[cfg(test)]
    async fn ro_memory() -> Self {
        // only read-write so that migrations can be applied
        let opts = SqliteConnectOptions::new().in_memory(true);
        let pool = SqlitePool::connect_with(opts).await.unwrap();
        sqlx::migrate!().run(&pool).await.unwrap();
        // ... then set to read-only
        pool.set_connect_options(SqliteConnectOptions::new().read_only(true));
        Self { pool }
    }

    #[cfg(test)]
    async fn memory() -> Self {
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
    use std::error::Error;
    use std::fmt::{self, Display};

    use crate::paths::InvalidPathTemplate;
    use crate::template::PathTemplateError;

    /// Something that went wrong in the chain of querying the database for a template and
    /// converting it into a usable template.
    #[derive(Debug)]
    pub enum SqliteTemplateError {
        /// It wasn't possible to get the requested template from the database.
        ConnectionError(sqlx::Error),
        /// There is no template available for the requested beamline
        BeamlineNotFound,
        /// The template was present in the database but it could not be parsed into a valid
        /// [`PathTemplate`].
        Invalid(PathTemplateError),
        /// The requested template is not set for the requested beamline
        TemplateNotSet,
    }

    impl Display for SqliteTemplateError {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            match self {
                Self::ConnectionError(e) => write!(f, "Could not access database: {e}"),
                Self::BeamlineNotFound => f.write_str("Beamline configuration not found"),
                Self::TemplateNotSet => f.write_str("No template set for beamline"),
                Self::Invalid(e) => write!(f, "Template is not valid: {e}"),
            }
        }
    }

    impl Error for SqliteTemplateError {}

    impl From<sqlx::Error> for SqliteTemplateError {
        fn from(sql: sqlx::Error) -> Self {
            Self::ConnectionError(sql)
        }
    }

    impl From<PathTemplateError> for SqliteTemplateError {
        fn from(err: PathTemplateError) -> Self {
            Self::Invalid(err)
        }
    }

    #[derive(Debug)]
    pub enum SqliteNumberError {
        BeamlineNotFound,
        DuplicateBeamlineError,
        ConnectionError(sqlx::Error),
        InvalidValue(i64),
    }

    impl Display for SqliteNumberError {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            match self {
                Self::BeamlineNotFound => f.write_str("No scan number configured for beamline"),
                Self::ConnectionError(e) => write!(f, "Could not access DB: {e}"),
                Self::InvalidValue(v) => write!(f, "Scan number {v} in DB is not valid"),
                Self::DuplicateBeamlineError => {
                    write!(f, "Invalid DB state - multiple entries for beamline")
                }
            }
        }
    }

    impl Error for SqliteNumberError {
        fn source(&self) -> Option<&(dyn Error + 'static)> {
            match self {
                Self::BeamlineNotFound | Self::InvalidValue(_) | Self::DuplicateBeamlineError => {
                    None
                }
                Self::ConnectionError(e) => Some(e),
            }
        }
    }

    impl From<sqlx::Error> for SqliteNumberError {
        fn from(value: sqlx::Error) -> Self {
            Self::ConnectionError(value)
        }
    }

    #[derive(Debug)]
    pub enum SqliteNumberDirectoryError {
        /// There is no directory configured for the requested beamline
        NotConfigured,
        /// The DB could not be accessed or queried
        NotAccessible(sqlx::Error),
        /// The directory was not present or not readable
        NotReabable(std::io::Error),
    }

    impl Display for SqliteNumberDirectoryError {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            match self {
                Self::NotConfigured => {
                    f.write_str("No directory configured for the given beamline")
                }
                Self::NotAccessible(e) => e.fmt(f),
                Self::NotReabable(e) => e.fmt(f),
            }
        }
    }

    impl Error for SqliteNumberDirectoryError {
        fn source(&self) -> Option<&(dyn Error + 'static)> {
            match self {
                Self::NotConfigured => None,
                Self::NotAccessible(e) => Some(e),
                Self::NotReabable(e) => Some(e),
            }
        }
    }

    impl From<std::io::Error> for SqliteNumberDirectoryError {
        fn from(value: std::io::Error) -> Self {
            Self::NotReabable(value)
        }
    }

    impl From<sqlx::Error> for SqliteNumberDirectoryError {
        fn from(value: sqlx::Error) -> Self {
            Self::NotAccessible(value)
        }
    }

    #[derive(Debug)]
    pub enum InsertTemplateError {
        Db(sqlx::Error),
        Invalid(InvalidPathTemplate),
        MissingBeamline(String),
    }

    impl Display for InsertTemplateError {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            match self {
                InsertTemplateError::Db(e) => write!(f, "Error inserting template: {e}"),
                InsertTemplateError::Invalid(e) => write!(f, "Template was not valid: {e}"),
                InsertTemplateError::MissingBeamline(bl) => {
                    write!(f, "No configuration for beamline {bl:?}")
                }
            }
        }
    }

    impl Error for InsertTemplateError {
        fn source(&self) -> Option<&(dyn Error + 'static)> {
            match self {
                InsertTemplateError::Db(e) => Some(e),
                InsertTemplateError::Invalid(e) => Some(e),
                InsertTemplateError::MissingBeamline(_) => None,
            }
        }
    }

    impl From<InvalidPathTemplate> for InsertTemplateError {
        fn from(value: InvalidPathTemplate) -> Self {
            Self::Invalid(value)
        }
    }

    impl From<sqlx::Error> for InsertTemplateError {
        fn from(value: sqlx::Error) -> Self {
            Self::Db(value)
        }
    }

    pub enum UpdateSingleError {
        Db(sqlx::Error),
        ZeroRecords,
        TooMany,
    }

    impl From<sqlx::Error> for UpdateSingleError {
        fn from(value: sqlx::Error) -> Self {
            Self::Db(value)
        }
    }
}

#[cfg(test)]
mod db_tests {
    use futures::StreamExt;
    use tokio::test;

    use super::SqliteScanPathService;
    use crate::cli::TemplateKind;
    use crate::db_service::{InsertTemplateError, SqliteNumberError, SqliteTemplateError};
    use crate::paths::InvalidPathTemplate;

    /// Remove repeated .await.unwrap() noise from tests
    macro_rules! ok {
        ($call:expr) => {
            $call.await.unwrap()
        };
    }
    /// Remove repeated .await.unwrap_err() noise from tests
    macro_rules! err {
        ($call:expr) => {
            $call.await.unwrap_err()
        };
    }

    #[test]
    async fn insert_invalid_visit_template() {
        let db = SqliteScanPathService::memory().await;
        let e = err!(db.set_beamline_template(
            "i22",
            TemplateKind::Visit,
            "/no/instrument/segment/for/{visit}".into()
        ));
        assert!(matches!(
            e,
            InsertTemplateError::Invalid(InvalidPathTemplate::MissingField(_)),
        ))
    }

    #[test]
    async fn read_only_fails() {
        let db = SqliteScanPathService::ro_memory().await;
        let e = err!(db.insert_beamline("i22"));
        assert!(matches!(e, sqlx::Error::Database(_)))
    }

    #[test]
    async fn insert_beamline() {
        let db = SqliteScanPathService::memory().await;
        let beamlines = db.beamlines().collect::<Vec<_>>().await;
        assert!(beamlines.is_empty());
        ok!(db.insert_beamline("i22"));

        let beamlines = db.beamlines().collect::<Vec<_>>().await;
        assert_eq!(beamlines.len(), 1);
        assert_eq!(beamlines[0].as_deref().unwrap(), "i22");
    }

    #[test]
    async fn scan_numbers() {
        let db = SqliteScanPathService::memory().await;
        ok!(db.insert_beamline("i22"));
        assert_eq!(ok!(db.next_scan_number("i22")), 1);
        assert_eq!(ok!(db.next_scan_number("i22")), 2);

        ok!(db.set_scan_number("i22", 122));
        assert_eq!(ok!(db.next_scan_number("i22")), 123);
        assert_eq!(ok!(db.next_scan_number("i22")), 124);
        assert_eq!(ok!(db.latest_scan_number("i22")), 124);
    }

    #[test]
    async fn latest_scan_number_for_missing_beamline() {
        let db = SqliteScanPathService::memory().await;
        let e = err!(db.latest_scan_number("i22"));
        let SqliteNumberError::BeamlineNotFound = e else {
            panic!("Unexpected error when beamline is missing: {e}")
        };
    }

    #[test]
    async fn inc_scan_number_for_missing_beamline() {
        let db = SqliteScanPathService::memory().await;
        let e = err!(db.next_scan_number("i22"));
        let SqliteNumberError::BeamlineNotFound = e else {
            panic!("Unexpected error when beamline is missing: {e}")
        };
    }

    #[test]
    async fn visit_template_for_missing_beamline() {
        let db = SqliteScanPathService::memory().await;
        let e = err!(db.visit_directory_template("i22"));
        let SqliteTemplateError::BeamlineNotFound = e else {
            panic!("Unexpected error for missing beamline: {e}")
        };

        err!(db.set_beamline_template("i22", TemplateKind::Visit, "/{instrument}/{visit}"));
    }

    #[test]
    async fn get_set_visit_template() {
        let db = SqliteScanPathService::memory().await;
        ok!(db.insert_beamline("i22"));
        ok!(db.set_beamline_template(
            "i22",
            TemplateKind::Visit,
            "/tmp/{instrument}/data/{year}/{visit}".into()
        ));
        let t = ok!(db.visit_directory_template("i22"));
        assert_eq!(t.to_string(), "/tmp/{instrument}/data/{year}/{visit}")
    }

    #[test]
    async fn get_set_scan_template() {
        let db = SqliteScanPathService::memory().await;
        ok!(db.insert_beamline("i22"));
        ok!(db.set_beamline_template(
            "i22",
            TemplateKind::Scan,
            "{instrument}_{scan_number}".into()
        ));
        let t = ok!(db.scan_file_template("i22"));
        assert_eq!(t.to_string(), "{instrument}_{scan_number}")
    }

    #[test]
    async fn get_set_detector_template() {
        let db = SqliteScanPathService::memory().await;
        ok!(db.insert_beamline("i22"));
        ok!(db.set_beamline_template(
            "i22",
            TemplateKind::Detector,
            "{instrument}_{scan_number}_{detector}".into()
        ));
        let t = ok!(db.detector_file_template("i22"));
        assert_eq!(t.to_string(), "{instrument}_{scan_number}_{detector}")
    }

    #[test]
    async fn get_templates() {
        let db = SqliteScanPathService::memory().await;
        ok!(db.insert_beamline("i22"));
        ok!(db.set_beamline_template("i22", TemplateKind::Visit, "/{instrument}/{visit}".into()));
        ok!(db.set_beamline_template("i22", TemplateKind::Scan, "{scan_number}".into()));
        ok!(db.set_beamline_template(
            "i22",
            TemplateKind::Detector,
            "{scan_number}_{detector}".into()
        ));

        assert_eq!(
            ok!(db.get_templates(TemplateKind::Visit))
                .into_iter()
                .map(|t| t.to_string())
                .collect::<Vec<_>>(),
            &["/{instrument}/{visit}"]
        );
        assert_eq!(
            ok!(db.get_templates(TemplateKind::Scan))
                .into_iter()
                .map(|t| t.to_string())
                .collect::<Vec<_>>(),
            &["{scan_number}"]
        );
        assert_eq!(
            ok!(db.get_templates(TemplateKind::Detector))
                .into_iter()
                .map(|t| t.to_string())
                .collect::<Vec<_>>(),
            &["{scan_number}_{detector}"]
        );
    }
}
