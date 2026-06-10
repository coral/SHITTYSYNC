//! The individual sync flows, one per destination, extracted out of `main`.

use crate::config::Config;
use crate::evermusic::Evermusic;
use crate::rsync::Rsync;
use crate::transcode::Transcoder;
use crate::{m3u, watch};
use anyhow::{Context, Result};
use bluos_api_rs::{BluOS, Discovery};
use rayon::prelude::*;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use swinsiandb::Database;

/// Returns the paths of every song in `playlist`, with the configured basepath
/// prefix rewritten to `prefix`.
fn playlist_files(
    db: &Database,
    playlist: &str,
    basepath: &str,
    prefix: &str,
) -> Result<Vec<String>> {
    let pl = db
        .get_playlist(playlist)
        .with_context(|| format!("looking up playlist '{}'", playlist))?;
    Ok(db
        .get_playlist_songs(&pl)
        .with_context(|| format!("reading songs for playlist '{}'", playlist))?
        .into_iter()
        .map(|t| t.path.replace(basepath, prefix))
        .collect())
}

/// Collects the de-duplicated set of song paths across several `playlists`.
fn unique_playlist_files(
    db: &Database,
    playlists: &[String],
    basepath: &str,
    prefix: &str,
) -> Result<Vec<String>> {
    let mut files = HashSet::new();
    for playlist in playlists {
        info!("Collecting: {}", playlist);
        files.extend(playlist_files(db, playlist, basepath, prefix)?);
    }
    Ok(files.into_iter().collect())
}

/// Syncs the configured playlists to the deck, then asks the BluOS amp to
/// re-index its library.
pub async fn sync_deck(db: &Database, cfg: &Config) -> Result<()> {
    for playlist in &cfg.decksync.playlists {
        info!("Syncing: {}", playlist);
        let files = playlist_files(db, playlist, &cfg.basepath, "")?;

        Rsync::new(&cfg.basepath, &cfg.decksync.destination)
            .sync_selective(&files, false)
            .await?;

        let m3u_path = m3u::create_m3u(playlist, &files).await?;
        Rsync::new(&m3u_path, &cfg.decksync.destination)
            .sync_file()
            .await?;
    }

    info!("Re-indexing the BluOS library");
    let device = Discovery::discover_one().await?;
    info!("Found BluOS device on: {}", device.hostname);
    BluOS::new_from_discovered(device)?.update_library().await?;

    Ok(())
}

/// Syncs the configured playlists to a disk destination, writing playlists into
/// a separate playlist folder.
pub async fn sync_disk(db: &Database, cfg: &Config) -> Result<()> {
    for playlist in &cfg.disksync.playlists {
        info!("Syncing: {}", playlist);
        let files = playlist_files(db, playlist, &cfg.basepath, "../")?;

        Rsync::new(&cfg.basepath, &cfg.disksync.destination)
            .sync_selective(&files, false)
            .await?;

        let m3u_path = m3u::create_m3u(playlist, &files).await?;
        Rsync::new(&m3u_path, &cfg.disksync.playlistfolder)
            .sync_file()
            .await?;
    }

    Ok(())
}

/// Discovers the phone over mDNS, mounts its Evermusic WebDAV share and syncs
/// the configured playlists to it.
pub async fn sync_phone(db: &Database, cfg: &Config) -> Result<()> {
    info!("Discovering Evermusic");
    let evermusic = Evermusic::new(&cfg.evermusic.servicename, &cfg.evermusic.mountpath, None).await?;
    info!(
        "Found Evermusic WebDAV at {}:{}",
        evermusic.phone.hostname, evermusic.phone.port
    );

    let files = unique_playlist_files(db, &cfg.evermusic.playlists, &cfg.basepath, "")?;

    Rsync::new(&cfg.basepath, &format!("{}/", cfg.evermusic.mountpath))
        .sync_selective(&files, false)
        .await?;

    Ok(())
}

/// Transcodes the configured playlists to AAC and uploads any files missing
/// from the MTP watch.
pub async fn sync_watch(db: &Database, cfg: &Config) -> Result<()> {
    info!("WATCH TIME: finding watch");
    let mut watch = watch::Watch::new(cfg.watch.clone()).await?;
    let transcoder = Transcoder::new("/tmp");
    let basepath = Path::new(&cfg.basepath);

    // Full source paths of every song we want on the watch.
    let mut to_sync: HashSet<String> = HashSet::new();
    for playlist in &cfg.watch.playlists {
        info!("Preparing '{}' for syncing", playlist);
        let pl = db
            .get_playlist(playlist)
            .with_context(|| format!("looking up playlist '{}'", playlist))?;
        to_sync.extend(db.get_playlist_songs(&pl)?.into_iter().map(|t| t.path));
    }

    // Only keep the files that aren't already on the device.
    let to_transcode: HashSet<PathBuf> = to_sync
        .iter()
        .filter_map(|f| {
            let src = Path::new(f);
            let mut relative = pathdiff::diff_paths(src, basepath)?;
            relative.set_extension("mp4");
            (!watch.exists(&relative)).then(|| src.to_path_buf())
        })
        .collect();

    info!("Transcoding {} files", to_transcode.len());
    let transfers: Vec<watch::TransferObject> = to_transcode
        .into_par_iter()
        .map(|src| transcode_for_watch(&transcoder, basepath, src))
        .collect::<Result<_>>()?;

    info!("Uploading {} files", transfers.len());
    for transfer in transfers {
        info!("Syncing file: {:?}", transfer);
        watch.put_file(transfer)?;
    }

    Ok(())
}

/// Transcodes a single source file and builds the corresponding watch transfer.
fn transcode_for_watch(
    transcoder: &Transcoder,
    basepath: &Path,
    src: PathBuf,
) -> Result<watch::TransferObject> {
    let transcoded = transcoder.transcode(&src)?;

    let mut destination = pathdiff::diff_paths(&src, basepath)
        .with_context(|| format!("computing destination path for {:?}", src))?;
    destination.set_extension("mp4");

    Ok(watch::TransferObject {
        transcoded,
        destination,
    })
}
