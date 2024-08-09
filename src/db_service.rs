use std::fmt;

use sqlx::query::QueryScalar;
use sqlx::sqlite::SqliteArguments;
use sqlx::{query_file_scalar, Sqlite, SqlitePool};

pub use self::error::SqliteTemplateError;
use crate::paths::{BeamlineField, DetectorField, InvalidKey, ScanField};
use crate::template::PathTemplate;

type SqliteTemplateResult<F> = Result<PathTemplate<F>, SqliteTemplateError>;

#[derive(Clone)]
pub struct SqliteScanPathService {
    pub pool: SqlitePool,
}

impl SqliteScanPathService {
    pub async fn connect(host: &str) -> Result<Self, sqlx::Error> {
        let pool = SqlitePool::connect(host).await?;
        sqlx::migrate!().run(&pool).await?;
        Ok(Self { pool })
    }

    /// Increment and return the latest scan number for the given beamline
    pub async fn next_scan_number(&self, beamline: &str) -> Result<usize, sqlx::Error> {
        let mut db = self.pool.begin().await?;
        let next = query_file_scalar!("queries/increment_scan_number.sql", beamline)
            .fetch_one(&mut *db)
            .await? as usize;
        db.commit().await?;
        Ok(next)
    }

    pub async fn visit_template(&self, bl: &str) -> SqliteTemplateResult<BeamlineField> {
        self.template_from(query_file_scalar!("queries/visit_template.sql", bl))
            .await
    }
    pub async fn scan_template(&self, bl: &str) -> SqliteTemplateResult<ScanField> {
        self.template_from(query_file_scalar!("queries/scan_template.sql", bl))
            .await
    }
    pub async fn detector_template(&self, bl: &str) -> SqliteTemplateResult<DetectorField> {
        self.template_from(query_file_scalar!("queries/detector_template.sql", bl))
            .await
    }

    /// Execute a prepared query and parse the returned string into a [`PathTemplate`]
    async fn template_from<'bl, F: TryFrom<String, Error = InvalidKey>>(
        &self,
        query: QueryScalar<'bl, Sqlite, String, SqliteArguments<'bl>>,
    ) -> SqliteTemplateResult<F> {
        Ok(PathTemplate::new(query.fetch_one(&self.pool).await?)?)
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
        /// It may not be present or there may have been a connection problem accessing the
        /// database.
        Unavailable(sqlx::Error),
        /// The template was present in the database but it could not be parsed into a valid
        /// [`PathTemplate`].
        Invalid(PathTemplateError<InvalidKey>),
    }

    impl Display for SqliteTemplateError {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            match self {
                Self::Unavailable(e) => write!(f, "Could not retrieve template: {e}"),
                Self::Invalid(e) => write!(f, "Template is not valid: {e}"),
            }
        }
    }

    impl Error for SqliteTemplateError {}

    impl From<sqlx::Error> for SqliteTemplateError {
        fn from(sql: sqlx::Error) -> Self {
            Self::Unavailable(sql)
        }
    }

    impl From<PathTemplateError<InvalidKey>> for SqliteTemplateError {
        fn from(err: PathTemplateError<InvalidKey>) -> Self {
            Self::Invalid(err)
        }
    }
}
