use std::env;
use std::error::Error;

use numtracker::db_service::SqliteScanPathService;
use numtracker::{ScanPathService, VisitRequest};
use sqlx::SqlitePool;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let pool = SqlitePool::connect("sqlite:///tmp/demo.db").await.unwrap();
    sqlx::migrate!().run(&pool).await.unwrap();
    let serv = SqliteScanPathService { pool };
    let visit = serv
        .visit_directory(VisitRequest::new("i22".into(), "cm12345-3".into()))
        .await
        .unwrap();

    println!("{visit:?}");
    // let mut args = env::args().skip(1);
    // let bl = args.next().unwrap_or("i22".into());
    // let visit = args.next().unwrap_or("cm12345-3".into());
    // let sub = args.next();
    // let dets = args.collect();

    // let cont = Controller::default();
    // let bl_ctx = VisitRequest::new(bl.clone(), visit.clone());
    // let scan_ctx = ScanRequest::new(bl, visit, sub, dets);
    // println!("i22: {:?}", cont.visit_directory(bl_ctx));
    // println!("scan: {:#?}", cont.scan_spec(scan_ctx));

    Ok(())
}
