//! Cross-platform OS path discovery, Steam/Proton/Heroic/Wine integration,
//! browser launching, and Linux desktop runtime helpers.

use anyhow::{Context, Result};
use std::collections::HashSet;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::SystemTime;

const SATISFACTORY_STEAM_APP_ID: &str = "526870";

/// Parses Valve's `libraryfolders.vdf` contents and returns every `"path"` entry.
pub fn parse_steam_library_folders_vdf(content: &str) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("//") {
            continue;
        }
        let tokens = extract_vdf_quoted_tokens(trimmed);
        if tokens.len() >= 2 && tokens[0].eq_ignore_ascii_case("path") && !tokens[1].is_empty() {
            paths.push(PathBuf::from(&tokens[1]));
        }
    }
    paths
}

fn extract_vdf_quoted_tokens(line: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '"' {
            let mut val = String::new();
            while let Some(inner) = chars.next() {
                if inner == '\\' {
                    match chars.next() {
                        Some('\\') => val.push('\\'),
                        Some('"') => val.push('"'),
                        Some('n') => val.push('\n'),
                        Some('t') => val.push('\t'),
                        Some(other) => {
                            val.push('\\');
                            val.push(other);
                        }
                        None => break,
                    }
                } else if inner == '"' {
                    break;
                } else {
                    val.push(inner);
                }
            }
            tokens.push(val);
        }
    }
    tokens
}

fn push_unique_existing_dir(
    out: &mut Vec<PathBuf>,
    seen: &mut HashSet<PathBuf>,
    candidate: PathBuf,
) {
    if !candidate.is_dir() {
        return;
    }
    let key = fs::canonicalize(&candidate).unwrap_or_else(|_| candidate.clone());
    if seen.insert(key) {
        out.push(candidate);
    }
}

/// Discovers existing Steam root and library folders across Linux and Windows.
pub fn discover_steam_libraries() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Some(steam_dir) = env::var_os("STEAM_DIR").filter(|v| !v.is_empty()) {
        roots.push(PathBuf::from(steam_dir));
    }

    #[cfg(not(target_os = "windows"))]
    {
        let home = env::var_os("HOME").map(PathBuf::from);
        let xdg_data_home = env::var_os("XDG_DATA_HOME")
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .or_else(|| home.as_ref().map(|h| h.join(".local").join("share")));

        if let Some(data_home) = xdg_data_home {
            roots.push(data_home.join("Steam"));
        }
        if let Some(h) = home {
            roots.push(h.join(".steam").join("steam"));
            roots.push(h.join(".steam").join("root"));
            roots.push(
                h.join(".var")
                    .join("app")
                    .join("com.valvesoftware.Steam")
                    .join(".local")
                    .join("share")
                    .join("Steam"),
            );
            roots.push(
                h.join(".var")
                    .join("app")
                    .join("com.valvesoftware.Steam")
                    .join(".steam")
                    .join("steam"),
            );
            roots.push(
                h.join("snap")
                    .join("steam")
                    .join("common")
                    .join(".local")
                    .join("share")
                    .join("Steam"),
            );
        }
    }

    #[cfg(target_os = "windows")]
    {
        roots.push(PathBuf::from(r"C:\Program Files (x86)\Steam"));
        roots.push(PathBuf::from(r"C:\Program Files\Steam"));
    }

    let mut libraries = Vec::new();
    let mut seen = HashSet::new();

    for root in roots {
        if !root.is_dir() {
            continue;
        }
        push_unique_existing_dir(&mut libraries, &mut seen, root.clone());

        for rel in ["steamapps/libraryfolders.vdf", "config/libraryfolders.vdf"] {
            let vdf_path = root.join(rel);
            if let Ok(content) = fs::read_to_string(&vdf_path) {
                for lib in parse_steam_library_folders_vdf(&content) {
                    push_unique_existing_dir(&mut libraries, &mut seen, lib);
                }
            }
        }
    }

    libraries
}

fn dir_contains_sav_files(dir: &Path) -> bool {
    let Ok(entries) = fs::read_dir(dir) else {
        return false;
    };
    entries.flatten().any(|entry| {
        let p = entry.path();
        p.is_file()
            && p.extension()
                .and_then(|e| e.to_str())
                .map_or(false, |ext| ext.eq_ignore_ascii_case("sav"))
    })
}

