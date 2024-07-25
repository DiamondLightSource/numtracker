use std::env;
use std::error::Error;

use numtracker::numtracker::{GdaNumTracker, NumTracker as _};
use numtracker::paths::{PathConstructor as _, TemplatePathConstructor};
use numtracker::BeamlineContext;

fn main() -> Result<(), Box<dyn Error>> {
    let dir = "/tmp/";
    let bl = env::args().nth(1).unwrap_or("i22".into());

    let ctx = BeamlineContext::new(bl);

    let mut num = GdaNumTracker::new(dir);
    println!("{}", num.increment_and_get(&ctx)?);

    let pc = TemplatePathConstructor::new("/tmp/{instrument}/data/{year}/").unwrap();

    // println!("{pc:?}");
    let dir = pc.visit_directory(&ctx);
    println!("{dir:?}");

    Ok(())
}
