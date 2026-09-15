use std::fmt;

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};

use crate::platform::{triple, ArchKind, OsKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolKind {
    Python,
    Uv,
    Git,
    Pnpm,
}

impl ToolKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Python => "python",
            Self::Uv => "uv",
            Self::Git => "git",
            Self::Pnpm => "pnpm",
        }
    }
}

impl fmt::Display for ToolKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.label())
    }
}

#[derive(Debug, Clone)]
pub struct PackageSpec {
    pub version: String,
    pub url: String,
    pub sha256: Option<String>,
    pub archive_name: String,
}

#[derive(Debug, Clone)]
pub struct ToolRequirement {
    pub version: String,
    pub package: Option<PackageSpec>,
}

/// 国内 PyPI 镜像；下载源为 mirror（默认）时，内核 `uv sync` 走它。
pub const PYPI_MIRROR: &str = "https://pypi.tuna.tsinghua.edu.cn/simple";

/// Python 官方独立构建（uv 下载解释器的来源），用于拼接 `UV_PYTHON_INSTALL_MIRROR`。
pub const PYTHON_BUILD_STANDALONE: &str =
    "https://github.com/astral-sh/python-build-standalone/releases/download";

/// 给 GitHub 地址套上加速前缀（如 `https://ghproxy.net/`）。
/// 前缀为空时原样返回（直连）；末尾多余的 `/` 会被规整掉，避免用户漏填或重复。
pub fn apply_github_mirror(url: &str, prefix: &str) -> String {
    let prefix = prefix.trim().trim_end_matches('/');
    if prefix.is_empty() {
        return url.to_string();
    }
    format!("{prefix}/{url}")
}

/// 校验 GitHub 加速前缀：允许留空（直连），否则必须是 http/https 地址。
pub fn validate_github_mirror(prefix: &str) -> Result<()> {
    let prefix = prefix.trim();
    if prefix.is_empty() {
        return Ok(());
    }
    if !(prefix.starts_with("https://") || prefix.starts_with("http://")) {
        return Err(anyhow!("加速前缀需以 http:// 或 https:// 开头（留空表示直连）"));
    }
    Ok(())
}

pub const PYTHON_VERSION: &str = "3.12.7";
pub const UV_VERSION: &str = "0.12.10";
pub const GIT_VERSION: &str = "2.55.0.5";
/// MinGit 的 Release tag：Git for Windows 的 tag 形如 `v2.55.0.windows.5`，
/// 段顺序和版本号 `2.55.0.5` 不同，所以单独记一份。
pub const GIT_RELEASE_TAG: &str = "v2.55.0.windows.5";
pub const PNPM_VERSION: &str = "9.12.0";

/// 各平台安装包的 SHA256。
///
/// 来源是 GitHub Release 资产的 `digest` 字段（`GET /repos/{owner}/{repo}/releases/tags/{tag}`），
/// 有值时 `download_verified` 会强制校验，从而挡住被篡改的镜像 / 加速前缀——
/// 这一点在使用第三方 GitHub 加速源时尤其重要。
///
/// 缺失的平台返回 `None`（不校验）：pnpm `v9.12.0` 上传得比 GitHub 的 digest 功能早，
/// 资产既没有 `digest` 也没有 `.sha256`，只能下载后自行计算（见 README）。
fn sha256_of(kind: ToolKind, os: OsKind, arch: ArchKind) -> Option<&'static str> {
    match (kind, os, arch) {
        (ToolKind::Uv, OsKind::Windows, ArchKind::X86_64) => {
            Some("f65744f94072152b1f86ba2aace4d01f1124d9a8ecb235805039e3718c36cac2")
        }
        (ToolKind::Uv, OsKind::Windows, ArchKind::Aarch64) => {
            Some("ee985c51c0c9c1f82267a5d80f959b34a7ff888c109182bd3b2b35c4661bbcde")
        }
        (ToolKind::Uv, OsKind::MacOS, ArchKind::X86_64) => {
            Some("5296d5aa2b9143360405eea866f8ef4d5dc8986b164eb0dc35e8f876a9304d30")
        }
        (ToolKind::Uv, OsKind::MacOS, ArchKind::Aarch64) => {
            Some("51c6170e8e3a01cef9f33b94f582b7b81ac65046f55d40afb35f9cff5a68c179")
        }
        (ToolKind::Uv, OsKind::Linux, ArchKind::X86_64) => {
            Some("173d95a0c32d18c896c46ba6fafbf3cf9c14ab74b033f81b76c883ef492a976b")
        }
        (ToolKind::Uv, OsKind::Linux, ArchKind::Aarch64) => {
            Some("9ff6b9d4665edcdd3a88dcc73cd1eb641754deb927f14e8c62ebfde6bf4f5f5e")
        }
        (ToolKind::Git, OsKind::Windows, ArchKind::X86_64) => {
            Some("56d7b226b7693196cfc71fef26568f536c4a021ab6c37ff2db4287bed908e96e")
        }
        (ToolKind::Git, OsKind::Windows, ArchKind::Aarch64) => {
            Some("05843f9d6e60306c3ab886799e2c67200caab921571f10512df3493049179ddb")
        }
        _ => None,
    }
}