fn newest_sav_mtime_in_dir(dir: &Path) -> Option<SystemTime> {
    let entries = fs::read_dir(dir).ok()?;
    let mut newest: Option<SystemTime> = None;
    for entry in entries.flatten() {
        let sp = entry.path();
        if sp.is_file()
            && sp
                .extension()
                .and_then(|e| e.to_str())
                .map_or(false, |ext| ext.eq_ignore_ascii_case("sav"))
        {
            if let Ok(mtime) = sp.metadata().and_then(|m| m.modified()) {
                if newest.map_or(true, |cur| mtime > cur) {
                    newest = Some(mtime);
                }
            }
        }
    }
    newest
}

fn newest_sav_mtime_in_save_root(save_games_root: &Path) -> Option<SystemTime> {
    let entries = fs::read_dir(save_games_root).ok()?;
    let mut newest: Option<SystemTime> = None;
    for entry in entries.flatten() {
        let p = entry.path();
        if !p.is_dir() {
            continue;
        }
        let dir_name = p
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        if dir_name.eq_ignore_ascii_case("server") || dir_name.eq_ignore_ascii_case("blueprints") {
            continue;
        }
        if let Some(mtime) = newest_sav_mtime_in_dir(&p) {
            if newest.map_or(true, |cur| mtime > cur) {
                newest = Some(mtime);
            }
        }
    }
    newest
}

/// Selects the active profile subdirectory inside a `FactoryGame/Saved/SaveGames` root,
/// preferring the directory with the most recently modified `.sav` file and skipping
/// `server` and `blueprints`.
pub fn select_profile_dir_in_root(save_games_root: &Path) -> Result<PathBuf, String> {
    if !save_games_root.exists() {
        return Err(format!(
            "Satisfactory SaveGames folder not found at {:?}",
            save_games_root
        ));
    }

    let mut best_dir: Option<(SystemTime, PathBuf)> = None;
    let mut first_profile_dir: Option<PathBuf> = None;

    if let Ok(entries) = fs::read_dir(save_games_root) {
        for entry in entries.flatten() {
            let p = entry.path();
            if !p.is_dir() {
                continue;
            }
            let dir_name = p
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            if dir_name.eq_ignore_ascii_case("server")
                || dir_name.eq_ignore_ascii_case("blueprints")
            {
                continue;
            }
            if first_profile_dir.is_none() {
                first_profile_dir = Some(p.clone());
            }
            if let Some(mtime) = newest_sav_mtime_in_dir(&p) {
                if best_dir.as_ref().map_or(true, |(t, _)| mtime > *t) {
                    best_dir = Some((mtime, p.clone()));
                }
            }
        }
    }

    best_dir
        .map(|(_, d)| d)
        .or(first_profile_dir)
        .ok_or_else(|| format!("No profile save folder found inside {:?}", save_games_root))
}

#[cfg(not(target_os = "windows"))]
fn collect_wine_prefix_save_roots(
    prefix_root: &Path,
    candidates: &mut Vec<PathBuf>,
    seen: &mut HashSet<PathBuf>,
) {
    let users_dir = prefix_root.join("drive_c").join("users");
    if !users_dir.is_dir() {
        return;
    }

    let mut user_dirs = Vec::new();
    let steamuser_dir = users_dir.join("steamuser");
    if steamuser_dir.is_dir() {
        user_dirs.push(steamuser_dir);
    }
    if let Ok(entries) = fs::read_dir(&users_dir) {
        for entry in entries.flatten() {
            let p = entry.path();
            if !p.is_dir() {
                continue;
            }
            if p.file_name().and_then(|n| n.to_str()) == Some("steamuser") {
                continue;
            }
            user_dirs.push(p);
        }
    }

    for user_dir in user_dirs {
        let appdata_local = user_dir
            .join("AppData")
            .join("Local")
            .join("FactoryGame")
            .join("Saved")
            .join("SaveGames");
        push_unique_existing_dir(candidates, seen, appdata_local);

        let local_settings = user_dir
            .join("Local Settings")
            .join("Application Data")
            .join("FactoryGame")
            .join("Saved")
            .join("SaveGames");
        push_unique_existing_dir(candidates, seen, local_settings);
    }
}

