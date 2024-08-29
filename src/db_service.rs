use std::fmt;
use std::path::Path;

use futures::Stream;
use sqlx::prelude::FromRow;
use sqlx::query::QueryScalar;
use sqlx::sqlite::{SqliteArguments, SqliteConnectOptions};
use sqlx::{query_file, query_file_as, query_file_scalar, Sqlite, SqlitePool};
use tracing::{debug, info, instrument, warn};

pub use self::error::{SqliteNumberDirectoryError, SqliteNumberError, SqliteTemplateError};
use crate::numtracker;
use crate::paths::{BeamlineField, DetectorField, InvalidKey, ScanField};
use crate::template::PathTemplate;

type SqliteTemplateResult<F> = Result<PathTemplate<F>, SqliteTemplateError>;

#[derive(Clone)]
pub struct SqliteScanPathService {
    pool: SqlitePool,
}

#[derive(Debug, FromRow)]
pub struct NumtrackerConfig {
    directory: String,
    extension: String,
}

impl NumtrackerConfig {
    pub fn display(&self) -> String {
        format!("{}/*.{}", self.directory, self.extension)
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

    /// Execute a prepared query and parse the returned string into a [`PathTemplate`]
    async fn template_from<'bl, F: TryFrom<String, Error = InvalidKey>>(
        &self,
        query: QueryScalar<'bl, Sqlite, String, SqliteArguments<'bl>>,
    ) -> SqliteTemplateResult<F> {
        let template = query
            .fetch_optional(&self.pool)
            .await?
            .ok_or(SqliteTemplateError::BeamlineNotFound)?;
        debug!(template = template, "Template from DB");

        Ok(PathTemplate::new(template)?)
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
            Some(nc) => Ok(numtracker::increment_and_get(&nc.directory, &nc.extension).await?),
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
        query_file_as!(
            NumtrackerConfig,
            "queries/number_file_directory.sql",
            beamline
        )
        .fetch_optional(&self.pool)
        .await
    }

    pub async fn set_scan_number(&self, bl: &str, current_file: usize) -> Result<(), sqlx::Error> {
        let current_file = current_file as i64;
        query_file!("queries/set_scan_number.sql", current_file, bl)
            .execute(&self.pool)
            .await?;
        Ok(())
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

    use crate::paths::InvalidKey;
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
        Invalid(PathTemplateError<InvalidKey>),
    }

    impl Display for SqliteTemplateError {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            match self {
                Self::ConnectionError(e) => write!(f, "Could not access database: {e}"),
                Self::BeamlineNotFound => f.write_str("No template found for beamline"),
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

    impl From<PathTemplateError<InvalidKey>> for SqliteTemplateError {
        fn from(err: PathTemplateError<InvalidKey>) -> Self {
            Self::Invalid(err)
        }
    }

    #[derive(Debug)]
    pub enum SqliteNumberError {
        BeamlineNotFound,
        ConnectionError(sqlx::Error),
        InvalidValue(i64),
    }

    impl Display for SqliteNumberError {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            match self {
                Self::BeamlineNotFound => f.write_str("No scan number configured for beamline"),
                Self::ConnectionError(e) => write!(f, "Could not access DB: {e}"),
                Self::InvalidValue(v) => write!(f, "Scan number {v} in DB is not valid"),
            }
        }
    }

    impl Error for SqliteNumberError {
        fn source(&self) -> Option<&(dyn Error + 'static)> {
            match self {
                Self::BeamlineNotFound | Self::InvalidValue(_) => None,
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
}
