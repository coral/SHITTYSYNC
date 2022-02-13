mod config;
mod error;
mod evermusic;
mod m3u;
mod rsync;

use anyhow::Result;
use bluos_api_rs::{BluOS, Discovery};
use clap::Parser;
use config::Config;
use evermusic::Evermusic;
use std::path::Path;
use swinsiandb::Database;

use pretty_env_logger;
#[macro_use]
extern crate log;

#[derive(Parser, Debug)]
struct Args {
    #[clap(short, long, default_value = "Config.toml")]
    config: String,

    #[clap(short, long)]
    deck: bool,
    #[clap(short, long)]
    phone: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    std::env::set_var("RUST_LOG", "info");
    pretty_env_logger::init();
    info!("SHITTYSYNC v0.0000000000000000000001");

    let args = Args::parse();
    let config = Path::new(&args.config);

    let cfg = match Config::load_config(config) {
        Ok(cfg) => cfg,
        Err(e) => panic!("Could not find config: {}", e),
    };

    //Load it up
    let db = Database::from_file(Path::new(&cfg.swinsian.dbpath))?;

    ///////////////////////
    // Deck
    ///////////////////////
    if args.deck {
        for playlist in &cfg.decksync.playlists {
            // files
            info!("Syncing: {}", playlist);
            let d = db.get_playlist(&playlist)?;
            let files: Vec<String> = db
                .get_playlist_songs(&d)?
                .into_iter()
                .map(|t| t.path.replace(&cfg.basepath, ""))
                .collect();

            let rs = rsync::Rsync::new(&cfg.basepath, &cfg.decksync.destination);
            rs.sync_selective(&files).await?;

            // playlist
            let new_pl = m3u::create_m3u(playlist, &files).await?;
            let r = rsync::Rsync::new(&new_pl, &cfg.decksync.destination)
                .sync_file()
                .await?;
        }

        // lets re-index the amp
        info!("Re-indexing the BluOS Library");
        let device = Discovery::discover_one().await?;
        info!("Found BluOS device on: {}", &device.hostname);
        BluOS::new_from_discovered(device)?.update_library().await?;
    }

    ///////////////////////
    // Phone
    ///////////////////////

    if args.phone {
        info!("Discovering evermusic");
        let e = Evermusic::new(&cfg.evermusic.servicename, &cfg.evermusic.mountpath).await?;
        info!(
            "Found Evermusic Webdav at {}:{}",
            &e.phone.hostname, &e.phone.port
        );

        for playlist in &cfg.evermusic.playlists {
            info!("Syncing: {}", playlist);
            let d = db.get_playlist(&playlist)?;
            let files: Vec<String> = db
                .get_playlist_songs(&d)?
                .into_iter()
                .map(|t| t.path.replace(&cfg.basepath, ""))
                .collect();

            let rs = rsync::Rsync::new(&cfg.basepath, &format!("{}/", cfg.evermusic.mountpath));
            rs.sync_selective(&files).await?;
        }
    }

    Ok(())
}
