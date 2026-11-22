use std::fs::{self, File};
use std::io::{BufReader, Read, Write};
use std::path::{Path, PathBuf};

use studynodes_graph::{ProjectBundle, StudyGraph};
use zip::write::SimpleFileOptions;

fn sanitize_folder_slug(title: &str) -> String {
    let raw: String = title
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect();
    let trimmed = raw.trim_matches('_');
    if trimmed.is_empty() {
        "imported_project".to_string()
    } else {
        trimmed.chars().take(48).collect()
    }
}

fn add_dir_entries_to_zip<W: Write + std::io::Seek>(
    zw: &mut zip::ZipWriter<W>,
    disk_dir: &Path,
    zip_prefix: &str,
    opts: SimpleFileOptions,
) -> Result<(), String> {
    if !disk_dir.exists() {
        return Ok(());
    }
    let entries = fs::read_dir(disk_dir).map_err(|e| e.to_string())?;
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        let entry_name = format!("{}/{}", zip_prefix, name);
        if path.is_file() {
            let bytes = fs::read(&path).map_err(|e| e.to_string())?;
            zw.start_file(&entry_name, opts).map_err(|e| e.to_string())?;
            zw.write_all(&bytes).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

/// Export a self-contained `.studynodes` (ZIP) bundle containing `project.json`,
/// content-addressed `assets/`, and `cache/extracted/`.
pub fn export_project_archive(
    bundle: &ProjectBundle,
    graph: &StudyGraph,
    dest_zip_path: &Path,
) -> Result<(), String> {
    if let Some(parent) = dest_zip_path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
    }
    let file = File::create(dest_zip_path).map_err(|e| e.to_string())?;
    let mut zw = zip::ZipWriter::new(file);
    let opts = SimpleFileOptions::default();

    let project_json = serde_json::to_vec_pretty(graph).map_err(|e| e.to_string())?;
    zw.start_file(ProjectBundle::PROJECT_FILE, opts)
        .map_err(|e| e.to_string())?;
    zw.write_all(&project_json).map_err(|e| e.to_string())?;

    add_dir_entries_to_zip(
        &mut zw,
        &bundle.root_dir.join(ProjectBundle::ASSETS_DIR),
        ProjectBundle::ASSETS_DIR,
        opts,
    )?;
    add_dir_entries_to_zip(
        &mut zw,
        &bundle.root_dir.join(ProjectBundle::EXTRACTED_DIR),
        ProjectBundle::EXTRACTED_DIR,
        opts,
    )?;

    zw.finish().map_err(|e| e.to_string())?;
    Ok(())
}

/// Import a `.studynodes` (or `.zip`) project archive into `projects_dir`.
/// Validates archive paths against Zip Slip traversal and guarantees unique folder + IDs.
pub fn import_project_archive(
    archive_path: &Path,
    projects_dir: &Path,
    collection_override: Option<String>,
) -> Result<PathBuf, String> {
    fs::create_dir_all(projects_dir).map_err(|e| e.to_string())?;
    let file = File::open(archive_path)
        .map_err(|e| format!("Failed to open archive '{}': {}", archive_path.display(), e))?;
    let mut archive = zip::ZipArchive::new(BufReader::new(file))
        .map_err(|e| format!("Invalid .studynodes archive: {}", e))?;

    // 1. Validate entries and load project.json
    let mut project_json_bytes: Option<Vec<u8>> = None;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| e.to_string())?;
        let Some(rel_path) = entry.enclosed_name().map(|p| p.to_path_buf()) else {
            return Err("Rejected archive with unsafe entry path".into());
        };
        if entry.is_dir() {
            continue;
        }
        let rel_norm = rel_path.to_string_lossy().replace('\\', "/");
        if rel_norm == ProjectBundle::PROJECT_FILE {
            let mut buf = Vec::new();
            entry.read_to_end(&mut buf).map_err(|e| e.to_string())?;
            project_json_bytes = Some(buf);
        }
    }

    let raw_json = project_json_bytes
        .ok_or_else(|| format!("Archive does not contain {}", ProjectBundle::PROJECT_FILE))?;
    let mut graph: StudyGraph =
        serde_json::from_slice(&raw_json).map_err(|e| format!("Invalid project.json: {}", e))?;
    if graph.schema_version > StudyGraph::CURRENT_SCHEMA_VERSION {
        return Err(format!(
            "Unsupported project schema version {}",
            graph.schema_version
        ));
    }

    // Check if an existing workspace in projects_dir already uses this project_id
    let mut id_collision = false;
    if let Ok(entries) = fs::read_dir(projects_dir) {
        for entry in entries.flatten() {
            let pjson = entry.path().join(ProjectBundle::PROJECT_FILE);
            if pjson.is_file() {
                if let Ok(existing_raw) = fs::read_to_string(&pjson) {
                    if let Ok(existing_g) = serde_json::from_str::<StudyGraph>(&existing_raw) {
                        if existing_g.project_id == graph.project_id {
                            id_collision = true;
                            break;
                        }
                    }
                }
            }
        }
    }
    if id_collision {
        graph.project_id = uuid::Uuid::new_v4().to_string();
    }

    if let Some(col_name) = collection_override {
        let trimmed = col_name.trim().to_string();
        graph.collection = if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        };
    }

    // Create a unique destination folder
    let base_slug = sanitize_folder_slug(&graph.title);
    let mut folder_name = base_slug.clone();
    let mut counter = 2usize;
    while projects_dir.join(&folder_name).exists() {
        folder_name = format!("{}_{}", base_slug, counter);
        counter += 1;
    }

    let dest_root = projects_dir.join(&folder_name);
    fs::create_dir_all(dest_root.join(ProjectBundle::ASSETS_DIR)).map_err(|e| e.to_string())?;
    fs::create_dir_all(dest_root.join(ProjectBundle::EXTRACTED_DIR))
        .map_err(|e| e.to_string())?;

    // Extract assets/ and cache/extracted/
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| e.to_string())?;
        let Some(rel_path) = entry.enclosed_name().map(|p| p.to_path_buf()) else {
            return Err("Rejected archive with unsafe entry path".into());
        };
        if entry.is_dir() {
            continue;
        }
        let rel_norm = rel_path.to_string_lossy().replace('\\', "/");
        if rel_norm.starts_with("assets/") || rel_norm.starts_with("cache/extracted/") {
            let target = dest_root.join(&rel_path);
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            let mut out_file = File::create(&target).map_err(|e| e.to_string())?;
            std::io::copy(&mut entry, &mut out_file).map_err(|e| e.to_string())?;
        }
    }

    let bundle = ProjectBundle {
        root_dir: dest_root.clone(),
    };
    bundle
        .save_graph_atomic(&graph)
        .map_err(|e| e.to_string())?;

    Ok(dest_root)
}

