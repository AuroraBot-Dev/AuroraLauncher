use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use futures_util::StreamExt;
use reqwest::header::{HeaderValue, RANGE};
use tokio::fs::OpenOptions;
use tokio::io::AsyncWriteExt;

use crate::hash::sha256_file;

pub async fn download_verified<F>(
    url: &str,
    dest: &Path,
    expected_sha256: Option<&str>,
    mut on_progress: F,
) -> Result<u64>
where
    F: FnMut(u64, u64) + Send,
{
    if dest.exists() {
        return Ok(std::fs::metadata(dest)?.len());
    }
    if let Some(parent) = dest.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    let mut part_path = dest.as_os_str().to_owned();
    part_path.push(".part");
    let part = PathBuf::from(part_path);

    let existing = tokio::fs::metadata(&part)
        .await
        .map(|meta| meta.len())
        .unwrap_or(0);

    let _downloaded = fetch_into(&part, url, existing, &mut on_progress).await?;
    if let Some(expected) = expected_sha256 {
        let actual = sha256_file(&part).await?;
        if !actual.eq_ignore_ascii_case(expected) {
            let _ = tokio::fs::remove_file(&part).await;
            return Err(anyhow!(
                "SHA256 校验失败：期望 {expected}，实际 {actual}\n来源：{url}"
            ));
        }
    }

    let total = std::fs::metadata(&part)?.len();
    on_progress(total, total);
    tokio::fs::rename(&part, dest).await?;
    Ok(total)
}

async fn fetch_into<F>(part: &Path, url: &str, existing: u64, on_progress: &mut F) -> Result<u64>
where
    F: FnMut(u64, u64) + Send,
{
    let client = reqwest::Client::builder()
        .user_agent("AuroraLauncher/0.1")
        .build()
        .context("构建下载客户端失败")?;

    let mut resume_from = existing;
    loop {
        let mut request = client.get(url);
        if resume_from > 0 {
            request = request.header(
                RANGE,
                HeaderValue::from_str(&format!("bytes={resume_from}-"))
                    .context("构造 Range 请求头失败")?,
            );
        }

        let response = request.send().await.context("发起下载请求失败")?;
        if response.status() == reqwest::StatusCode::RANGE_NOT_SATISFIABLE && resume_from > 0 {
            let _ = tokio::fs::remove_file(part).await;
            resume_from = 0;
            continue;
        }
        if !response.status().is_success() {
            return Err(anyhow!("下载失败：HTTP {}", response.status()));
        }

        let total_hint = if response.status() == reqwest::StatusCode::PARTIAL_CONTENT {
            resume_from.saturating_add(response.content_length().unwrap_or(0))
        } else {
            response.content_length().unwrap_or(0)
        };
        on_progress(resume_from, total_hint);

        let mut file =
            if resume_from > 0 && response.status() == reqwest::StatusCode::PARTIAL_CONTENT {
                if let Some(parent) = part.parent() {
                    tokio::fs::create_dir_all(parent).await?;
                }
                OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(part)
                    .await
                    .context("打开续传文件失败")?
            } else {
                if part.exists() {
                    let _ = tokio::fs::remove_file(part).await;
                }
                if let Some(parent) = part.parent() {
                    tokio::fs::create_dir_all(parent).await?;
                }
                OpenOptions::new()
                    .create_new(true)
                    .write(true)
                    .open(part)
                    .await
                    .context("创建下载文件失败")?
            };

        let mut stream = response.bytes_stream();
        let mut offset = resume_from;
        while let Some(chunk) = stream.next().await {
            let bytes = chunk.context("读取下载数据失败")?;
            file.write_all(&bytes).await.context("写入下载数据失败")?;
            offset += bytes.len() as u64;
            on_progress(offset, total_hint);
        }
        file.flush().await.ok();
        return Ok(offset);
    }
}
