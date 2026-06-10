//! The individual sync flows, one per destination, extracted out of `main`.

use crate::config::Config;
use crate::evermusic::Evermusic;
use crate::rsync::Rsync;
use crate::transcode::Transcoder;
use crate::{m3u, watch};
use anyhow::{Context, Result};
use bluos_api_rs::{BluOS, Discovery};
use glob::{MatchOptions, Pattern};
use rayon::prelude::*;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use swinsiandb::{Database, Playlist};

/// Resolves a target's playlist selection into concrete playlists.
///
/// `names` are matched exactly (preserving the historical "first match wins"
/// behaviour for duplicate names). `patterns` are globs matched against each
/// playlist's folder path — e.g. `"Weatherall/*"` selects everything directly
/// inside the "Weatherall" folder, `"Weatherall/**"` recurses. Results are
/// de-duplicated by playlist id.
fn resolve_playlists(db: &Database, names: &[String], patterns: &[String]) -> Result<Vec<Playlist>> {
    let mut seen = HashSet::new();
    let mut resolved = Vec::new();

    for name in names {
        let playlist = db
            .get_playlist(name)
            .with_context(|| format!("looking up playlist '{}'", name))?;
        if seen.insert(playlist.playlist_id) {
            resolved.push(playlist);
        }
    }

    if !patterns.is_empty() {
        let matchers = patterns
            .iter()
            .map(|p| Pattern::new(p).with_context(|| format!("invalid playlist pattern '{}'", p)))
            .collect::<Result<Vec<_>>>()?;
        // `*` stays within one folder level; `**` crosses folder boundaries.
        let opts = MatchOptions {
            require_literal_separator: true,
            ..MatchOptions::new()
        };

        let mut pattern_hit = vec![false; matchers.len()];
        for entry in db.get_playlists_with_paths()? {
            let mut matched = false;
            for (i, matcher) in matchers.iter().enumerate() {
                if matcher.matches_with(&entry.path, opts) {
                    pattern_hit[i] = true;
                    matched = true;
                }
            }
            if matched && seen.insert(entry.playlist.playlist_id) {
                resolved.push(entry.playlist);
            }
        }

        for (i, hit) in pattern_hit.iter().enumerate() {
            if !hit {
                warn!("pattern '{}' matched no playlists", patterns[i]);
            }
        }
    }

    Ok(resolved)
}

/// Returns the paths of every song in `playlist`, with the configured basepath
/// prefix rewritten to `prefix`.
fn playlist_files(
    db: &Database,
    playlist: &Playlist,
    basepath: &str,
    prefix: &str,
) -> Result<Vec<String>> {
    Ok(db
        .get_playlist_songs(playlist)
        .with_context(|| format!("reading songs for playlist '{}'", playlist.name))?
        .into_iter()
        .map(|t| t.path.replace(basepath, prefix))
        .collect())
}

/// Collects the de-duplicated set of song paths across several `playlists`.
fn unique_playlist_files(
    db: &Database,
    playlists: &[Playlist],
    basepath: &str,
    prefix: &str,
) -> Result<Vec<String>> {
    let mut files = HashSet::new();
    for playlist in playlists {
        info!("Collecting: {}", playlist.name);
        files.extend(playlist_files(db, playlist, basepath, prefix)?);
    }
    Ok(files.into_iter().collect())
}

/// Syncs the configured playlists to the deck, then asks the BluOS amp to
/// re-index its library.
pub async fn sync_deck(db: &Database, cfg: &Config) -> Result<()> {
    let playlists = resolve_playlists(db, &cfg.decksync.playlists, &cfg.decksync.patterns)?;
    for playlist in &playlists {
        info!("Syncing: {}", playlist.name);
        let files = playlist_files(db, playlist, &cfg.basepath, "")?;

        Rsync::new(&cfg.basepath, &cfg.decksync.destination)
            .sync_selective(&files, false)
            .await?;

        let m3u_path = m3u::create_m3u(&playlist.name, &files).await?;
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
    let playlists = resolve_playlists(db, &cfg.disksync.playlists, &cfg.disksync.patterns)?;
    for playlist in &playlists {
        info!("Syncing: {}", playlist.name);
        let files = playlist_files(db, playlist, &cfg.basepath, "../")?;

        Rsync::new(&cfg.basepath, &cfg.disksync.destination)
            .sync_selective(&files, false)
            .await?;

        let m3u_path = m3u::create_m3u(&playlist.name, &files).await?;
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

    let playlists = resolve_playlists(db, &cfg.evermusic.playlists, &cfg.evermusic.patterns)?;
    let files = unique_playlist_files(db, &playlists, &cfg.basepath, "")?;

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
    let playlists = resolve_playlists(db, &cfg.watch.playlists, &cfg.watch.patterns)?;
    let mut to_sync: HashSet<String> = HashSet::new();
    for playlist in &playlists {
        info!("Preparing '{}' for syncing", playlist.name);
        to_sync.extend(db.get_playlist_songs(playlist)?.into_iter().map(|t| t.path));
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
