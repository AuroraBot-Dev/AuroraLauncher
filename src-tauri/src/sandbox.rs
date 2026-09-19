use std::path::{Path, PathBuf};
use std::process::Command;

use crate::manifest::ToolKind;
use crate::platform::is_windows;
use crate::state::{RuntimePaths, SettingsStore};

pub struct Sandbox<'a> {
    paths: &'a RuntimePaths,
    settings: &'a SettingsStore,
}

impl<'a> Sandbox<'a> {
    pub fn new(paths: &'a RuntimePaths, settings: &'a SettingsStore) -> Self {
        Self { paths, settings }
    }

    pub fn command(&self, executable: &Path) -> Command {
        let mut cmd = Command::new(executable);
        cmd.env_clear();
        // 注入的 TEMP/APPDATA/缓存目录必须存在：uv/pip 安装依赖时会往 TEMP
        // 下建临时目录，缺失会直接报 os error 3。启动时预建过一次，这里每次
        // 拉起子进程前再兜底补建，避免运行中被清理后失败。
        self.paths.ensure_sandbox_dirs();
        let overrides = self.settings.snapshot().tool_dirs;

        // GUI 启动的子进程是控制台程序，隐藏它们自己的终端窗口
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }

        // env_clear 会把 Windows 系统级变量一并清掉。实测缺失 SystemRoot 等会让
        // git(curl 的线程化 DNS 解析) 直接以 “getaddrinfo() thread failed to start”
        // 失败。这里只从父进程挑出系统级变量补回，PATH 仍由下方显式控制，
        // 不引入宿主的用户级路径/代理，隔离目标不受影响。
        #[cfg(windows)]
        {
            const SYSTEM_VARS: &[(&str, &str)] = &[
                ("SystemRoot", "C:\\Windows"),
                ("windir", "C:\\Windows"),
                ("SystemDrive", "C:"),
                ("ProgramData", "C:\\ProgramData"),
                ("COMSPEC", "C:\\Windows\\System32\\cmd.exe"),
                ("PATHEXT", ".COM;.EXE;.BAT;.CMD"),
                ("NUMBER_OF_PROCESSORS", "1"),
                ("PROCESSOR_ARCHITECTURE", "AMD64"),
            ];
            for (name, fallback) in SYSTEM_VARS {
                let value = std::env::var_os(name).unwrap_or_else(|| fallback.into());
                cmd.env(name, value);
            }
        }

        let mut path_entries = self.paths.tool_path_entries(&overrides);
        // 受管工具缺失时会回退到系统 PATH 上的同名工具，但沙箱 PATH 只含受管目录，
        // 导致内核 `aurora setup` 里按名字调用的 uv/git/pnpm 找不到；这里把实际解析到的
        // 系统工具目录补进来，保证子进程按名字也能调用。
        for kind in [ToolKind::Uv, ToolKind::Git, ToolKind::Pnpm] {
            if self.paths.tool_exe(kind, &overrides).is_file() {
                continue;
            }
            if let Some(dir) =
                crate::tools::system_exe_path(kind).and_then(|exe| exe.parent().map(Path::to_path_buf))
            {
                path_entries.push(dir);
            }
        }
        // 系统 pnpm 是 npm 包装脚本，运行时还需要 node；受管 pnpm 是独立 exe，无需 node
        if !self.paths.tool_exe(ToolKind::Pnpm, &overrides).is_file() {
            if let Some(dir) =
                crate::tools::which_in_path("node").and_then(|exe| exe.parent().map(Path::to_path_buf))
            {
                path_entries.push(dir);
            }
        }
        if is_windows() {
            path_entries.push(PathBuf::from("C:\\Windows\\System32"));
            path_entries.push(PathBuf::from("C:\\Windows"));
        } else {
            path_entries.push(PathBuf::from("/usr/bin"));
            path_entries.push(PathBuf::from("/bin"));
        }
        let joined = std::env::join_paths(&path_entries).unwrap_or_default();
        cmd.env("PATH", joined);
        cmd.env("HOME", &self.paths.home);
        cmd.env("USERPROFILE", &self.paths.home);
        cmd.env("APPDATA", self.paths.home.join("AppData").join("Roaming"));
        cmd.env(
            "LOCALAPPDATA",
            self.paths.home.join("AppData").join("Local"),
        );
        cmd.env("TEMP", self.paths.home.join("temp"));
        cmd.env("TMP", self.paths.home.join("temp"));

        cmd.env("UV_CACHE_DIR", self.paths.root.join("cache").join("uv"));
        cmd.env(
            "UV_TOOL_DIR",
            self.paths.root.join("cache").join("uv-tools"),
        );
        cmd.env(
            "UV_PYTHON_INSTALL_DIR",
            self.paths.tool_dir(ToolKind::Python, &overrides),
        );
        cmd.env("UV_PROJECT_ENVIRONMENT", &self.paths.env_aurora);
        cmd.env("VIRTUAL_ENV", &self.paths.env_aurora);
        cmd.env("PIP_CACHE_DIR", self.paths.root.join("cache").join("pip"));
        // 包下载源：默认走国内 PyPI 镜像（只影响沙箱内的 uv/pip，不动宿主）
        if self.settings.download_source() != "official" {
            cmd.env("UV_DEFAULT_INDEX", crate::manifest::PYPI_MIRROR);
            cmd.env("PIP_INDEX_URL", crate::manifest::PYPI_MIRROR);
        }
        // GitHub 加速：非空时 uv 下载 Python 解释器改走镜像。
        // （工具安装包同样套这个前缀，见 tools.rs::install_package）
        let github_mirror = self.settings.github_mirror();
        if !github_mirror.is_empty() {
            cmd.env(
                "UV_PYTHON_INSTALL_MIRROR",
                crate::manifest::apply_github_mirror(
                    crate::manifest::PYTHON_BUILD_STANDALONE,
                    &github_mirror,
                ),
            );
        }
        cmd.env("PYTHONNOUSERSITE", "1");
        cmd.env("PYTHONUSERBASE", self.paths.home.join(".local"));
        // Windows 下 Python 默认按控制台代码页（简体中文为 GBK）编码管道输出，
        // 内核 `aurora about` 的 Unicode 图标会因此 UnicodeEncodeError；强制 UTF-8。
        cmd.env("PYTHONIOENCODING", "utf-8");
        cmd.env("PYTHONUTF8", "1");
        cmd.env("GIT_CONFIG_NOSYSTEM", "1");
        cmd.env("GIT_CONFIG_GLOBAL", self.paths.home.join(".gitconfig"));
        cmd.env(
            "NPM_CONFIG_CACHE",
            self.paths.root.join("cache").join("npm"),
        );
        cmd.env("PNPM_HOME", self.paths.home.join("pnpm"));
        cmd.env("npm_config_userconfig", self.paths.home.join(".npmrc"));

        cmd.env_remove("PYTHONHOME");
        cmd.env_remove("PYTHONPATH");
        cmd.env_remove("VIRTUAL_ENV_PROMPT");
        cmd.env_remove("CONDA_PREFIX");
        cmd.env_remove("CONDA_DEFAULT_ENV");
        cmd.env_remove("NVM_DIR");
        cmd.env_remove("NVM_BIN");
        cmd
    }
}