#[cfg(test)]
mod tests {
    use super::*;
    use studynodes_graph::coords::WorldPos;
    use studynodes_graph::model::{Node, NodeKind};
    use tempfile::tempdir;

    #[test]
    fn test_export_and_import_project_archive_roundtrip() {
        let dir = tempdir().unwrap();
        let projects_dir = dir.path().join("projects");
        fs::create_dir_all(&projects_dir).unwrap();

        let (bundle, mut graph) =
            ProjectBundle::create(&projects_dir, "neuro_export", "Neuro Export").unwrap();
        graph.collection = Some("Biology".into());
        let sample_asset = dir.path().join("snippet.txt");
        fs::write(&sample_asset, b"hello asset bytes").unwrap();
        let (hash, _) = bundle.ingest_asset(&sample_asset).unwrap();
        let mut n = Node::new(NodeKind::Document, "Doc 1", WorldPos::new(10.0, 20.0));
        n.asset_hash = Some(hash.clone());
        graph.add_node(n);
        bundle.save_graph_atomic(&graph).unwrap();

        let archive_path = dir.path().join("neuro_export.studynodes");
        export_project_archive(&bundle, &graph, &archive_path).unwrap();
        assert!(archive_path.exists());

        let imported_dir =
            import_project_archive(&archive_path, &projects_dir, Some("Imported Bio".into()))
                .unwrap();
        assert_ne!(imported_dir, bundle.root_dir);
        let (imp_bundle, imp_graph) = ProjectBundle::open(&imported_dir).unwrap();
        assert_eq!(imp_graph.title, "Neuro Export");
        assert_eq!(imp_graph.collection.as_deref(), Some("Imported Bio"));
        assert_eq!(imp_graph.nodes.len(), 1);
        assert!(imp_bundle.find_asset_path(&hash).is_some());
    }
}
