use std::error::Error;

use numtracker::gda::GdaNumTracker;

fn main() -> Result<(), Box<dyn Error>> {
    use numtracker::NumTracker;
    let mut num = GdaNumTracker::default();
    println!("{}", num.increment_and_get()?);
    Ok(())
}
