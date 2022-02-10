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

    #[error("Stdin Errr")]
    CouldNotGetStdin,

    #[error(transparent)]
    PhoneTimeout(#[from] tokio::time::error::Elapsed),

    #[error("could not find phone")]
    CouldNotFindPhone,

    #[error("other")]
    Other,
}
