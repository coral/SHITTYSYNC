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

    pub async fn sync_selective(&self, files: &Vec<String>) -> Result<Output, Error> {
        let mut cmd = Command::new("rsync");
        cmd.args([
            "--ignore-existing",
            "-r",
            "-v",
            "--files-from=-",
            &self.source,
            &self.dest,
        ]);

        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());
        cmd.stdin(Stdio::piped());

        let mut child = cmd.spawn()?;

        let mut tn = child.stdin.take().ok_or(Error::CouldNotGetStdin)?;

        let mut filelist: String = "".to_string();
        for f in files {
            filelist.push_str(&format!("{}\n", f));
        }
        tn.write_all(filelist.as_bytes()).await?;

        drop(tn);

        let v = child.wait_with_output().await?;

        Ok(v)
    }

    pub async fn sync_file(&self) -> Result<Output, Error> {
        let mut cmd = Command::new("rsync");
        cmd.args(["--ignore-existing", "-r", "-v", &self.source, &self.dest]);

        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        let child = cmd.spawn()?;
        let v = child.wait_with_output().await?;

        Ok(v)
    }
}
