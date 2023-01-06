use crate::error::Error;
use std::path::Path;
use std::process::Command;

pub struct Transcoder {
    cache_folder: String,
}

impl Transcoder {
    pub fn new(cache_folder: String) -> Self {
        Transcoder { cache_folder }
    }

    pub fn transcode(&self, file: String) -> Result<String, Error> {
        let path = Path::new(&file);

        let mut new_file = path
            .file_stem()
            .ok_or(Error::TranscodeCouldNotGenerateOutputFilename(file.clone()))?
            .to_os_string();

        new_file.push(".mp4");

        let new_file_path = Path::new(&self.cache_folder).join(new_file);

        match new_file_path.exists() {
            true => return Ok(new_file_path.to_string_lossy().to_string()),
            _ => {}
        };

        let child = Command::new("ffmpeg")
            .args([
                "-i",
                &file,
                "-c:a",
                "aac",
                "-b:a",
                "256k",
                "-ar",
                "44100",
                "-map_metadata",
                "0",
                "-map_metadata",
                "0:s:0",
                "-vn",
                &new_file_path.to_str().ok_or(Error::Other)?,
            ])
            .status()?;

        match child.success() {
            true => Ok(new_file_path.to_string_lossy().to_string()),
            false => Err(Error::FFMpegError),
        }
    }
}
