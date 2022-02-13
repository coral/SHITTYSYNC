use crate::error::Error;
use filenamify::filenamify;
use tokio::fs::File;
use tokio::io::AsyncWriteExt;

pub async fn create_m3u(name: &str, files: &Vec<String>) -> Result<String, Error> {
    let p = &format!("/tmp/{}.m3u", filenamify(name));
    let mut file = File::create(&p).await?;

    file.write_all(files.join("\n").as_bytes()).await?;

    Ok(p.clone())
}