pub fn requirement(kind: ToolKind) -> Result<ToolRequirement> {
    let os = crate::platform::detect_os();
    let arch = crate::platform::detect_arch();
    let sha256 = sha256_of(kind, os, arch).map(str::to_string);

    let (version, package) = match kind {
        ToolKind::Python => (PYTHON_VERSION.to_string(), None),
        ToolKind::Uv => {
            // uv 官方发布：Windows 是 .zip，macOS/Linux 是 .tar.gz
            let suffix = if matches!(os, OsKind::Windows) {
                ".zip"
            } else {
                ".tar.gz"
            };
            let archive_name = format!("uv-{}{suffix}", triple(os, arch));
            let url = format!(
                "https://github.com/astral-sh/uv/releases/download/{UV_VERSION}/{archive_name}"
            );
            (
                UV_VERSION.to_string(),
                Some(PackageSpec {
                    version: UV_VERSION.to_string(),
                    url,
                    sha256,
                    archive_name,
                }),
            )
        }
        ToolKind::Git => match (os, arch) {
            (OsKind::Windows, ArchKind::X86_64) => {
                let archive_name = format!("MinGit-{GIT_VERSION}-64-bit.zip");
                let url = format!(
                    "https://github.com/git-for-windows/git/releases/download/{GIT_RELEASE_TAG}/{archive_name}"
                );
                (
                    GIT_VERSION.to_string(),
                    Some(PackageSpec {
                        version: GIT_VERSION.to_string(),
                        url,
                        sha256,
                        archive_name,
                    }),
                )
            }
            (OsKind::Windows, ArchKind::Aarch64) => {
                let archive_name = format!("MinGit-{GIT_VERSION}-arm64.zip");
                let url = format!(
                    "https://github.com/git-for-windows/git/releases/download/{GIT_RELEASE_TAG}/{archive_name}"
                );
                (
                    GIT_VERSION.to_string(),
                    Some(PackageSpec {
                        version: GIT_VERSION.to_string(),
                        url,
                        sha256,
                        archive_name,
                    }),
                )
            }
            _ => {
                return Err(anyhow!(
                    "当前发布包未提供 {os:?}/{arch:?} 的便携 Git 二进制，请在发布流水线补充 manifest 资产"
                ))
            }
        },
        ToolKind::Pnpm => {
            let (platform_part, arch_part) = match (os, arch) {
                (OsKind::Windows, ArchKind::X86_64) => ("win", "x64"),
                (OsKind::Windows, ArchKind::Aarch64) => ("win", "arm64"),
                (OsKind::MacOS, ArchKind::X86_64) => ("macos", "x64"),
                (OsKind::MacOS, ArchKind::Aarch64) => ("macos", "arm64"),
                (OsKind::Linux, ArchKind::X86_64) => ("linux", "x64"),
                (OsKind::Linux, ArchKind::Aarch64) => ("linux", "arm64"),
            };
            let suffix = if crate::platform::is_windows() {
                ".exe"
            } else {
                ""
            };
            let archive_name = format!("pnpm-{platform_part}-{arch_part}{suffix}");
            let url = format!(
                "https://github.com/pnpm/pnpm/releases/download/v{PNPM_VERSION}/{archive_name}"
            );
            (
                PNPM_VERSION.to_string(),
                Some(PackageSpec {
                    version: PNPM_VERSION.to_string(),
                    url,
                    sha256,
                    archive_name,
                }),
            )
        }
    };
    Ok(ToolRequirement { version, package })
}
