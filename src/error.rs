use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error(transparent)]
    ParseError(#[from] serde_json::Error),

    #[error(transparent)]
    ConfigError(#[from] toml::de::Error),

    #[error(transparent)]
    ReadError(#[from] std::io::Error),

    #[error(transparent)]
    RequestError(#[from] reqwest::Error),

    #[error(transparent)]
    MTPError(#[from] libmtp_rs::error::Error),

    #[error("Stdin Errr")]
    CouldNotGetStdin,

    #[error(transparent)]
    PhoneTimeout(#[from] tokio::time::error::Elapsed),

    #[error("could not find phone")]
    CouldNotFindPhone,

    #[error("could not find Watch")]
    CouldNotFindWatch,

    #[error("transcoder: could not generate output filename for `{0}`")]
    TranscodeCouldNotGenerateOutputFilename(String),

    #[error("ffmpeg error")]
    FFMpegError,

    #[error("other")]
    Other,
}