#[cfg(not(target_os = "windows"))]
fn collect_heroic_save_roots(
    home: &Path,
    candidates: &mut Vec<PathBuf>,
    seen: &mut HashSet<PathBuf>,
) {
    let heroic_prefixes = home.join("Games").join("Heroic").join("Prefixes");
    if !heroic_prefixes.is_dir() {
        return;
    }

    let mut prefix_dirs = Vec::new();
    if let Ok(entries) = fs::read_dir(&heroic_prefixes) {
        for entry in entries.flatten() {
            let p = entry.path();
            if !p.is_dir() {
                continue;
            }
            if p.file_name().and_then(|n| n.to_str()) == Some("default") {
                if let Ok(sub_entries) = fs::read_dir(&p) {
                    for sub in sub_entries.flatten() {
                        let sp = sub.path();
                        if sp.is_dir() {
                            prefix_dirs.push(sp);
                        }
                    }
                }
            } else {
                prefix_dirs.push(p);
            }
        }
    }

    for prefix in prefix_dirs {
        collect_wine_prefix_save_roots(&prefix.join("pfx"), candidates, seen);
        collect_wine_prefix_save_roots(&prefix, candidates, seen);
    }
}

/// Locates the `FactoryGame/Saved/SaveGames` root directory across Windows,
/// Steam/Proton, Heroic, Wine, and native Linux paths.
pub fn find_satisfactory_save_games_root() -> Result<PathBuf, String> {
    if let Some(override_val) = env::var_os("SATISFACTORY_SAVE_DIR").filter(|v| !v.is_empty()) {
        let override_path = PathBuf::from(override_val);
        if override_path.is_dir() {
            return Ok(override_path);
        }
        return Err(format!(
            "SATISFACTORY_SAVE_DIR does not exist or is not a directory: {:?}",
            override_path
        ));
    }

    #[cfg(target_os = "windows")]
    {
        let local_app_data = env::var("LOCALAPPDATA")
            .map_err(|_| "LOCALAPPDATA environment variable not found".to_string())?;
        let save_games_root = PathBuf::from(local_app_data)
            .join("FactoryGame")
            .join("Saved")
            .join("SaveGames");
        if save_games_root.is_dir() {
            return Ok(save_games_root);
        }
        return Err(format!(
            "Satisfactory SaveGames folder not found at {:?}. Set SATISFACTORY_SAVE_DIR to override.",
            save_games_root
        ));
    }

    #[cfg(not(target_os = "windows"))]
    {
        let mut candidates = Vec::new();
        let mut seen = HashSet::new();

        for lib in discover_steam_libraries() {
            let pfx_root = lib
                .join("steamapps")
                .join("compatdata")
                .join(SATISFACTORY_STEAM_APP_ID)
                .join("pfx");
            collect_wine_prefix_save_roots(&pfx_root, &mut candidates, &mut seen);
        }

        if let Some(home) = env::var_os("HOME").map(PathBuf::from) {
            collect_heroic_save_roots(&home, &mut candidates, &mut seen);

            if let Some(wineprefix) = env::var_os("WINEPREFIX").filter(|v| !v.is_empty()) {
                collect_wine_prefix_save_roots(&PathBuf::from(wineprefix), &mut candidates, &mut seen);
            }
            collect_wine_prefix_save_roots(&home.join(".wine"), &mut candidates, &mut seen);

            let xdg_config = env::var_os("XDG_CONFIG_HOME")
                .filter(|v| !v.is_empty())
                .map(PathBuf::from)
                .unwrap_or_else(|| home.join(".config"));
            push_unique_existing_dir(
                &mut candidates,
                &mut seen,
                xdg_config
                    .join("Epic")
                    .join("FactoryGame")
                    .join("Saved")
                    .join("SaveGames"),
            );
        }

        if candidates.is_empty() {
            return Err(
                "Could not locate Satisfactory SaveGames directory in Steam Proton (AppID 526870), \
                Heroic, Wine prefixes, or ~/.config/Epic. Set SATISFACTORY_SAVE_DIR to override."
                    .to_string(),
            );
        }

        let mut best: Option<(SystemTime, PathBuf)> = None;
        for root in &candidates {
            if let Some(mtime) = newest_sav_mtime_in_save_root(root) {
                if best.as_ref().map_or(true, |(t, _)| mtime > *t) {
                    best = Some((mtime, root.clone()));
                }
            }
        }

        Ok(best
            .map(|(_, p)| p)
            .unwrap_or_else(|| candidates.into_iter().next().unwrap()))
    }
}

