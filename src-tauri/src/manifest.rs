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

pub const PYTHON_VERSION: &str = "3.12.7";
pub const UV_VERSION: &str = "0.12.5";
pub const GIT_VERSION: &str = "2.46.0";
pub const PNPM_VERSION: &str = "9.12.0";

pub fn requirement(kind: ToolKind) -> Result<ToolRequirement> {
    let (version, package) = match kind {
        ToolKind::Python => (PYTHON_VERSION.to_string(), None),
        ToolKind::Uv => {
            let os = crate::platform::detect_os();
            let arch = crate::platform::detect_arch();
            let archive_name = format!("uv-{}.tar.gz", triple(os, arch));
            let url = format!(
                "https://github.com/astral-sh/uv/releases/download/{UV_VERSION}/{archive_name}"
            );
            (
                UV_VERSION.to_string(),
                Some(PackageSpec {
                    version: UV_VERSION.to_string(),
                    url,
                    sha256: None,
                    archive_name,
                }),
            )
        }
        ToolKind::Git => {
            let os = crate::platform::detect_os();
            let arch = crate::platform::detect_arch();
            match (os, arch) {
                (OsKind::Windows, ArchKind::X86_64) => {
                    let archive_name = format!("MinGit-{GIT_VERSION}-64-bit.zip");
                    let url = format!(
                        "https://github.com/git-for-windows/git/releases/download/v{GIT_VERSION}.windows.1/{archive_name}"
                    );
                    (
                        GIT_VERSION.to_string(),
                        Some(PackageSpec {
                            version: GIT_VERSION.to_string(),
                            url,
                            sha256: None,
                            archive_name,
                        }),
                    )
                }
                (OsKind::Windows, ArchKind::Aarch64) => {
                    let archive_name = format!("MinGit-{GIT_VERSION}-arm64.zip");
                    let url = format!(
                        "https://github.com/git-for-windows/git/releases/download/v{GIT_VERSION}.windows.1/{archive_name}"
                    );
                    (
                        GIT_VERSION.to_string(),
                        Some(PackageSpec {
                            version: GIT_VERSION.to_string(),
                            url,
                            sha256: None,
                            archive_name,
                        }),
                    )
                }
                _ => {
                    return Err(anyhow!(
                        "当前发布包未提供 {os:?}/{arch:?} 的便携 Git 二进制，请在发布流水线补充 manifest 资产"
                    ))
                }
            }
        }
        ToolKind::Pnpm => {
            let (platform_part, arch_part) =
                match (crate::platform::detect_os(), crate::platform::detect_arch()) {
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
                    sha256: None,
                    archive_name,
                }),
            )
        }
    };
    Ok(ToolRequirement { version, package })
}
