use std::path::Path;

use anyhow::{bail, Context, Result};
use serde::Serialize;
use tauri::State;

use crate::state::AppState;

/// 一个需要在 `.env` 里填写的变量（密钥或平台参数）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvEntry {
    pub name: String,
    pub value: String,
    pub secret: bool,
}

/// `config/apps.toml` 里的一个平台（MCP App）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppEntry {
    pub package: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LauncherConfig {
    pub env: Vec<EnvEntry>,
    pub apps: Vec<AppEntry>,
}

fn models_path(root: &Path) -> std::path::PathBuf {
    root.join("config").join("models.toml")
}

fn apps_path(root: &Path) -> std::path::PathBuf {
    root.join("config").join("apps.toml")
}

fn env_path(root: &Path) -> std::path::PathBuf {
    root.join(".env")
}

/// 解析 `key = "value"` 或 `key = value` 形式的一行。
fn parse_assign(line: &str) -> Option<(String, String)> {
    let (key, value) = line.split_once('=')?;
    let key = key.trim().to_string();
    if key.is_empty() {
        return None;
    }
    Some((key, value.trim().trim_matches('"').to_string()))
}

/// 提取一行里所有 `"..."` 字符串。
fn quoted_strings(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = line;
    while let Some(start) = rest.find('"') {
        let after = &rest[start + 1..];
        match after.find('"') {
            Some(end) => {
                out.push(after[..end].to_string());
                rest = &after[end + 1..];
            }
            None => break,
        }
    }
    out
}

/// `.env` 里非注释的 `KEY=VALUE`。
fn read_env_map(text: &str) -> std::collections::HashMap<String, String> {
    let mut map = std::collections::HashMap::new();
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            map.insert(key.trim().to_string(), value.trim().to_string());
        }
    }
    map
}

/// 从 models.toml / apps.toml 收集需要在 `.env` 填写的变量名。
fn required_env_names(root: &Path) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    let mut push = |name: String| {
        if !name.is_empty() && !names.contains(&name) {
            names.push(name);
        }
    };

    if let Ok(text) = std::fs::read_to_string(models_path(root)) {
        for raw in text.lines() {
            let line = raw.split('#').next().unwrap_or("").trim();
            if let Some((key, value)) = parse_assign(line) {
                if key == "secret_env" {
                    push(value);
                }
            }
        }
    }
    if let Ok(text) = std::fs::read_to_string(apps_path(root)) {
        for raw in text.lines() {
            let line = raw.split('#').next().unwrap_or("").trim();
            if let Some((key, value)) = line.split_once('=') {
                if key.trim() == "env" {
                    for name in quoted_strings(value) {
                        push(name);
                    }
                }
            }
        }
    }
    names
}

/// 解析 `config/apps.toml` 里所有 `[[app]]` 的 package 与 enabled。
fn read_apps(root: &Path) -> Vec<AppEntry> {
    let text = std::fs::read_to_string(apps_path(root)).unwrap_or_default();
    let mut apps = Vec::new();
    let mut in_app = false;
    let mut package: Option<String> = None;
    let mut enabled: Option<bool> = None;

    let flush = |package: &mut Option<String>, enabled: &mut Option<bool>, apps: &mut Vec<AppEntry>| {
        if let Some(pkg) = package.take() {
            apps.push(AppEntry {
                package: pkg,
                enabled: enabled.take().unwrap_or(false),
            });
        }
        *enabled = None;
    };

    for raw in text.lines() {
        let line = raw.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with("[[app]]") {
            if in_app {
                flush(&mut package, &mut enabled, &mut apps);
            }
            in_app = true;
            continue;
        }
        if line.starts_with('[') {
            if in_app {
                flush(&mut package, &mut enabled, &mut apps);
            }
            in_app = false;
            continue;
        }
        if !in_app {
            continue;
        }
        if let Some((key, value)) = parse_assign(line) {
            match key.as_str() {
                "package" if package.is_none() => package = Some(value),
                "enabled" if enabled.is_none() => enabled = Some(value == "true"),
                _ => {}
            }
        }
    }
    if in_app {
        flush(&mut package, &mut enabled, &mut apps);
    }
    apps
}

#[tauri::command]
pub fn read_launcher_config(state: State<'_, AppState>) -> Result<LauncherConfig, String> {
    let root = &state.paths.kernel_aurora;
    let env_text = std::fs::read_to_string(env_path(root)).unwrap_or_default();
    let env_map = read_env_map(&env_text);

    let env = required_env_names(root)
        .into_iter()
        .map(|name| {
            let secret = name.contains("KEY") || name.contains("TOKEN") || name.contains("SECRET");
            EnvEntry {
                value: env_map.get(&name).cloned().unwrap_or_default(),
                name,
                secret,
            }
        })
        .collect();

    Ok(LauncherConfig {
        env,
        apps: read_apps(root),
    })
}

