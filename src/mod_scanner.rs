//! Scans a Satisfactory Mods directory (`.uplugin` folders, `.uasset`, `.pak`, `.utoc`)
//! for item descriptors (`Desc_*.uasset`, `BP_EquipmentDescriptor*.uasset`, etc.)
//! without requiring any external agent or Unreal Engine runtime.

use crate::item_registry::{lookup_item_metadata, KnownItemEntry};
use anyhow::{bail, Result};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

/// Scans `mods_dir` recursively for mod item descriptors and returns deduplicated entries.
pub fn scan_mods_folder(mods_dir: &Path) -> Result<Vec<KnownItemEntry>> {
    if !mods_dir.exists() || !mods_dir.is_dir() {
        bail!("Mods folder does not exist or is not a directory: {}", mods_dir.display());
    }

    let mut found: HashMap<String, KnownItemEntry> = HashMap::new();
    let mut mod_roots = Vec::new();

    // Each immediate subdirectory under Mods/ is typically a mod plugin folder (e.g. KLib, RefinedPower)
    if let Ok(entries) = fs::read_dir(mods_dir) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() {
                let mod_name = p
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("Modded")
                    .to_string();
                mod_roots.push((mod_name, p));
            }
        }
    }

    if mod_roots.is_empty() {
        let fallback_name = mods_dir
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("Modded")
            .to_string();
        mod_roots.push((fallback_name, mods_dir.to_path_buf()));
    }

    for (mod_name, root) in mod_roots {
        let mut stack: Vec<PathBuf> = vec![root.clone()];
        while let Some(dir) = stack.pop() {
            let Ok(rd) = fs::read_dir(&dir) else {
                continue;
            };
            for entry in rd.flatten() {
                let p = entry.path();
                if p.is_dir() {
                    stack.push(p);
                    continue;
                }
                let Some(ext) = p.extension().and_then(|e| e.to_str()) else {
                    continue;
                };
                match ext.to_ascii_lowercase().as_str() {
                    "uasset" => {
                        if let Some(item_path) = uasset_file_to_item_path(&mod_name, &root, &p) {
                            insert_item(&mut found, item_path);
                        }
                    }
                    "pak" | "utoc" => {
                        scan_binary_package_for_items(&mod_name, &p, &mut found);
                    }
                    _ => {}
                }
            }
        }
    }

    let mut items: Vec<KnownItemEntry> = found.into_values().collect();
    items.sort_by(|a, b| {
        a.mod_source
            .cmp(&b.mod_source)
            .then_with(|| a.display_name.cmp(&b.display_name))
    });
    Ok(items)
}

fn is_item_descriptor_stem(stem: &str) -> bool {
    (stem.starts_with("Desc_") || stem.starts_with("BP_EquipmentDescriptor") || stem.starts_with("Item_"))
        && !stem.contains("Build_")
        && !stem.ends_with("_Recipe")
        && !stem.starts_with("Recipe_")
}

fn uasset_file_to_item_path(mod_name: &str, mod_root: &Path, file_path: &Path) -> Option<String> {
    let stem = file_path.file_stem()?.to_str()?;
    if !is_item_descriptor_stem(stem) {
        return None;
    }
    let rel = file_path.strip_prefix(mod_root).ok()?;
    let rel_str = rel.to_string_lossy().replace('\\', "/");
    let without_ext = rel_str.strip_suffix(".uasset").unwrap_or(&rel_str);
    let content_rel = without_ext
        .strip_prefix("Content/")
        .unwrap_or(without_ext);
    Some(format!("/{}/{}.{}_C", mod_name, content_rel, stem))
}

/// Scans `.pak` / `.utoc` bytes for ASCII/UTF-8 paths or descriptor names (`Desc_*.uasset` / `Desc_*`).
fn scan_binary_package_for_items(
    mod_name: &str,
    pkg_path: &Path,
    found: &mut HashMap<String, KnownItemEntry>,
) {
    let Ok(bytes) = fs::read(pkg_path) else {
        return;
    };
    let len = bytes.len();
    let mut i = 0usize;
    while i + 8 < len {
        // Quick scan for "Desc_" or "BP_EquipmentDescriptor"
        let is_desc = &bytes[i..i + 5] == b"Desc_";
        let is_equip = i + 22 < len && &bytes[i..i + 22] == b"BP_EquipmentDescriptor";
        if is_desc || is_equip {
            // Walk backwards up to 160 bytes to capture a slash-delimited relative path if present
            let mut start = i;
            while start > 0 && i - start < 160 {
                let b = bytes[start - 1];
                if b.is_ascii_alphanumeric() || matches!(b, b'/' | b'_' | b'-' | b'.') {
                    start -= 1;
                } else {
                    break;
                }
            }
            let mut end = i + if is_desc { 5 } else { 22 };
            while end < len && end - i < 120 {
                let b = bytes[end];
                if b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.') {
                    end += 1;
                } else {
                    break;
                }
            }
            if let Ok(token) = std::str::from_utf8(&bytes[start..end]) {
                let clean = token.trim_matches('/');
                let clean_no_ext = clean
                    .strip_suffix(".uasset")
                    .or_else(|| clean.strip_suffix(".uexp"))
                    .unwrap_or(clean);
                let stem = clean_no_ext.rsplit('/').next().unwrap_or(clean_no_ext);
                let stem_no_c = stem.strip_suffix("_C").unwrap_or(stem);
                if is_item_descriptor_stem(stem_no_c) && stem_no_c.len() >= 6 {
                    let rel = clean_no_ext
                        .strip_prefix("Content/")
                        .unwrap_or(clean_no_ext)
                        .trim_start_matches(&format!("{}/", mod_name));
                    let item_path = if rel.contains('/') {
                        format!("/{}/{}.{}_C", mod_name, rel, stem_no_c)
                    } else {
                        format!("/{}/Items/{}.{}_C", mod_name, stem_no_c, stem_no_c)
                    };
                    insert_item(found, item_path);
                }
            }
            i = end;
        } else {
            i += 1;
        }
    }
}

fn insert_item(found: &mut HashMap<String, KnownItemEntry>, item_path: String) {
    if found.contains_key(&item_path) {
        return;
    }
    let meta = lookup_item_metadata(&item_path);
    found.insert(
        item_path.clone(),
        KnownItemEntry {
            item_path,
            display_name: meta.display_name,
            category: meta.category,
            mod_source: meta.mod_source,
        },
    );
}
