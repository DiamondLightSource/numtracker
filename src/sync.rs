use std::fmt::Display;
use std::path::Path;

use futures::TryStreamExt as _;
use inquire::list_option::ListOption;
use numtracker::db_service::{NumtrackerConfig, SqliteScanPathService};
use numtracker::numtracker::GdaNumTracker;
use tokio::sync::oneshot;

use crate::cli::{SyncMode, SyncOptions};

pub async fn sync_directories(db: &Path, opts: SyncOptions) {
    let db = SqliteScanPathService::connect(db).await.unwrap();

    if let Some(bl) = opts.beamline() {
        sync_beamline_directory(&db, bl, opts.mode).await;
    } else {
        let mut all = db.beamlines();
        while let Ok(Some(bl)) = all.try_next().await {
            sync_beamline_directory(&db, &bl, opts.mode).await;
        }
    }
}

async fn sync_beamline_directory(db: &SqliteScanPathService, bl: &str, mode: Option<SyncMode>) {
    let Some(NumtrackerConfig {
        directory,
        extension,
    }) = db.number_tracker_directory(bl).await.unwrap()
    else {
        println!("Directory not configured for {bl}");
        return;
    };
    let current_db = db.latest_scan_number(bl).await.unwrap();
    let gda_num_tracker = &GdaNumTracker::new(&directory);
    let current_file = gda_num_tracker
        .latest_scan_number(&extension)
        .await
        .unwrap();

    if current_db == current_file {
        println!("{bl} scan numbers are in sync: {current_db}");
        return;
    }
    println!("{bl} scan numbers do not match");
    println!("    DB  : {current_db}");
    println!("    File: {current_file} ({directory}/{current_file}.{extension})");

    match mode {
        Some(SyncMode::Import { force }) => {
            if force
                || confirm(format!("Set DB scan number for {bl} to {current_file}?"))
                    .await
                    .unwrap_or(false)
            {
                println!("    Updating DB scan number from {current_db} to {current_file}");
                db.set_scan_number(bl, current_file).await.unwrap();
            }
        }
        Some(SyncMode::Export { force }) => {
            if force
                || confirm(format!("Set scan file for {bl} to {current_db}?"))
                    .await
                    .unwrap_or(false)
            {
                println!("    Updating file scan number from {current_file} to {current_db}");
                gda_num_tracker
                    .set_scan_number(&extension, current_db)
                    .await
                    .unwrap();
            }
        }
        None => {
            let (tx, rx) = oneshot::channel();
            tokio::task::spawn_blocking(move || {
                tx.send(
                    inquire::Select::new(
                        "Sync scan numbers?",
                        vec![
                            SyncDirection::Import(SyncState {
                                db: current_db,
                                file: current_file,
                            }),
                            SyncDirection::Export(SyncState {
                                db: current_db,
                                file: current_file,
                            }),
                            SyncDirection::Skip,
                        ],
                    )
                    .with_formatter(
                        &(|sd: ListOption<&SyncDirection>| {
                            match sd.value {
                                SyncDirection::Import(_) => "Update DB",
                                SyncDirection::Export(_) => "Update File",
                                SyncDirection::Skip => "skip",
                            }
                            .into()
                        }),
                    )
                    .prompt()
                    .unwrap_or(SyncDirection::Skip),
                )
            });
            match rx.await.unwrap_or(SyncDirection::Skip) {
                SyncDirection::Import(_) => db.set_scan_number(bl, current_file).await.unwrap(),
                SyncDirection::Export(_) => gda_num_tracker
                    .set_scan_number(&extension, current_db)
                    .await
                    .unwrap(),
                SyncDirection::Skip => println!("Skipping sync"),
            }
        }
    }
}

#[derive(Debug)]
struct SyncState {
    db: usize,
    file: usize,
}

#[derive(Debug)]
enum SyncDirection {
    Import(SyncState),
    Export(SyncState),
    Skip,
}

impl Display for SyncDirection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SyncDirection::Import(ss) => {
                write!(f, "DB: {} -> {}", ss.db, ss.file)
            }
            SyncDirection::Export(ss) => {
                write!(f, "File: {} -> {}", ss.file, ss.db)
            }
            SyncDirection::Skip => f.write_str("Skip"),
        }
    }
}

fn confirm(prompt: String) -> oneshot::Receiver<bool> {
    let (tx, rx) = oneshot::channel();
    tokio::task::spawn_blocking(move || {
        tx.send(
            inquire::Confirm::new(&prompt)
                .with_default(false)
                .prompt()
                .unwrap_or(false),
        )
    });
    rx
}
