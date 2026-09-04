use std::path::{Path, PathBuf};
use std::process::Command;

use crate::platform::is_windows;
use crate::state::RuntimePaths;

pub struct Sandbox<'a> {
    paths: &'a RuntimePaths,
}

impl<'a> Sandbox<'a> {
    pub fn new(paths: &'a RuntimePaths) -> Self {
        Self { paths }
    }

    pub fn command(&self, executable: &Path) -> Command {
        let mut cmd = Command::new(executable);
        cmd.env_clear();

        let mut path_entries = self.paths.tool_path_entries();
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
        cmd.env("UV_PYTHON_INSTALL_DIR", &self.paths.tools_python);
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
