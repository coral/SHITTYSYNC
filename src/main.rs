mod cli;
mod commands;
mod config;
mod error;
mod evermusic;
mod m3u;
mod rsync;
mod transcode;
mod watch;

#[macro_use]
extern crate log;

use anyhow::{Context, Result};
use clap::Parser;
use cli::Args;
use config::Config;
use std::path::Path;
use swinsiandb::Database;

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    init_logging();
    info!("SHITTYSYNC v{}", env!("CARGO_PKG_VERSION"));

    let cfg = Config::load_config(&args.config)
        .with_context(|| format!("loading config from {}", args.config.display()))?;
    let db = Database::from_file(Path::new(&cfg.swinsian.dbpath))
        .context("opening Swinsian database")?;

    if args.deck {
        commands::sync_deck(&db, &cfg).await?;
    }
    if args.disk {
        commands::sync_disk(&db, &cfg).await?;
    }
    if args.phone {
        commands::sync_phone(&db, &cfg).await?;
    }
    if args.watch {
        commands::sync_watch(&db, &cfg).await?;
    }

    info!("------------- DONE -------------");
    Ok(())
}

fn init_logging() {
    if std::env::var_os("RUST_LOG").is_none() {
        std::env::set_var("RUST_LOG", "info");
    }
    pretty_env_logger::init();
}