/// Locates the active Satisfactory SaveGames profile folder where `.sav` files live.
pub fn find_satisfactory_save_profile_dir() -> Result<PathBuf, String> {
    if let Some(override_val) = env::var_os("SATISFACTORY_SAVE_DIR").filter(|v| !v.is_empty()) {
        let override_path = PathBuf::from(override_val);
        if !override_path.is_dir() {
            return Err(format!(
                "SATISFACTORY_SAVE_DIR does not exist or is not a directory: {:?}",
                override_path
            ));
        }
        if dir_contains_sav_files(&override_path) {
            return Ok(override_path);
        }
        return select_profile_dir_in_root(&override_path).or(Ok(override_path));
    }

    let root = find_satisfactory_save_games_root()?;
    select_profile_dir_in_root(&root)
}

/// Locates the Satisfactory Blueprints directory for `session_name`.
pub fn find_satisfactory_blueprints_dir(session_name: &str) -> Result<PathBuf, String> {
    if let Some(override_val) = env::var_os("SATISFACTORY_SAVE_DIR").filter(|v| !v.is_empty()) {
        let override_path = PathBuf::from(override_val);
        if override_path.is_dir() {
            if let Some(parent) = override_path.parent() {
                if parent
                    .file_name()
                    .and_then(|n| n.to_str())
                    .map_or(false, |n| n.eq_ignore_ascii_case("SaveGames"))
                {
                    return Ok(parent.join("blueprints").join(session_name));
                }
            }
            if dir_contains_sav_files(&override_path) {
                return Ok(override_path.join("blueprints").join(session_name));
            }
        }
    }

    let root = find_satisfactory_save_games_root()?;
    Ok(root.join("blueprints").join(session_name))
}

/// Locates the Satisfactory `FactoryGame/Mods` directory automatically.
pub fn find_satisfactory_mods_dir() -> Option<PathBuf> {
    if let Some(override_val) = env::var_os("SATISFACTORY_MODS_DIR").filter(|v| !v.is_empty()) {
        let p = PathBuf::from(override_val);
        if p.is_dir() {
            return Some(p);
        }
    }

    for lib in discover_steam_libraries() {
        for folder_name in ["Satisfactory", "SatisfactoryExperimental"] {
            let candidate = lib
                .join("steamapps")
                .join("common")
                .join(folder_name)
                .join("FactoryGame")
                .join("Mods");
            if candidate.is_dir() {
                return Some(candidate);
            }
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        if let Some(home) = env::var_os("HOME").map(PathBuf::from) {
            let linux_candidates = [
                home.join("Games")
                    .join("Heroic")
                    .join("Satisfactory")
                    .join("FactoryGame")
                    .join("Mods"),
                home.join("Games")
                    .join("Heroic")
                    .join("SatisfactoryEarlyAccess")
                    .join("FactoryGame")
                    .join("Mods"),
                home.join("Games")
                    .join("Heroic")
                    .join("SatisfactoryExperimental")
                    .join("FactoryGame")
                    .join("Mods"),
                home.join("Games")
                    .join("satisfactory")
                    .join("FactoryGame")
                    .join("Mods"),
            ];
            for candidate in linux_candidates {
                if candidate.is_dir() {
                    return Some(candidate);
                }
            }
        }
    }

    #[cfg(target_os = "windows")]
    {
        let win_candidates = [
            PathBuf::from(r"C:\Program Files (x86)\Steam\steamapps\common\Satisfactory\FactoryGame\Mods"),
            PathBuf::from(r"C:\Program Files\Epic Games\SatisfactoryEarlyAccess\FactoryGame\Mods"),
            PathBuf::from(r"C:\Program Files\Epic Games\Satisfactory\FactoryGame\Mods"),
        ];
        for candidate in win_candidates {
            if candidate.is_dir() {
                return Some(candidate);
            }
        }
    }

    None
}

/// Resolves the workspace/storage directory for uploaded or sample `.sav` files.
pub fn resolve_samples_dir() -> PathBuf {
    let cwd_samples = Path::new("samples");
    if cwd_samples.is_dir() {
        return cwd_samples.to_path_buf();
    }

    if let Ok(exe_path) = env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            let exe_samples = exe_dir.join("samples");
            if exe_samples.is_dir() {
                return exe_samples;
            }
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        let data_home = env::var_os("XDG_DATA_HOME")
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .or_else(|| {
                env::var_os("HOME")
                    .filter(|v| !v.is_empty())
                    .map(|h| PathBuf::from(h).join(".local").join("share"))
            });
        if let Some(dir) = data_home {
            return dir.join("sat-rwg-map").join("samples");
        }
    }

    PathBuf::from("samples")
}

