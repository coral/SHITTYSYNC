use crate::error::Error;
use std::process::{Output, Stdio};
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

pub struct Rsync {
    source: String,
    dest: String,
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
    pub async fn sync_selective(
        &self,
        files: &[String],
        delete_existing: bool,
    ) -> Result<Output, Error> {
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

        cmd.stdout(Stdio::inherit());
        cmd.stderr(Stdio::inherit());
        cmd.stdin(Stdio::piped());

        let mut child = cmd.spawn()?;

        let mut stdin = child.stdin.take().ok_or(Error::CouldNotGetStdin)?;
        let filelist = files.join("\n");
        stdin.write_all(filelist.as_bytes()).await?;
        drop(stdin);

        Ok(child.wait_with_output().await?)
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
