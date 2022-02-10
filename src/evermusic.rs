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
use zeroconf::{MdnsBrowser, ServiceDiscovery, ServiceType};

pub struct Evermusic<'a> {
    pub phone: DiscoveredPhone,
    mountpath: &'a str,
}

impl<'a> Evermusic<'a> {
    pub async fn new(name: &str, mountpath: &'a str) -> Result<Evermusic<'a>, Error> {
        let phone = match PhoneDiscovery::discover_phone(
            "evermusic.webdav",
            std::time::Duration::from_secs(5),
        )
        .await
        {
            Ok(v) => v,
            Err(_) => {
                return Err(Error::CouldNotFindPhone);
            }
        };

        mkdirp(mountpath)?;

        tokio::process::Command::new("mount_webdav")
            .arg(format!("http://{}:{}/", phone.hostname, phone.port))
            .arg(mountpath)
            .output()
            .await?;

        let e = Evermusic { phone, mountpath };

        Ok(e)
    }
}

impl<'a> Drop for Evermusic<'a> {
    fn drop(&mut self) {
        std::process::Command::new("umount")
            .arg(self.mountpath)
            .output()
            .expect("failed to execute process");
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
    /// Discover uses mDNS to scan the network for BluOS devices
    /// Returns a Tokio channel that streams results as they are found
    ///
    /// The discovery process is cancelled on drop
    pub async fn discover(&mut self) -> Result<Receiver<DiscoveredPhone>, Error> {
        //Check if we're already doing this
        if self.cancel.is_some() {
            return Err(Error::Other);
        }

        let (tx, rx) = mpsc::channel(200);
        let (ctx, crx): (
            std::sync::mpsc::Sender<bool>,
            std::sync::mpsc::Receiver<bool>,
        ) = std::sync::mpsc::channel();
        self.cancel = Some(ctx);

        tokio::task::spawn_blocking(move || {
            let mut browser = MdnsBrowser::new(ServiceType::new("http", "tcp").unwrap());

            browser.set_service_discovered_callback(Box::new(
                move |result: zeroconf::Result<ServiceDiscovery>,
                      _context: Option<Arc<dyn Any>>| {
                    let res = result.unwrap();
                    let _ = tx.blocking_send(DiscoveredPhone {
                        name: res.name().clone(),
                        hostname: res.address().clone(),
                        port: *res.port(),
                    });
                },
            ));

            let event_loop = browser.browse_services().unwrap();

            loop {
                event_loop.poll(Duration::from_millis(500)).unwrap();

                match crx.try_recv() {
                    Ok(_) => return,
                    Err(e) => match e {
                        std::sync::mpsc::TryRecvError::Empty => {}
                        std::sync::mpsc::TryRecvError::Disconnected => return,
                    },
                }
            }
        });

        Ok(rx)
    }
    /// Discover one is a helper function that scans the network and returns the FIRST BluOS device it finds.
    /// This is useful if you only have one BluOS device.
    pub async fn discover_phone(name: &str, t: Duration) -> Result<DiscoveredPhone, Error> {
        let hname = name.to_string();
        let mut d = PhoneDiscovery::new();
        let mut c = d.discover().await?;

        let (tx, rx) = oneshot::channel();

        tokio::spawn(async move {
            while let Some(i) = c.recv().await {
                if i.name == hname {
                    tx.send(i);
                    return;
                }
            }
        });
        // Wrap the future with a `Timeout` set to expire in 10 milliseconds.
        match timeout(t, rx).await? {
            Ok(v) => return Ok(v),
            Err(_) => return Err(Error::Other),
        }
    }
}

impl Drop for PhoneDiscovery {
    fn drop(&mut self) {
        match &self.cancel {
            Some(c) => {
                let _ = c.send(true);
            }
            None => {}
        }
    }
}

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
        None => {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                "failed to create whole tree",
            ))
        }
    };
    match fs::create_dir(path) {
        Ok(()) => created,
        Err(_) if path.is_dir() => created,
        Err(ref e) if e.kind() == io::ErrorKind::AlreadyExists => created,
        Err(e) => Err(e),
    }
}
