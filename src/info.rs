use std::error::Error;
use std::fmt::Display;
use std::path::Path;

use futures::TryStreamExt as _;

use crate::db_service::{NumtrackerConfig, SqliteScanPathService};
use crate::numtracker::GdaNumTracker;

pub async fn list_info(db: &Path, beamline: Option<&str>) {
    let db = SqliteScanPathService::connect(db).await.unwrap();
    if let Some(bl) = beamline {
        list_bl_info(&db, bl).await;
    } else {
        let mut all = db.beamlines();
        while let Ok(Some(bl)) = all.try_next().await {
            list_bl_info(&db, &bl).await;
        }
    }
}

fn bl_field<F: Display, E: Error>(field: &str, value: Result<F, E>) {
    match value {
        Ok(value) => println!("    {field}: {value}"),
        Err(e) => println!("    {field} not available: {e}"),
    }
}

async fn list_bl_info(db: &SqliteScanPathService, bl: &str) {
    println!("{bl}");
    bl_field("Visit", db.visit_directory_template(bl).await);
    bl_field("Scan", db.scan_file_template(bl).await);
    bl_field("Detector", db.detector_file_template(bl).await);
    bl_field("Scan number", db.latest_scan_number(bl).await);
    if let Some(fallback) = db.number_tracker_directory(bl).await.transpose() {
        match fallback {
            Ok(NumtrackerConfig {
                directory,
                extension,
            }) => match GdaNumTracker::new(&directory)
                .latest_scan_number(&extension)
                .await
            {
                Ok(latest) => println!("    Numtracker file: {directory}/{latest}.{extension}"),
                Err(e) => println!("    Numtracker file unavailable: {e}"),
            },
            Err(e) => println!("    Could not read fallback numtracker directory: {e}"),
        }
    }
}
