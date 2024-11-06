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

use std::error::Error;
use std::fmt::Display;
use std::path::Path;

use inquire::Select;

use crate::cli::TemplateKind::*;
use crate::cli::{BeamlineConfig, ConfigAction, TemplateKind};
use crate::db_service::{InsertTemplateError, SqliteScanPathService};
use crate::paths::InvalidPathTemplate;

pub async fn configure(db: &Path, opts: ConfigAction) -> Result<(), ConfigError> {
    let db = SqliteScanPathService::connect(db).await?;
    match opts {
        ConfigAction::Beamline(opts) => configure_beamline(&db, opts).await,
    }
}

async fn configure_beamline(
    db: &SqliteScanPathService,
    opts: BeamlineConfig,
) -> Result<(), ConfigError> {
    println!("{opts:#?}");
    // ensure the beamline is present but we don't care about the ID
    let _ = db.insert_beamline(&opts.beamline).await?;

    if let Some(visit) = opts.visit {
        let visit = set_template(db, Visit, visit).await?;
        db.set_beamline_template(&opts.beamline, Visit, &visit)
            .await?;
    }
    if let Some(scan) = opts.scan {
        let scan = set_template(db, Scan, scan).await?;
        db.set_beamline_template(&opts.beamline, Scan, &scan)
            .await?;
    }
    if let Some(detector) = opts.detector {
        let detector = set_template(db, Detector, detector).await?;
        db.set_beamline_template(&opts.beamline, Detector, &detector)
            .await?;
    }
    Ok(())
}

async fn set_template(
    db: &SqliteScanPathService,
    kind: TemplateKind,
    template: Option<String>,
) -> Result<String, ConfigError> {
    match template {
        Some(template) => Ok(template),
        None => choose_template(db, kind).await,
    }
}

async fn choose_template(
    db: &SqliteScanPathService,
    kind: TemplateKind,
) -> Result<String, ConfigError> {
    let templates = db.get_templates(kind).await?;
    if templates.is_empty() {
        return Err(ConfigError::NoTemplates);
    }

    Select::new(&format!("Choose a {kind:?} template: "), templates)
        .prompt()
        .map_err(|_| ConfigError::Cancelled)
}

#[derive(Debug)]
pub enum ConfigError {
    Cancelled,
    Db(sqlx::Error),
    InvalidTemplate(InvalidPathTemplate),
    MissingBeamline(String),
    NoTemplates,
}

impl From<sqlx::Error> for ConfigError {
    fn from(value: sqlx::Error) -> Self {
        Self::Db(value)
    }
}

impl From<InsertTemplateError> for ConfigError {
    fn from(value: InsertTemplateError) -> Self {
        match value {
            InsertTemplateError::Db(e) => Self::Db(e),
            InsertTemplateError::Invalid(e) => Self::InvalidTemplate(e),
            InsertTemplateError::MissingBeamline(bl) => Self::MissingBeamline(bl),
        }
    }
}

impl Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigError::Cancelled => write!(f, "User cancelled operation"),
            ConfigError::Db(db) => write!(f, "Error reading/writing to DB: {db}"),
            ConfigError::InvalidTemplate(e) => write!(f, "Template was not valid: {e}"),
            ConfigError::MissingBeamline(bl) => write!(f, "Beamline {bl:?} is not configured"),
            ConfigError::NoTemplates => f.write_str("No templates available"),
        }
    }
}

impl Error for ConfigError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Cancelled | Self::MissingBeamline(_) | Self::NoTemplates => None,
            Self::Db(db) => Some(db),
            Self::InvalidTemplate(e) => Some(e),
        }
    }
}
