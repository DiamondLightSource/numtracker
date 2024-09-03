use std::error::Error;
use std::fmt::Display;
use std::path::Path;

use inquire::Select;
use numtracker::db_service::SqliteScanPathService;

use crate::cli::{ConfigAction, TemplateAction, TemplateKind};

pub async fn configure(db: &Path, opts: ConfigAction) -> Result<(), ConfigError> {
    let db = SqliteScanPathService::connect(db).await.unwrap();
    match opts {
        ConfigAction::Beamline(opts) => {
            println!("{}", opts.beamline);
            println!("{opts:#?}");
            if let Some(visit) = opts.visit {
                set_template(&db, &opts.beamline, TemplateKind::Visit, visit).await?;
            }
            if let Some(scan) = opts.scan {
                set_template(&db, &opts.beamline, TemplateKind::Scan, scan).await?;
            }
            if let Some(detector) = opts.detector {
                set_template(&db, &opts.beamline, TemplateKind::Detector, detector).await?;
            }
        }
        ConfigAction::Template(opts) => match opts.action {
            TemplateAction::Add { kind, template } => match kind {
                TemplateKind::Visit => println!("Adding visit: {template:?}"),
                TemplateKind::Scan => println!("Adding scan: {template:?}"),
                TemplateKind::Detector => println!("Adding detector: {template:?}"),
            },
            TemplateAction::List { filter } => match filter {
                Some(TemplateKind::Visit) => println!("Listing visit"),
                Some(TemplateKind::Scan) => println!("Listing scan"),
                Some(TemplateKind::Detector) => println!("Listing detector"),
                None => println!("Listing all"),
            },
        },
    };
    Ok(())
}

async fn set_template(
    db: &SqliteScanPathService,
    bl: &str,
    kind: TemplateKind,
    template: Option<String>,
) -> Result<(), ConfigError> {
    let template = match template {
        Some(template) => new_template(db, kind, template).await?,
        None => choose_template(db, kind).await?,
    };
    println!("Setting {bl} {kind:?} to {template:?}");
    Ok(())
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
