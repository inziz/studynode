#![allow(unsafe_code)]

//! Platform-aware user directory resolution (`Documents/StudyNodes`) and safe first-run
//! migration from legacy working-directory `./study_projects`.

use std::path::{Path, PathBuf};

#[cfg(target_os = "windows")]
pub fn user_documents_dir() -> Option<PathBuf> {
    use windows::Win32::System::Com::CoTaskMemFree;
    use windows::Win32::UI::Shell::{FOLDERID_Documents, KF_FLAG_DEFAULT, SHGetKnownFolderPath};

    unsafe {
        if let Ok(pwstr) = SHGetKnownFolderPath(&FOLDERID_Documents, KF_FLAG_DEFAULT, None) {
            let s = pwstr.to_string().ok();
            CoTaskMemFree(Some(pwstr.0 as *const _));
            if let Some(dir) = s.filter(|p| !p.trim().is_empty()) {
                return Some(PathBuf::from(dir));
            }
        }
    }

    std::env::var_os("USERPROFILE").map(|home| PathBuf::from(home).join("Documents"))
}

#[cfg(not(target_os = "windows"))]
pub fn user_documents_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|home| PathBuf::from(home).join("Documents"))
}

/// Default persistent directory for StudyNodes project bundles and preferences:
/// `<Documents>/StudyNodes` (falling back to `./study_projects` if Documents is unavailable).
pub fn default_projects_dir() -> PathBuf {
    let target = user_documents_dir()
        .map(|docs| docs.join("StudyNodes"))
        .unwrap_or_else(|| {
            std::env::current_dir()
                .unwrap_or_else(|_| PathBuf::from("."))
                .join("study_projects")
        });
    let _ = std::fs::create_dir_all(&target);

    if !cfg!(test) {
        if let Ok(cwd) = std::env::current_dir() {
            let legacy = cwd.join("study_projects");
            let _ = migrate_legacy_projects_if_needed(&target, &legacy);
        }
    }

    target
}

fn dir_has_existing_studynodes_data(dir: &Path) -> bool {
    if dir.join("reader_prefs.json").is_file() {
        return true;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return false;
    };
    entries.flatten().any(|entry| {
        let p = entry.path();
        p.is_dir() && p.extension().is_some_and(|e| e == "studynodes")
    })
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        if src_path.is_dir() {
            copy_dir_recursive(&src_path, &dst_path)?;
        } else if src_path.is_file() && !dst_path.exists() {
            std::fs::copy(&src_path, &dst_path)?;
        }
    }
    Ok(())
}

/// Non-destructively copy `.studynodes` project bundles and `reader_prefs.json` from a legacy
/// `./study_projects` directory into `target_dir` on first launch when `target_dir` has no data.
pub fn migrate_legacy_projects_if_needed(target_dir: &Path, legacy_dir: &Path) -> usize {
    if target_dir == legacy_dir || !legacy_dir.is_dir() {
        return 0;
    }
    let _ = std::fs::create_dir_all(target_dir);
    if dir_has_existing_studynodes_data(target_dir) {
        return 0;
    }

    let mut migrated = 0usize;
    let legacy_prefs = legacy_dir.join("reader_prefs.json");
    let target_prefs = target_dir.join("reader_prefs.json");
    if legacy_prefs.is_file() && !target_prefs.exists() && std::fs::copy(&legacy_prefs, &target_prefs).is_ok() {
        migrated += 1;
    }

    if let Ok(entries) = std::fs::read_dir(legacy_dir) {
        for entry in entries.flatten() {
            let src = entry.path();
            if src.is_dir() && src.extension().is_some_and(|e| e == "studynodes") {
                let dst = target_dir.join(entry.file_name());
                if !dst.exists() && copy_dir_recursive(&src, &dst).is_ok() {
                    migrated += 1;
                }
            }
        }
    }

    migrated
}