#[tauri::command]
pub fn set_env_value(state: State<'_, AppState>, name: String, value: String) -> Result<(), String> {
    let root = &state.paths.kernel_aurora;
    write_env_value(&env_path(root), &name, &value).map_err(|e| format!("{e:#}"))
}

#[tauri::command]
pub fn set_app_enabled(
    state: State<'_, AppState>,
    package: String,
    enabled: bool,
) -> Result<(), String> {
    let root = &state.paths.kernel_aurora;
    write_app_enabled(&apps_path(root), &package, enabled).map_err(|e| format!("{e:#}"))
}

/// 更新（或追加）`.env` 里的一个变量，保留原有注释与其它行。
fn write_env_value(path: &Path, name: &str, value: &str) -> Result<()> {
    let text = std::fs::read_to_string(path).unwrap_or_default();
    let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
    let mut replaced = false;
    for line in lines.iter_mut() {
        let trimmed = line.trim_start();
        if trimmed.starts_with('#') {
            continue;
        }
        if let Some((key, _)) = line.split_once('=') {
            if key.trim() == name {
                *line = format!("{name}={value}");
                replaced = true;
                break;
            }
        }
    }
    if !replaced {
        lines.push(format!("{name}={value}"));
    }
    std::fs::write(path, lines.join("\n") + "\n")
        .with_context(|| format!("写入 {} 失败", path.display()))
}

/// 切换 `config/apps.toml` 里某个 `[[app]]` 的 enabled，保留其余内容。
fn write_app_enabled(path: &Path, package: &str, enabled: bool) -> Result<()> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("读取 {} 失败", path.display()))?;
    let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
    let mut in_target = false;
    let mut changed = false;
    for line in lines.iter_mut() {
        let content = line.split('#').next().unwrap_or("").trim().to_string();
        if content.starts_with("[[app]]") || content.starts_with('[') {
            in_target = false;
            continue;
        }
        if let Some((key, value)) = parse_assign(&content) {
            if key == "package" {
                in_target = value == package;
            } else if in_target && key == "enabled" {
                let indent: String = line.chars().take_while(|c| c.is_whitespace()).collect();
                *line = format!("{indent}enabled = {}", if enabled { "true" } else { "false" });
                changed = true;
                break;
            }
        }
    }
    if !changed {
        bail!("未在 config/apps.toml 找到平台 {package}");
    }
    std::fs::write(path, lines.join("\n") + "\n")
        .with_context(|| format!("写入 {} 失败", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("aurora-cfg-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("config")).unwrap();
        dir
    }

    #[test]
    fn parses_apps_with_comments() {
        let dir = temp_dir("apps");
        std::fs::write(
            dir.join("config/apps.toml"),
            "# top\n[[app]]\npackage = \"org.aurora.clock\" # clock\nenabled = false\n\n[[app]]\npackage = \"org.aurora.qq\"\nenabled = true\n",
        )
        .unwrap();
        let apps = read_apps(&dir);
        assert_eq!(apps.len(), 2);
        assert_eq!(apps[0].package, "org.aurora.clock");
        assert!(!apps[0].enabled);
        assert_eq!(apps[1].package, "org.aurora.qq");
        assert!(apps[1].enabled);
    }

    #[test]
    fn toggles_app_enabled_keeps_comments() {
        let dir = temp_dir("toggle");
        let path = dir.join("config/apps.toml");
        std::fs::write(
            &path,
            "# keep\n[[app]]\npackage = \"org.aurora.qq\" # qq\nenabled = false\n",
        )
        .unwrap();
        write_app_enabled(&path, "org.aurora.qq", true).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("enabled = true"));
        assert!(text.contains("# keep"));
        assert!(text.contains("package = \"org.aurora.qq\" # qq"));
    }

    #[test]
    fn writes_env_value_keeps_other_lines() {
        let dir = temp_dir("env");
        let path = dir.join(".env");
        std::fs::write(&path, "# comment\nDEEPSEEK_API_KEY=\nOTHER=x\n").unwrap();
        write_env_value(&path, "DEEPSEEK_API_KEY", "sk-123").unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("DEEPSEEK_API_KEY=sk-123"));
        assert!(text.contains("# comment"));
        assert!(text.contains("OTHER=x"));
    }

    #[test]
    fn collects_required_env_names() {
        let dir = temp_dir("names");
        std::fs::write(
            dir.join("config/models.toml"),
            "[models.providers.deepseek]\nsecret_env = \"DEEPSEEK_API_KEY\"\n",
        )
        .unwrap();
        std::fs::write(
            dir.join("config/apps.toml"),
            "[[app]]\npackage = \"org.aurora.qq\"\nenv = [\"AURORA_QQ_TOKEN\", \"AURORA_QQ_CONFIG\"]\n",
        )
        .unwrap();
        let names = required_env_names(&dir);
        assert!(names.contains(&"DEEPSEEK_API_KEY".to_string()));
        assert!(names.contains(&"AURORA_QQ_TOKEN".to_string()));
        assert!(names.contains(&"AURORA_QQ_CONFIG".to_string()));
    }
}
