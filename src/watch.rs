use crate::config::Watch as WatchConfig;
use crate::error::Error;
use libmtp_rs::device::raw::detect_raw_devices;
use libmtp_rs::device::MtpDevice;
use libmtp_rs::device::StorageSort;
use libmtp_rs::object::filetypes::Filetype;
use libmtp_rs::object::AsObjectId;
use libmtp_rs::storage::{files::FileMetadata, Parent, Storage};
use sha3::{Digest, Sha3_256};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

pub struct Watch {
    cfg: WatchConfig,
    device: MtpDevice,

    /// Hashes of the files already present on the device.
    map: Option<HashSet<String>>,
    music_folder: Option<Parent>,
}

#[derive(Debug, Clone)]
pub struct TransferObject {
    pub transcoded: PathBuf,
    pub destination: PathBuf,
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct WFile {
    name: String,
    id: u32,
    ftype: Filetype,
    children: Option<Vec<WFile>>,
}

impl WFile {
    /// Recursively reads the files and folders under `parent` into a tree.
    pub fn from_storage(storage: &Storage, parent: Parent) -> Vec<WFile> {
        storage
            .files_and_folders(parent)
            .iter()
            .map(|f| {
                let children = matches!(f.ftype(), Filetype::Folder)
                    .then(|| Self::from_storage(storage, Parent::Folder(f.as_id())));
                WFile {
                    name: f.name().to_string(),
                    id: f.as_id(),
                    ftype: f.ftype(),
                    children,
                }
            })
            .collect()
    }

    /// Flattens this node into the list of leaf (file) paths beneath `parent`.
    pub fn resolve_recursive(&self, parent: PathBuf) -> Vec<PathBuf> {
        match &self.children {
            Some(children) => children
                .iter()
                .flat_map(|f| f.resolve_recursive(parent.join(&self.name)))
                .collect(),
            None => vec![parent.join(&self.name)],
        }
    }
}

impl Watch {
    pub async fn new(cfg: WatchConfig) -> Result<Watch, Error> {
        let raw_devices = detect_raw_devices()?;

        let mut device = raw_devices
            .into_iter()
            .filter_map(|raw| raw.open_uncached())
            .find(|d| match d.get_friendly_name() {
                Ok(name) => {
                    println!("Found device {}", name);
                    name == cfg.device_name
                }
                Err(_) => false,
            })
            .ok_or_else(|| Error::CouldNotFindWatch(cfg.device_name.clone()))?;

        device.update_storage(StorageSort::ByFreeSpace)?;

        let mut watch = Watch {
            cfg,
            device,
            map: None,
            music_folder: None,
        };
        watch.build_index()?;

        Ok(watch)
    }

    fn build_index(&mut self) -> Result<(), Error> {
        let storage_pool = self.device.storage_pool();
        let (_, storage) = storage_pool.iter().next().ok_or(Error::NoWatchStorage)?;

        let music_folder_id = Self::find_folder(storage, &self.cfg.base_folder)?;
        let music_folder = Parent::Folder(music_folder_id);
        self.music_folder = Some(music_folder);

        let index = WFile::from_storage(storage, music_folder);

        let mut map = HashSet::new();
        for node in &index {
            for path in node.resolve_recursive(PathBuf::new()) {
                map.insert(strip_extension(&path));
            }
        }
        self.map = Some(map);

        Ok(())
    }

    /// Returns whether a file for `p` (matched by its content hash) already
    /// exists on the device.
    pub fn exists(&self, p: &Path) -> bool {
        match &self.map {
            Some(map) => map.contains(&sha3_hex(&strip_extension(p))),
            None => false,
        }
    }

    pub fn put_file(&mut self, t: TransferObject) -> Result<(), Error> {
        use libmtp_rs::util::CallbackReturn;
        use std::io::Write;

        let storage_pool = self.device.storage_pool();
        let (_, storage) = storage_pool.iter().next().ok_or(Error::NoWatchStorage)?;

        let file = std::fs::File::open(&t.transcoded)?;
        let file_metadata = file.metadata()?;

        let file_name = format!("{}.mp4", sha3_hex(&strip_extension(&t.destination)));
        let metadata = FileMetadata {
            file_size: file_metadata.len(),
            file_name: &file_name,
            file_type: Filetype::Text,
            modification_date: file_metadata.modified()?.into(),
        };

        let folder = self.music_folder.ok_or(Error::NoWatchStorage)?;

        println!("sending {}", metadata.file_name);
        storage.send_file_from_path_with_callback(
            &t.transcoded,
            folder,
            metadata,
            |sent, total| {
                print!("\rProgress {}/{}", sent, total);
                std::io::stdout().lock().flush().expect("Failed to flush");
                CallbackReturn::Continue
            },
        )?;
        println!();

        Ok(())
    }

    fn find_folder(storage: &Storage, key: &str) -> Result<u32, Error> {
        storage
            .files_and_folders(Parent::Root)
            .iter()
            .find(|f| f.name() == key && f.ftype() == Filetype::Folder)
            .map(|n| n.as_id())
            .ok_or_else(|| Error::CouldNotFindFolder(key.to_string()))
    }
}

/// Returns `path` without its extension as a lossy UTF-8 string.
fn strip_extension(path: &Path) -> String {
    path.with_extension("").to_string_lossy().into_owned()
}

/// Hex-encoded SHA3-256 digest of `input`.
fn sha3_hex(input: &str) -> String {
    let mut hasher = Sha3_256::new();
    hasher.update(input);
    hasher
        .finalize()
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect()
}
