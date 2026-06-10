use crate::error::Error;
use std::any::Any;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc::{self, Receiver};
use tokio::sync::oneshot;
use tokio::time::timeout;
use zeroconf::prelude::*;
use zeroconf::{BrowserEvent, MdnsBrowser, ServiceType};

const DEFAULT_DISCOVERY_TIMEOUT: Duration = Duration::from_secs(10);

/// A WebDAV share advertised by the Evermusic iOS app, mounted locally for the
/// lifetime of this value. The mount is torn down on drop.
pub struct Evermusic<'a> {
    pub phone: DiscoveredPhone,
    mountpath: &'a str,
}

impl<'a> Evermusic<'a> {
    pub async fn new(
        name: &str,
        mountpath: &'a str,
        timeout: Option<Duration>,
    ) -> Result<Evermusic<'a>, Error> {
        let timeout = timeout.unwrap_or(DEFAULT_DISCOVERY_TIMEOUT);

        let phone = PhoneDiscovery::discover_phone(name, timeout)
            .await
            .map_err(|_| Error::CouldNotFindPhone(name.to_string()))?;

        mkdirp(mountpath)?;

        let output = tokio::process::Command::new("mount_webdav")
            .arg(format!("http://{}:{}/", phone.hostname, phone.port))
            .arg(mountpath)
            .output()
            .await?;

        if !output.status.success() {
            return Err(Error::Mount(
                String::from_utf8_lossy(&output.stderr).trim().to_string(),
            ));
        }

        Ok(Evermusic { phone, mountpath })
    }
}

impl<'a> Drop for Evermusic<'a> {
    fn drop(&mut self) {
        if let Err(e) = std::process::Command::new("umount")
            .arg(self.mountpath)
            .output()
        {
            eprintln!("failed to unmount {}: {}", self.mountpath, e);
        }
    }
}

pub struct DiscoveredPhone {
    pub name: String,
    pub hostname: String,
    pub port: u16,
}

pub struct PhoneDiscovery {
    cancel: Option<std::sync::mpsc::Sender<bool>>,
}

impl PhoneDiscovery {
    pub fn new() -> PhoneDiscovery {
        PhoneDiscovery { cancel: None }
    }

    /// Uses mDNS to scan the network for WebDAV devices, streaming results over
    /// a Tokio channel as they are found. Discovery is cancelled on drop.
    pub async fn discover(&mut self) -> Result<Receiver<DiscoveredPhone>, Error> {
        if self.cancel.is_some() {
            return Err(Error::DiscoveryAlreadyRunning);
        }

        let (tx, rx) = mpsc::channel(200);
        let (cancel_tx, cancel_rx) = std::sync::mpsc::channel::<bool>();
        self.cancel = Some(cancel_tx);

        tokio::task::spawn_blocking(move || {
            let mut browser = MdnsBrowser::new(ServiceType::new("webdav", "tcp").unwrap());

            browser.set_service_callback(Box::new(
                move |event: zeroconf::Result<BrowserEvent>,
                      _context: Option<Arc<dyn Any + Send + Sync>>| {
                    if let Ok(BrowserEvent::Add(res)) = event {
                        let _ = tx.blocking_send(DiscoveredPhone {
                            name: res.name().clone(),
                            hostname: res.host_name().clone(),
                            port: *res.port(),
                        });
                    }
                },
            ));

            let event_loop = browser.browse_services().unwrap();

            loop {
                event_loop.poll(Duration::from_millis(500)).unwrap();

                match cancel_rx.try_recv() {
                    Ok(_) | Err(std::sync::mpsc::TryRecvError::Disconnected) => return,
                    Err(std::sync::mpsc::TryRecvError::Empty) => {}
                }
            }
        });

        Ok(rx)
    }

    /// Scans the network and returns the first device whose advertised name
    /// matches `name`, giving up after `t`.
    pub async fn discover_phone(name: &str, t: Duration) -> Result<DiscoveredPhone, Error> {
        let wanted = name.to_string();
        let mut discovery = PhoneDiscovery::new();
        let mut found = discovery.discover().await?;

        let (tx, rx) = oneshot::channel();

        tokio::spawn(async move {
            while let Some(phone) = found.recv().await {
                info!("Found: {}", phone.name);
                if phone.name == wanted {
                    let _ = tx.send(phone);
                    return;
                }
            }
        });

        timeout(t, rx)
            .await?
            .map_err(|_| Error::CouldNotFindPhone(name.to_string()))
    }
}

impl Drop for PhoneDiscovery {
    fn drop(&mut self) {
        if let Some(cancel) = &self.cancel {
            let _ = cancel.send(true);
        }
    }
}

/// Recursively creates `path` and all missing parent directories, returning the
/// created path (or `None` if it already existed).
pub fn mkdirp<P: AsRef<Path>>(path: P) -> io::Result<Option<PathBuf>> {
    let path = path.as_ref();
    if path == Path::new("") {
        return Ok(None);
    }

    match fs::create_dir(path) {
        Ok(()) => return Ok(Some(path.to_owned())),
        Err(ref e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(ref e) if e.kind() == io::ErrorKind::AlreadyExists => return Ok(None),
        Err(_) if path.is_dir() => return Ok(None),
        Err(e) => return Err(e),
    }

    let created = match path.parent() {
        Some(p) => mkdirp(p),
        None => Err(io::Error::other("failed to create whole tree")),
    };

    match fs::create_dir(path) {
        Ok(()) => created,
        Err(_) if path.is_dir() => created,
        Err(ref e) if e.kind() == io::ErrorKind::AlreadyExists => created,
        Err(e) => Err(e),
    }
}
