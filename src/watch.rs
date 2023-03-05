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
use std::path::Path;
use std::path::PathBuf;

pub struct Watch {
    cfg: WatchConfig,
    device: MtpDevice,

    index: Option<Vec<WFile>>,
    map: Option<HashSet<String>>,
    music_folder: Option<Parent>,
}

// i am lazy haha
#[derive(Debug, Clone)]
pub struct TransferObject {
    pub source: PathBuf,
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
    pub fn fr(storage: &Storage, parent: Parent) -> Vec<WFile> {
        storage
            .files_and_folders(parent)
            .iter()
            .map(|f| match f.ftype() {
                Filetype::Folder => WFile {
                    name: f.name().to_string(),
                    id: f.as_id(),
                    ftype: f.ftype(),
                    children: Some(Self::fr(storage, Parent::Folder(f.as_id()))),
                },
                _ => WFile {
                    name: f.name().to_string(),
                    id: f.as_id(),
                    ftype: f.ftype(),
                    children: None,
                },
            })
            .collect()
    }

    pub fn resolve_recursive(&self, parent: PathBuf) -> Vec<PathBuf> {
        match &self.children {
            Some(v) => v
                .iter()
                .flat_map(|f| f.resolve_recursive(parent.join(self.name.clone())))
                .collect(),
            None => {
                vec![parent.join(self.name.clone())]
            }
        }
    }
}

impl<'a> Watch {
    pub async fn new(cfg: WatchConfig) -> Result<Watch, Error> {
        let raw_devices = detect_raw_devices()?;
        let mtp_devices = raw_devices.into_iter().map(|raw| raw.open_uncached());

        let mut device = mtp_devices
            .into_iter()
            .find(|d| match d {
                Some(v) => match v.get_friendly_name() {
                    Ok(v) => {
                        println!("Found device {}", v);
                        v == cfg.device_name
                    }
                    Err(_) => false,
                },
                None => false,
            })
            .ok_or(Error::CouldNotFindWatch)?
            .ok_or(Error::CouldNotFindWatch)?;

        device.update_storage(StorageSort::ByFreeSpace)?;

        let mut w = Watch {
            cfg,
            device,
            index: None,
            map: None,
            music_folder: None,
        };
        w.build_index()?;

        Ok(w)
    }

    fn build_index(&mut self) -> Result<(), Error> {
        let storage_pool = self.device.storage_pool();
        let (_, storage) = storage_pool.iter().next().ok_or(Error::NoWatchStorge)?;

        let music_folder_id = Self::find_folder(storage, &self.cfg.base_folder)?;

        self.music_folder = Some(Parent::Folder(music_folder_id));

        let index = WFile::fr(storage, self.music_folder.unwrap());

        let map: HashSet<String> = HashSet::new();
        self.map = Some(map);

        for n in index.iter() {
            let p = PathBuf::new();
            for res in n.resolve_recursive(p.clone()) {
                self.insert_hash(&res.as_path());
            }
        }

        self.index = Some(index);

        Ok(())
    }

    fn insert_hash(&mut self, p: &Path) {
        self.map.as_mut().unwrap().insert(
            p.with_extension("")
                .as_os_str()
                .to_str()
                .unwrap()
                .to_string(),
        );
    }

    pub fn exists(&self, p: &Path) -> bool {
        let mut hash = Sha3_256::new();

        hash.update(p.with_extension("").as_os_str().to_str().unwrap());
        let hash = format!("{:x}", hash.finalize());

        match &self.map {
            Some(m) => m.contains(&hash),
            None => false,
        }
    }

    pub fn put_file(&mut self, t: TransferObject) -> Result<(), Error> {
        use libmtp_rs::util::CallbackReturn;
        use std::io::Write;

        let storage_pool = self.device.storage_pool();
        let (_, storage) = storage_pool.iter().next().ok_or(Error::NoWatchStorge)?;

        let file = std::fs::File::open(t.transcoded.clone())?;
        let metadata = file.metadata()?;

        let mut hash = Sha3_256::new();

        hash.update(
            t.destination
                .with_extension("")
                .as_os_str()
                .to_str()
                .unwrap(),
        );
        let hash = format!("{:x}", hash.finalize());
        let p = format!("{}.mp4", hash);

        let metadata = FileMetadata {
            file_size: metadata.len(),
            file_name: &p,
            file_type: Filetype::Text,
            modification_date: metadata.modified()?.into(),
        };
        let f = self.music_folder.unwrap();

        println!("sending {}", metadata.file_name);
        storage.send_file_from_path_with_callback(t.transcoded, f, metadata, |sent, total| {
            print!("\rProgress {}/{}", sent, total);
            std::io::stdout().lock().flush().expect("Failed to flush");
            CallbackReturn::Continue
        })?;
        println!("");

        Ok(())
    }

    fn find_folder(storage: &Storage, key: &str) -> Result<u32, Error> {
        storage
            .files_and_folders(Parent::Root)
            .iter()
            .find(|f| f.name() == key && f.ftype() == Filetype::Folder)
            .map(|n| n.as_id())
            .ok_or(Error::CouldNotFindFolder(key.to_string()))
    }
}
