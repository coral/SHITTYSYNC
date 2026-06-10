use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error(transparent)]
    Config(#[from] toml::de::Error),

    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Mtp(#[from] libmtp_rs::error::Error),

    #[error(transparent)]
    PhoneTimeout(#[from] tokio::time::error::Elapsed),

    #[error("could not read child process stdin")]
    CouldNotGetStdin,

    #[error("could not find phone `{0}` on the network")]
    CouldNotFindPhone(String),

    #[error("could not find watch `{0}`")]
    CouldNotFindWatch(String),

    #[error("failed to mount WebDAV share: {0}")]
    Mount(String),

    #[error("transcoder: could not generate output filename for `{0}`")]
    TranscodeCouldNotGenerateOutputFilename(String),

    #[error("ffmpeg failed to transcode `{0}`")]
    FFmpeg(String),

    #[error("watch has no usable storage")]
    NoWatchStorage,

    #[error("could not find folder: `{0}`")]
    CouldNotFindFolder(String),

    #[error("mDNS discovery is already running")]
    DiscoveryAlreadyRunning,
}
