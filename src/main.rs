use std::error::Error;

use numtracker::db_service::SqliteScanPathService;
use numtracker::{ScanPathService, ScanRequest, VisitRequest};
use sqlx::SqlitePool;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let pool = SqlitePool::connect("sqlite://./demo.db").await.unwrap();
    sqlx::migrate!().run(&pool).await.unwrap();
    let serv = SqliteScanPathService { pool };
    let visit = serv
        .visit_directory(VisitRequest::new("i22", "cm12345-3"))
        .await
        .unwrap();

    let scan_1 = serv
        .scan_spec(ScanRequest::new(
            "i22",
            "cm12345-3",
            Option::<String>::None,
            vec!["pilatus_SAXS", "I0"],
        ))
        .await
        .unwrap();

    println!("Scan 1: {scan_1:#?}");
    let scan_2 = serv
        .scan_spec(ScanRequest::new(
            "i22",
            "cm12345-3",
            Option::<String>::None,
            vec!["pilatus_SAXS", "I0"],
        ))
        .await
        .unwrap();

    println!("Scan 2: {scan_2:#?}");

    println!("{visit:?}");

    Ok(())
}
