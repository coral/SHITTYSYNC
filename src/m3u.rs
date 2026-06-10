use crate::error::Error;
use filenamify::filenamify;
use tokio::fs::File;
use tokio::io::AsyncWriteExt;

/// Writes a simple `.m3u` playlist containing `files` into `/tmp`, named after
/// `name`, and returns the path it was written to.
pub async fn create_m3u(name: &str, files: &[String]) -> Result<String, Error> {
    let path = format!("/tmp/{}.m3u", filenamify(name));
    let mut file = File::create(&path).await?;
    file.write_all(files.join("\n").as_bytes()).await?;
    Ok(path)
}
