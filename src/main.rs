mod config;
mod error;
mod evermusic;
mod m3u;
mod rsync;
mod transcode;
mod watch;

use anyhow::Result;
use bluos_api_rs::{BluOS, Discovery};
use clap::Parser;
use config::Config;
use evermusic::Evermusic;
use rayon::prelude::{IntoParallelIterator, ParallelIterator};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
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
    #[clap(long)]
    disk: bool,
    #[clap(long, short)]
    watch: bool,
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
            rs.sync_selective(&files, false).await?;

            // playlist
            let new_pl = m3u::create_m3u(playlist, &files).await?;
            rsync::Rsync::new(&new_pl, &cfg.decksync.destination)
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
    // Disk
    ///////////////////////
    if args.disk {
        for playlist in &cfg.disksync.playlists {
            // files
            info!("Syncing: {}", playlist);
            let d = db.get_playlist(&playlist)?;
            let files: Vec<String> = db
                .get_playlist_songs(&d)?
                .into_iter()
                .map(|t| t.path.replace(&cfg.basepath, "../"))
                .collect();

            let rs = rsync::Rsync::new(&cfg.basepath, &cfg.disksync.destination);
            rs.sync_selective(&files, false).await?;

            // playlist
            let new_pl = m3u::create_m3u(playlist, &files).await?;
            rsync::Rsync::new(&new_pl, &cfg.disksync.playlistfolder)
                .sync_file()
                .await?;
        }
    }

    ///////////////////////
    // Phone
    ///////////////////////

    if args.phone {
        info!("Discovering evermusic");
        let e = Evermusic::new(&cfg.evermusic.servicename, &cfg.evermusic.mountpath, None).await?;
        info!(
            "Found Evermusic Webdav at {}:{}",
            &e.phone.hostname, &e.phone.port
        );

        let mut tosync: HashSet<String> = HashSet::new();

        for playlist in &cfg.evermusic.playlists {
            info!("Syncing: {}", playlist);
            let d = db.get_playlist(&playlist)?;
            let files: Vec<String> = db
                .get_playlist_songs(&d)?
                .into_iter()
                .map(|t| t.path.replace(&cfg.basepath, ""))
                .collect();

            for f in files {
                tosync.insert(f);
            }
        }

        let rs = rsync::Rsync::new(&cfg.basepath, &format!("{}/", cfg.evermusic.mountpath));
        rs.sync_selective(&tosync.into_iter().collect(), false)
            .await?;
    }

    if args.watch {
        info!("WATCH TIME: finding watch");
        let mut w = watch::Watch::new(cfg.watch.clone()).await?;
        let trs = transcode::Transcoder::new("/tmp".to_string());

        // Generate list of files to be synced
        let mut tosync: HashSet<String> = HashSet::new();

        let bp = Path::new(&cfg.basepath);

        for playlist in &cfg.watch.playlists {
            info!("Preparing '{}' for syncing", playlist);
            let d = db.get_playlist(&playlist)?;
            let files: Vec<String> = db
                .get_playlist_songs(&d)?
                .into_iter()
                .map(|t| t.path)
                .collect();

            for f in files {
                tosync.insert(f);
            }
        }

        // Diff against files on device
        let mut to_transcode: HashSet<PathBuf> = HashSet::new();
        for f in tosync.iter() {
            let p = Path::new(f);
            let mut d = pathdiff::diff_paths(p, bp).unwrap();
            d.set_extension("mp4");
            if !w.exists(d.as_path()) {
                to_transcode.insert(p.to_path_buf());
            }
        }

        info!("Transcoding {} files", to_transcode.len());

        let transcoded_files: Vec<watch::TransferObject> = to_transcode
            .into_par_iter()
            .map(|d| {
                let src = d.clone();

                let transcoded_file = trs
                    .transcode(d.into_os_string().into_string().unwrap())
                    .unwrap();

                let mut dst = pathdiff::diff_paths(src.as_path(), bp).unwrap();
                dst.set_extension("mp4");

                watch::TransferObject {
                    source: src.clone(),
                    transcoded: Path::new("/tmp").join(transcoded_file).to_path_buf(),
                    destination: dst,
                }
            })
            .collect();

        info!("uploading {:?}", &transcoded_files);

        for f in transcoded_files {
            info!("Syncing file: {:?}", f);
            println!("");
            w.put_file(f)?;
        }
    }

    println!("------------- DONE -------------");

    Ok(())
}
