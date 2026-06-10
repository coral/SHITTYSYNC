use crate::error::Error;
use std::path::{Path, PathBuf};
use std::process::Command;

pub struct Transcoder {
    cache_folder: PathBuf,
}

impl Transcoder {
    pub fn new(cache_folder: impl Into<PathBuf>) -> Self {
        Transcoder {
            cache_folder: cache_folder.into(),
        }
    }

    /// Transcodes `file` to AAC in an `.mp4` container inside the cache folder,
    /// returning the path of the transcoded file. Already-cached files are
    /// returned without re-encoding.
    pub fn transcode(&self, file: &Path) -> Result<PathBuf, Error> {
        let stem = file.file_stem().ok_or_else(|| {
            Error::TranscodeCouldNotGenerateOutputFilename(file.to_string_lossy().into_owned())
        })?;

        let output = self.cache_folder.join(stem).with_extension("mp4");
        if output.exists() {
            return Ok(output);
        }

        let status = Command::new("ffmpeg")
            .args(["-i", &file.to_string_lossy()])
            .args([
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
            ])
            .arg(&output)
            .status()?;

        if status.success() {
            Ok(output)
        } else {
            Err(Error::FFmpeg(file.to_string_lossy().into_owned()))
        }
    }
}
