use std::path::Path;

use inquire::Select;
use numtracker::db_service::SqliteScanPathService;

use crate::cli::{ConfigAction, TemplateAction, TemplateKind};

pub async fn configure(db: &Path, opts: ConfigAction) {
    let db = SqliteScanPathService::connect(db).await.unwrap();
    match opts {
        ConfigAction::Beamline(opts) => {
            println!("{}", opts.beamline);
            println!("{opts:#?}");
            if let Some(visit) = opts.visit {
                set_template(&db, &opts.beamline, TemplateKind::Visit, visit).await;
            }
            if let Some(scan) = opts.scan {
                set_template(&db, &opts.beamline, TemplateKind::Scan, scan).await;
            }
            if let Some(detector) = opts.detector {
                set_template(&db, &opts.beamline, TemplateKind::Detector, detector).await;
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
}

async fn set_template(
    db: &SqliteScanPathService,
    bl: &str,
    kind: TemplateKind,
    template: Option<String>,
) {
    let template = match template {
        Some(template) => new_template(db, kind, template).await,
        None => choose_template(db, kind).await.transpose().unwrap(),
    };
    println!("Setting {bl} {kind:?} to {template:?}")
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
) -> Result<Option<i64>, sqlx::Error> {
    let templates = match kind {
        TemplateKind::Visit => db.get_visit_templates().await,
        TemplateKind::Scan => db.get_scan_templates().await,
        TemplateKind::Detector => db.get_detector_templates().await,
    }?;

    let template = Select::new(&format!("Choose a {kind:?} template: "), templates).prompt();
    Ok(template.ok().map(|to| to.id))
}
