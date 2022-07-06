use crate::error::Error;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use std::sync::Arc;

impl Config {
    pub fn load_config(path: &Path) -> Result<Arc<Config>, Error> {
        let data = fs::read_to_string(path)?;
        let cfg: Config = toml::from_str(&data)?;
        Ok(Arc::new(cfg))
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    pub basepath: String,
    pub swinsian: SwinsianConfig,
    pub decksync: DeckSync,
    pub disksync: DiskSync,
    pub evermusic: Evermusic,
    pub watch: Watch,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SwinsianConfig {
    pub dbpath: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeckSync {
    pub destination: String,
    pub playlists: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiskSync {
    pub destination: String,
    pub playlistfolder: String,
    pub playlists: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Evermusic {
    pub servicename: String,
    pub mountpath: String,
    pub playlists: Vec<String>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Watch {
    pub workspace: String,
}
