use std::env;
use std::error::Error;

use numtracker::paths::{PathConstructor as _, TemplatePathConstructor};
use numtracker::{BeamlineContext, Subdirectory};

fn main() -> Result<(), Box<dyn Error>> {
    let bl = env::args().nth(1).unwrap_or("i22".into());

    let ctx = BeamlineContext::new(bl, "cm12345-3".parse().unwrap());

    let pc = TemplatePathConstructor::new("/tmp/{instrument}/data/{year}/{visit}").unwrap();

    let dir = pc.visit_directory(&ctx);
    println!("Visit: {dir:?}");

    let scan = ctx
        .next_scan()
        .with_subdirectory(Subdirectory::new("demo/subdir").unwrap());

    println!("Scan File: {:?}", pc.scan_file(&scan));

    Ok(())
}
