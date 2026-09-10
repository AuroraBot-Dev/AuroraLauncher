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
        cmd.env("PYTHONNOUSERSITE", "1");
        cmd.env("PYTHONUSERBASE", self.paths.home.join(".local"));
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
