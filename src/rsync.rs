use crate::error::Error;
use std::io::Write;
use std::process::{Output, Stdio};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;

pub struct Rsync {
    source: String,
    dest: String,
}

/// Tally of what an rsync run did, derived from its verbose output.
#[derive(Debug, Default, Clone, Copy)]
pub struct SyncStats {
    pub copied: usize,
    pub skipped: usize,
}

impl Rsync {
    pub fn new(source: &str, dest: &str) -> Rsync {
        Rsync {
            source: source.to_string(),
            dest: dest.to_string(),
        }
    }

    /// Syncs an explicit list of files from `source` to `dest`, feeding the
    /// file list to rsync over stdin. When `delete_existing` is set, files on
    /// the destination that aren't in the list are removed.
    ///
    /// rsync's per-file chatter is consumed rather than printed; instead a
    /// single status line is rendered in place with a running copied/skipped
    /// counter.
    pub async fn sync_selective(
        &self,
        files: &[String],
        delete_existing: bool,
    ) -> Result<SyncStats, Error> {
        let mut cmd = Command::new("rsync");

        if delete_existing {
            cmd.args([
                "--ignore-existing",
                "-r",
                "-v",
                "-p",
                "--include-from=-",
                "--exclude=*",
                "--delete-excluded",
                &self.source,
                &self.dest,
            ]);
        } else {
            cmd.args([
                "--ignore-existing",
                "-r",
                "-v",
                "--files-from=-",
                &self.source,
                &self.dest,
            ]);
        }

        // Capture stdout to count/summarise; leave stderr inherited so genuine
        // errors still surface.
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::inherit());
        cmd.stdin(Stdio::piped());

        let mut child = cmd.spawn()?;
        let mut stdin = child.stdin.take().ok_or(Error::CouldNotGetStdin)?;
        let stdout = child.stdout.take().ok_or(Error::CouldNotGetStdin)?;

        // Feed the file list on a separate task: rsync streams output while we
        // write, so doing both inline could deadlock on full pipe buffers.
        let filelist = files.join("\n");
        let writer = tokio::spawn(async move {
            let _ = stdin.write_all(filelist.as_bytes()).await;
            // `stdin` drops here, signalling EOF to rsync.
        });

        let mut stats = SyncStats::default();
        let mut reader = BufReader::new(stdout);
        let mut buf = Vec::new();
        let mut stderr = std::io::stderr();
        // Read raw bytes rather than `lines()`: openrsync echoes filesystem
        // paths verbatim, which on a foreign-normalised or otherwise non-UTF-8
        // volume aren't valid UTF-8. `lines()` would error out on the first
        // such path and abort the whole sync; decoding lossily lets one odd
        // filename through as a harmless mangled line instead.
        loop {
            buf.clear();
            if reader.read_until(b'\n', &mut buf).await? == 0 {
                break;
            }
            let line = String::from_utf8_lossy(&buf);
            match classify(&line) {
                Some(LineKind::Copied) => stats.copied += 1,
                Some(LineKind::Skipped) => stats.skipped += 1,
                None => continue,
            }
            let _ = write!(stderr, "\r  …{} copied, {} skipped   ", stats.copied, stats.skipped);
            let _ = stderr.flush();
        }

        let _ = writer.await;
        let status = child.wait().await?;

        // Replace the in-place line with a final summary.
        let _ = writeln!(stderr, "\r  {} copied, {} skipped        ", stats.copied, stats.skipped);

        if !status.success() {
            warn!("rsync exited with {}", status);
        }

        Ok(stats)
    }

    /// Recursively syncs `source` to `dest`.
    pub async fn sync_file(&self) -> Result<Output, Error> {
        let mut cmd = Command::new("rsync");
        cmd.args(["-r", "-v", &self.source, &self.dest]);
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        let child = cmd.spawn()?;
        Ok(child.wait_with_output().await?)
    }
}

enum LineKind {
    Copied,
    Skipped,
}

/// Classifies a line of openrsync verbose output, ignoring headers, blank
/// lines, transfer stats, and deletions.
fn classify(line: &str) -> Option<LineKind> {
    let line = line.trim_end();
    if line.is_empty() {
        return None;
    }
    if line.starts_with("Skip existing ") {
        return Some(LineKind::Skipped);
    }
    if line.starts_with("Transfer starting:")
        || line.starts_with("sent ")
        || line.starts_with("total size is ")
        || line.starts_with("deleting ")
        || line.ends_with('/')
    {
        return None;
    }
    Some(LineKind::Copied)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Counts a block of real openrsync `-v` output (as captured from the CLI).
    fn tally(output: &str) -> SyncStats {
        let mut stats = SyncStats::default();
        for line in output.lines() {
            match classify(line) {
                Some(LineKind::Copied) => stats.copied += 1,
                Some(LineKind::Skipped) => stats.skipped += 1,
                None => {}
            }
        }
        stats
    }

    #[test]
    fn counts_copied_and_skipped() {
        let output = "Transfer starting: 6 files\n\
                      Skip existing 'A/one.flac'\n\
                      A/two.flac\n\
                      A/three.flac\n\
                      \n\
                      sent 4289 bytes  received 42 bytes  43310000 bytes/sec\n\
                      total size is 8192  speedup is 1.89\n";
        let stats = tally(output);
        assert_eq!(stats.copied, 2);
        assert_eq!(stats.skipped, 1);
    }

    #[test]
    fn ignores_directories_and_deletions() {
        assert!(classify("AlbumA/").is_none());
        assert!(classify("deleting old.flac").is_none());
        assert!(classify("").is_none());
        assert!(classify("   ").is_none());
    }

    #[test]
    fn paths_with_unusual_names_count_as_copied() {
        // Filenames can contain spaces, quotes and apostrophes.
        assert!(matches!(
            classify("The Orb - Morphology/06 - Sentinel (7'' mix).flac"),
            Some(LineKind::Copied)
        ));
    }
}
