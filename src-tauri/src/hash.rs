use sha2::{Digest, Sha256};

pub async fn sha256_file(path: &std::path::Path) -> anyhow::Result<String> {
    let file = tokio::fs::File::open(path).await?;
    let mut reader = tokio::io::BufReader::new(file);
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 8192];
    loop {
        let read = tokio::io::AsyncReadExt::read(&mut reader, &mut buffer).await?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}
