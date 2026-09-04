use std::fs::File;
use std::io::{BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};

pub async fn extract(archive: PathBuf, dest: PathBuf) -> Result<()> {
    tokio::task::spawn_blocking(move || extract_blocking(&archive, &dest))
        .await
        .context("解压任务失败")?
}

fn extract_blocking(archive: &Path, dest: &Path) -> Result<()> {
    std::fs::create_dir_all(dest)?;
    let name = archive
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if name.ends_with(".zip") {
        extract_zip(archive, dest)
    } else if name.ends_with(".tar.gz") || name.ends_with(".tgz") {
        extract_tar_gz(archive, dest)
    } else if name.ends_with(".tar") {
        extract_tar(archive, dest)
    } else {
        Err(anyhow!("不支持的归档格式：{name}"))
    }
}

fn extract_zip(archive: &Path, dest: &Path) -> Result<()> {
    let file = File::open(archive)?;
    let mut zip = zip::ZipArchive::new(BufReader::new(file)).context("解析 ZIP 失败")?;
    for index in 0..zip.len() {
        let mut entry = zip.by_index(index).context("读取 ZIP 条目失败")?;
        let Some(relative) = entry.enclosed_name() else {
            continue;
        };
        let output = dest.join(relative);
        if entry.is_dir() {
            std::fs::create_dir_all(&output)?;
            continue;
        }
        if let Some(parent) = output.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut writer = BufWriter::new(File::create(&output)?);
        std::io::copy(&mut entry, &mut writer)?;
        writer.flush()?;
    }
    Ok(())
}

fn extract_tar_gz(archive: &Path, dest: &Path) -> Result<()> {
    let file = File::open(archive)?;
    let decoder = flate2::read::GzDecoder::new(BufReader::new(file));
    let mut tar = tar::Archive::new(decoder);
    tar.unpack(dest).context("解压 tar.gz 失败")
}

fn extract_tar(archive: &Path, dest: &Path) -> Result<()> {
    let file = File::open(archive)?;
    let mut tar = tar::Archive::new(BufReader::new(file));
    tar.unpack(dest).context("解压 tar 失败")
}
