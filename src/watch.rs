use crate::config::Watch as WatchConfig;
use crate::error::Error;
use libmtp_rs::device::raw::detect_raw_devices;
use libmtp_rs::device::MtpDevice;

pub struct Watch {
    cfg: WatchConfig,
    device: MtpDevice,
}

impl Watch {
    pub async fn new(cfg: WatchConfig) -> Result<Watch, Error> {
        let raw_devices = detect_raw_devices()?;
        let mtp_devices = raw_devices.into_iter().map(|raw| raw.open_uncached());

        let device = mtp_devices
            .into_iter()
            .find(|d| match d {
                Some(v) => match v.get_friendly_name() {
                    Ok(v) => v == cfg.device_name,
                    Err(_) => false,
                },
                None => false,
            })
            .ok_or(Error::CouldNotFindWatch)?
            .ok_or(Error::CouldNotFindWatch)?;

        Ok(Watch { cfg, device })
    }
}