/// Parses an entry from `~/.config/user-dirs.dirs` (e.g. `XDG_DOWNLOAD_DIR="$HOME/Downloads"`).
pub fn parse_xdg_user_dir_value(content: &str, key: &str, home: &Path) -> Option<PathBuf> {
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let Some((lhs, rhs)) = trimmed.split_once('=') else {
            continue;
        };
        if lhs.trim() != key {
            continue;
        }
        let mut raw_val = rhs.trim();
        if raw_val.starts_with('"') && raw_val.ends_with('"') && raw_val.len() >= 2 {
            raw_val = &raw_val[1..raw_val.len() - 1];
        } else if raw_val.starts_with('\'') && raw_val.ends_with('\'') && raw_val.len() >= 2 {
            raw_val = &raw_val[1..raw_val.len() - 1];
        }
        if raw_val.is_empty() {
            return None;
        }
        if raw_val == "$HOME" {
            return Some(home.to_path_buf());
        }
        if let Some(rest) = raw_val.strip_prefix("$HOME/") {
            return Some(home.join(rest));
        }
        if let Some(rest) = raw_val.strip_prefix("${HOME}/") {
            return Some(home.join(rest));
        }
        return Some(PathBuf::from(raw_val));
    }
    None
}

/// Resolves the user's Downloads directory across Linux (XDG) and Windows.
pub fn resolve_downloads_dir() -> PathBuf {
    #[cfg(not(target_os = "windows"))]
    {
        if let Some(home) = env::var_os("HOME").filter(|v| !v.is_empty()).map(PathBuf::from) {
            let xdg_config = env::var_os("XDG_CONFIG_HOME")
                .filter(|v| !v.is_empty())
                .map(PathBuf::from)
                .unwrap_or_else(|| home.join(".config"));
            let user_dirs_file = xdg_config.join("user-dirs.dirs");
            if let Ok(content) = fs::read_to_string(&user_dirs_file) {
                if let Some(dir) = parse_xdg_user_dir_value(&content, "XDG_DOWNLOAD_DIR", &home) {
                    if dir.is_dir() || dir.parent().map_or(false, |p| p.is_dir()) {
                        return dir;
                    }
                }
            }
            let downloads = home.join("Downloads");
            if downloads.is_dir() {
                return downloads;
            }
            if home.is_dir() {
                return home;
            }
        }
        PathBuf::from(".")
    }

    #[cfg(target_os = "windows")]
    {
        if let Some(profile) = env::var_os("USERPROFILE")
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
        {
            let downloads = profile.join("Downloads");
            if downloads.is_dir() || profile.is_dir() {
                return downloads;
            }
        }
        PathBuf::from(".")
    }
}

/// Strips both Unix (`/`) and Windows (`\`) directory prefixes from an uploaded filename
/// and rejects empty / traversal segments.
pub fn sanitize_upload_filename(raw: &str, fallback: &str) -> String {
    let trimmed = raw.trim();
    let leaf = trimmed
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or("")
        .trim();
    if leaf.is_empty() || leaf == "." || leaf == ".." {
        fallback.to_string()
    } else {
        leaf.to_string()
    }
}

/// Opens `url` in the system default web browser.
pub fn open_url_in_browser(url: &str) -> Result<()> {
    #[cfg(target_os = "linux")]
    {
        Command::new("xdg-open")
            .arg(url)
            .spawn()
            .with_context(|| format!("Failed to launch xdg-open for {}", url))?;
        return Ok(());
    }

    #[cfg(target_os = "macos")]
    {
        Command::new("open")
            .arg(url)
            .spawn()
            .with_context(|| format!("Failed to launch open for {}", url))?;
        return Ok(());
    }

    #[cfg(target_os = "windows")]
    {
        Command::new("cmd")
            .args(["/C", "start", "", url])
            .spawn()
            .with_context(|| format!("Failed to launch browser for {}", url))?;
        return Ok(());
    }
}

/// Applies safe WebKitGTK environment defaults on Linux before any threads or windows start.
#[cfg(target_os = "linux")]
pub fn configure_linux_webview_env() {
    if env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_none()
        && (Path::new("/proc/driver/nvidia/version").exists() || Path::new("/dev/nvidia0").exists())
    {
        // Work around WebKitGTK DMA-BUF / GBM blank-window and crash issues on NVIDIA drivers
        // while keeping WebGL functional.
        env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
    }
}

