use std::error::Error;

use cli::{Cli, Command};
use tracing::debug;

mod cli;
mod configuration;
mod context;
mod db_service;
mod graphql;
mod info;
mod logging;
mod numtracker;
mod paths;
mod sync;
mod template;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let args = Cli::init();
    let _ = logging::init_logging(args.log_level(), args.tracing());
    debug!(args = format_args!("{:#?}", args));
    match args.command {
        Command::Serve(opts) => graphql::serve_graphql(&args.db, opts).await,
        Command::Schema => graphql::graphql_schema(),
        Command::Info(info) => info::list_info(&args.db, info.beamline()).await,
        Command::Sync(opts) => sync::sync_directories(&args.db, opts).await?,
        Command::Config(opts) => configuration::configure(&args.db, opts.action).await?,
    }
    Ok(())
}
