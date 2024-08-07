use std::fmt::Debug;

use sqlx::{query_as, query_scalar, FromRow, Pool, Sqlite, SqlitePool};

#[derive(Clone)]
pub struct SqliteScanPathService {
    pub pool: Pool<Sqlite>,
}

#[derive(Debug, FromRow)]
pub struct ScanTemplates {
    visit: String,
    scan: String,
    detector: String,
}

impl Debug for SqliteScanPathService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SqliteScanPathService")
            .field("db", &self.pool.connect_options().get_filename())
            .finish()
    }
}

impl SqliteScanPathService {
    pub async fn connect(host: &str) -> Result<Self, sqlx::Error> {
        let pool = SqlitePool::connect(host).await?;
        sqlx::migrate!().run(&pool).await?;
        Ok(Self { pool })
    }
    pub async fn next_scan_number(&self, beamline: &str) -> Result<usize, sqlx::Error> {
        let mut db = self.pool.begin().await?;
        let next = query_scalar!(r#"
            UPDATE scan_number
            SET last_number = number + 1
            FROM (
                SELECT beamline.id AS bl_id, beamline.name AS name, scan_number.last_number AS number
                FROM scan_number
                    JOIN beamline ON scan_number.beamline = beamline.id
                WHERE beamline.name=?
            )
            WHERE beamline = bl_id
            RETURNING last_number
            "#,
            beamline
        ).fetch_one(&mut *db)
            .await? as usize;
        db.commit().await?;
        Ok(next)
    }

    pub async fn visit_template(&self, beamline: &str) -> Result<String, sqlx::Error> {
        query_scalar!(
            "SELECT template FROM beamline_visit_template WHERE beamline = ?",
            beamline
        )
        .fetch_one(&self.pool)
        .await
    }

    pub async fn scan_templates(&self, beamline: &str) -> Result<ScanTemplates, sqlx::Error> {
        query_as!(
            ScanTemplates,
            "SELECT visit, scan, detector FROM beamline_template WHERE beamline = ?",
            beamline
        )
        .fetch_one(&self.pool)
        .await
    }
}