/// Returns true if an X11 or Wayland display environment variable is present on Linux.
#[cfg(target_os = "linux")]
pub fn has_linux_display() -> bool {
    env::var_os("WAYLAND_DISPLAY").is_some() || env::var_os("DISPLAY").is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_parse_steam_library_folders_vdf() {
        let sample_linux = r#"
"libraryfolders"
{
	"0"
	{
		"path"		"/home/alice/.local/share/Steam"
		"label"		""
	}
	"1"
	{
		"path"		"/mnt/Games/SteamLibrary"
		"apps"
		{
			"526870"		"30444302660"
		}
	}
}
"#;
        let parsed = parse_steam_library_folders_vdf(sample_linux);
        assert_eq!(
            parsed,
            vec![
                PathBuf::from("/home/alice/.local/share/Steam"),
                PathBuf::from("/mnt/Games/SteamLibrary"),
            ]
        );

        let sample_windows = r#"
"libraryfolders"
{
	"0"
	{
		"path"		"C:\\Program Files (x86)\\Steam"
	}
	"1"
	{
		"path"		"D:\\Games\\SteamLibrary"
	}
}
"#;
        let parsed_win = parse_steam_library_folders_vdf(sample_windows);
        assert_eq!(
            parsed_win,
            vec![
                PathBuf::from(r"C:\Program Files (x86)\Steam"),
                PathBuf::from(r"D:\Games\SteamLibrary"),
            ]
        );
    }

    #[test]
    fn test_parse_xdg_user_dir_value() {
        let content = r#"
# This file is written by xdg-user-dirs-update
XDG_DESKTOP_DIR="$HOME/Desktop"
XDG_DOWNLOAD_DIR="$HOME/My Downloads"
XDG_TEMPLATES_DIR="/mnt/data/Templates"
"#;
        let home = Path::new("/home/pioneer");
        assert_eq!(
            parse_xdg_user_dir_value(content, "XDG_DOWNLOAD_DIR", home),
            Some(PathBuf::from("/home/pioneer/My Downloads"))
        );
        assert_eq!(
            parse_xdg_user_dir_value(content, "XDG_TEMPLATES_DIR", home),
            Some(PathBuf::from("/mnt/data/Templates"))
        );
        assert_eq!(
            parse_xdg_user_dir_value(content, "XDG_PICTURES_DIR", home),
            None
        );
    }

    #[test]
    fn test_sanitize_upload_filename() {
        assert_eq!(
            sanitize_upload_filename("/tmp/test/world_1.sav", "fallback.sav"),
            "world_1.sav"
        );
        assert_eq!(
            sanitize_upload_filename(r"C:\fakepath\My Save.sav", "fallback.sav"),
            "My Save.sav"
        );
        assert_eq!(sanitize_upload_filename("..", "fallback.sav"), "fallback.sav");
        assert_eq!(sanitize_upload_filename(".", "fallback.sav"), "fallback.sav");
        assert_eq!(sanitize_upload_filename("   ", "fallback.sav"), "fallback.sav");
    }

    #[test]
    fn test_select_profile_dir_in_root_skips_server_and_blueprints_and_picks_newest() {
        let base = env::temp_dir().join(format!(
            "sat_rwg_map_platform_test_{}_{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        let server_dir = base.join("server");
        let blueprints_dir = base.join("Blueprints");
        let older_profile = base.join("11111111111111111");
        let newer_profile = base.join("76561198020625428");

        fs::create_dir_all(&server_dir).unwrap();
        fs::create_dir_all(&blueprints_dir).unwrap();
        fs::create_dir_all(&older_profile).unwrap();
        fs::create_dir_all(&newer_profile).unwrap();

        fs::write(server_dir.join("dedicated.sav"), b"ignored").unwrap();
        fs::write(blueprints_dir.join("bp.sav"), b"ignored").unwrap();
        fs::write(older_profile.join("old.sav"), b"old").unwrap();
        std::thread::sleep(Duration::from_millis(30));
        fs::write(newer_profile.join("new.sav"), b"new").unwrap();

        let selected = select_profile_dir_in_root(&base).unwrap();
        assert_eq!(selected, newer_profile);

        let _ = fs::remove_dir_all(&base);
    }
}
