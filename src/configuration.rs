#![deny(clippy::unwrap_used)]
use std::error::Error;
use std::fmt::Display;
use std::path::Path;

use inquire::Select;

use crate::cli::{BeamlineConfig, ConfigAction, TemplateAction, TemplateConfig, TemplateKind};
use crate::db_service::{SqliteScanPathService, TemplateOption};

pub async fn configure(db: &Path, opts: ConfigAction) -> Result<(), ConfigError> {
    let db = SqliteScanPathService::connect(db).await.unwrap();
    match opts {
        ConfigAction::Beamline(opts) => configure_beamline(&db, opts).await,
        ConfigAction::Template(opts) => configure_template(&db, opts).await,
    }
}

async fn configure_beamline(
    db: &SqliteScanPathService,
    opts: BeamlineConfig,
) -> Result<(), ConfigError> {
    println!("{opts:#?}");
    if let Some(visit) = opts.visit {
        let visit_id = set_template(&db, TemplateKind::Visit, visit).await?;
        db.set_visit_template(&opts.beamline, visit_id).await?;
    }
    if let Some(scan) = opts.scan {
        let scan_id = set_template(&db, TemplateKind::Scan, scan).await?;
        db.set_scan_template(&opts.beamline, scan_id).await?;
    }
    if let Some(detector) = opts.detector {
        let detector_id = set_template(&db, TemplateKind::Detector, detector).await?;
        db.set_detector_template(&opts.beamline, detector_id)
            .await?;
    }
    Ok(())
}

async fn configure_template(
    db: &SqliteScanPathService,
    opts: TemplateConfig,
) -> Result<(), ConfigError> {
    Ok(match opts.action {
        TemplateAction::Add { kind, template } => match kind {
            TemplateKind::Visit => {
                println!("Adding visit template: {template:?}");
                db.get_or_insert_visit_template(template).await?;
            }
            TemplateKind::Scan => {
                println!("Adding scan template: {template:?}");
                db.get_or_insert_scan_template(template).await?;
            }
            TemplateKind::Detector => {
                println!("Adding detector template: {template:?}");
                db.get_or_insert_detector_template(template).await?;
            }
        },
        TemplateAction::List { filter } => {
            if let Some(TemplateKind::Visit) | None = filter {
                list_templates("Visit", &db.get_visit_templates().await?)
            }
            if let Some(TemplateKind::Scan) | None = filter {
                list_templates("Scan", &db.get_scan_templates().await?)
            }
            if let Some(TemplateKind::Detector) | None = filter {
                list_templates("Detector", &db.get_detector_templates().await?)
            }
        }
    })
}

fn list_templates(heading: &str, templates: &[TemplateOption]) {
    println!("{heading} Templates:");
    for tmp in templates {
        println!("    {}", tmp);
    }
}

async fn set_template(
    db: &SqliteScanPathService,
    kind: TemplateKind,
    template: Option<String>,
) -> Result<i64, ConfigError> {
    match template {
        Some(template) => Ok(new_template(db, kind, template).await?),
        None => choose_template(db, kind).await,
    }
}

async fn new_template(
    db: &SqliteScanPathService,
    kind: TemplateKind,
    template: String,
) -> Result<i64, sqlx::Error> {
    match kind {
        TemplateKind::Visit => db.get_or_insert_visit_template(template).await,
        TemplateKind::Scan => db.get_or_insert_scan_template(template).await,
        TemplateKind::Detector => db.get_or_insert_detector_template(template).await,
    }
}

async fn choose_template(
    db: &SqliteScanPathService,
    kind: TemplateKind,
) -> Result<i64, ConfigError> {
    let templates = match kind {
        TemplateKind::Visit => db.get_visit_templates().await,
        TemplateKind::Scan => db.get_scan_templates().await,
        TemplateKind::Detector => db.get_detector_templates().await,
    }?;

    Select::new(&format!("Choose a {kind:?} template: "), templates)
        .prompt()
        .map(|t| t.id)
        .map_err(|_| ConfigError::Cancelled)
}

#[derive(Debug)]
pub enum ConfigError {
    Db(sqlx::Error),
    Cancelled,
}

impl From<sqlx::Error> for ConfigError {
    fn from(value: sqlx::Error) -> Self {
        Self::Db(value)
    }
}

impl Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigError::Db(db) => write!(f, "Error reading/writing to DB: {db}"),
            ConfigError::Cancelled => write!(f, "User cancelled operation"),
        }
    }
}

impl Error for ConfigError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            ConfigError::Db(db) => Some(db),
            ConfigError::Cancelled => None,
        }
    }
}
