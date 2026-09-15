use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Context, Result};
use futures_util::StreamExt;
use reqwest::header::{HeaderValue, RANGE};
use tokio::fs::OpenOptions;
use tokio::io::AsyncWriteExt;

use crate::hash::sha256_file;

/// 一次下载进度：已下载字节、总字节（0 表示未知）、瞬时速度（字节/秒）。
#[derive(Debug, Clone, Copy)]
pub struct DownloadProgress {
    pub current: u64,
    pub total: u64,
    pub speed: u64,
}

/// 单个请求的最大尝试次数：网络抖动/连接中断时从 `.part` 已下载的字节续传，
/// 不用从头再来。
const MAX_ATTEMPTS: u32 = 3;
/// 读超时：两次读到数据之间超过这个时长即判定连接卡死，触发重试。
const READ_TIMEOUT: Duration = Duration::from_secs(60);
/// 进度上报的最小间隔，避免每收一个 chunk 就发一次事件刷爆前端。
const PROGRESS_INTERVAL: Duration = Duration::from_millis(200);

/// 全局复用的下载客户端：连接池与 TLS 会话可复用，避免每次下载都重新握手。
///
/// 用 rustls 而不是 Windows schannel。schannel 会做证书吊销检查（CRL/OCSP），
/// 国内网络下吊销服务器常不可达，导致 TLS 握手中断（unexpected EOF during handshake）。
/// 证书走系统证书库（rustls-tls-native-roots），以兼容 Watt Toolkit 等
/// 网络加速软件安装的本地根证书；`system-proxy` 特性让 reqwest 自动读取系统代理。
static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();

fn client() -> Result<&'static reqwest::Client> {
    if CLIENT.get().is_none() {
        let built = reqwest::Client::builder()
            .user_agent("AuroraLauncher/0.1")
            .connect_timeout(Duration::from_secs(30))
            .read_timeout(READ_TIMEOUT)
            .tcp_nodelay(true)
            .build()
            .context("构建下载客户端失败")?;
        // 并发下可能两个线程同时构建，set 失败的那个丢掉即可，行为不受影响
        let _ = CLIENT.set(built);
    }
    CLIENT.get().ok_or_else(|| anyhow!("下载客户端不可用"))
}

pub async fn download_verified<F>(
    url: &str,
    dest: &Path,
    expected_sha256: Option<&str>,
    mut on_progress: F,
) -> Result<u64>
where
    F: FnMut(DownloadProgress) + Send,
{
    if dest.exists() {
        match expected_sha256 {
            // 已下载的缓存也要校验：否则被替换过的缓存会被一直复用
            Some(expected) => {
                let matches = sha256_file(dest)
                    .await
                    .map(|actual| actual.eq_ignore_ascii_case(expected))
                    .unwrap_or(false);
                if matches {
                    return Ok(std::fs::metadata(dest)?.len());
                }
                let _ = tokio::fs::remove_file(dest).await;
            }
            None => return Ok(std::fs::metadata(dest)?.len()),
        }
    }
    if let Some(parent) = dest.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    let mut part_path = dest.as_os_str().to_owned();
    part_path.push(".part");
    let part = PathBuf::from(part_path);

    fetch_into(&part, url, &mut on_progress).await?;
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
    on_progress(DownloadProgress {
        current: total,
        total,
        speed: 0,
    });
    tokio::fs::rename(&part, dest).await?;
    Ok(total)
}

/// 带重试地下载到 `part`：每次尝试都从 `.part` 已有字节续传，失败按指数退避再试。
async fn fetch_into<F>(part: &Path, url: &str, on_progress: &mut F) -> Result<u64>
where
    F: FnMut(DownloadProgress) + Send,
{
    let mut attempt = 0;
    loop {
        attempt += 1;
        let resume_from = tokio::fs::metadata(part)
            .await
            .map(|meta| meta.len())
            .unwrap_or(0);
        match fetch_once(part, url, resume_from, on_progress).await {
            Ok(downloaded) => return Ok(downloaded),
            Err(_) if attempt < MAX_ATTEMPTS => {
                // 中间失败只重试；最后一次仍失败才把错误抛出去
                tokio::time::sleep(Duration::from_millis(500 * 2u64.pow(attempt - 1))).await;
            }
            Err(error) => {
                return Err(error.context(format!("下载失败，已重试 {MAX_ATTEMPTS} 次")));
            }
        }
    }
}

/// 单次下载：返回写入的总字节数（含续传部分）。
async fn fetch_once<F>(part: &Path, url: &str, existing: u64, on_progress: &mut F) -> Result<u64>
where
    F: FnMut(DownloadProgress) + Send,
{
    let client = client()?;

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
            // 服务器不接受续传（临时文件比远端还大）：清掉重下
            let _ = tokio::fs::remove_file(part).await;
            resume_from = 0;
            continue;
        }
        if !response.status().is_success() {
            return Err(anyhow!("下载失败：HTTP {}", response.status()));
        }

        // 服务器接受续传（206）时追加写；否则（含忽略 Range 返回 200）清空重写，
        // 此时偏移必须从 0 开始，不能沿用 .part 的旧长度。
        let resuming = resume_from > 0 && response.status() == reqwest::StatusCode::PARTIAL_CONTENT;
        let start_at = if resuming { resume_from } else { 0 };
        let total_hint = if resuming {
            resume_from.saturating_add(response.content_length().unwrap_or(0))
        } else {
            response.content_length().unwrap_or(0)
        };
        on_progress(DownloadProgress {
            current: start_at,
            total: total_hint,
            speed: 0,
        });

        let mut file = if resuming {
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
        let mut offset = start_at;
        let mut window_start = Instant::now();
        let mut window_bytes = offset;
        while let Some(chunk) = stream.next().await {
            let bytes = chunk.context("读取下载数据失败")?;
            file.write_all(&bytes).await.context("写入下载数据失败")?;
            offset += bytes.len() as u64;
            // 按固定时间窗计算瞬时速度，并顺带限流进度事件
            let elapsed = window_start.elapsed();
            if elapsed >= PROGRESS_INTERVAL {
                let speed = ((offset - window_bytes) as f64 / elapsed.as_secs_f64()) as u64;
                window_start = Instant::now();
                window_bytes = offset;
                on_progress(DownloadProgress {
                    current: offset,
                    total: total_hint,
                    speed,
                });
            }
        }
        file.flush().await.ok();
        return Ok(offset);
    }
}
