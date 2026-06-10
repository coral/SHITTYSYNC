use clap::Parser;
use std::path::PathBuf;

/// Syncs Swinsian playlists to various destinations.
#[derive(Parser, Debug)]
#[command(version, about)]
pub struct Args {
    /// Path to the configuration file.
    #[arg(short, long, default_value = "config.toml")]
    pub config: PathBuf,

    /// Sync playlists to the DJ deck and re-index the BluOS library.
    #[arg(short, long)]
    pub deck: bool,

    /// Sync playlists to the phone over WebDAV (Evermusic).
    #[arg(short, long)]
    pub phone: bool,

    /// Sync playlists to a disk destination.
    #[arg(long)]
    pub disk: bool,

    /// Transcode and sync playlists to an MTP watch.
    #[arg(short, long)]
    pub watch: bool,
}
