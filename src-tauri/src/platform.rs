use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OsKind {
    Windows,
    MacOS,
    Linux,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ArchKind {
    X86_64,
    Aarch64,
}

pub fn detect_os() -> OsKind {
    match std::env::consts::OS {
        "windows" => OsKind::Windows,
        "macos" => OsKind::MacOS,
        _ => OsKind::Linux,
    }
}

pub fn detect_arch() -> ArchKind {
    match std::env::consts::ARCH {
        "aarch64" | "arm64" => ArchKind::Aarch64,
        _ => ArchKind::X86_64,
    }
}

pub fn is_windows() -> bool {
    matches!(detect_os(), OsKind::Windows)
}

pub fn exe_suffix() -> &'static str {
    if is_windows() {
        ".exe"
    } else {
        ""
    }
}

pub fn triple(os: OsKind, arch: ArchKind) -> String {
    let os_part = match os {
        OsKind::Windows => "pc-windows-msvc",
        OsKind::MacOS => "apple-darwin",
        OsKind::Linux => "unknown-linux-gnu",
    };
    let arch_part = match arch {
        ArchKind::X86_64 => "x86_64",
        ArchKind::Aarch64 => "aarch64",
    };
    format!("{arch_part}-{os_part}")
}
