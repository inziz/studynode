use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::graph::StudyGraph;

#[derive(Error, Debug)]
pub enum BundleError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Serialization error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Invalid bundle structure: {0}")]
    InvalidBundle(String),
    #[error("Unsupported schema version: {0}. Please update StudyNodes.")]
    UnsupportedSchemaVersion(u32),
}

/// Manages a self-contained study project bundle directory on disk.
#[derive(Clone, Debug)]
pub struct ProjectBundle {
    pub root_dir: PathBuf,
}

impl ProjectBundle {
    pub const PROJECT_FILE: &'static str = "project.json";
    pub const ASSETS_DIR: &'static str = "assets";
    pub const CACHE_DIR: &'static str = "cache";
    pub const EXTRACTED_DIR: &'static str = "cache/extracted";

    /// Initialize a new project bundle on disk with the required directory structure.
    pub fn create(parent_dir: &Path, folder_name: &str, title: &str) -> Result<(Self, StudyGraph), BundleError> {
        let root_dir = parent_dir.join(folder_name);
        fs::create_dir_all(&root_dir)?;
        fs::create_dir_all(root_dir.join(Self::ASSETS_DIR))?;
        fs::create_dir_all(root_dir.join(Self::CACHE_DIR))?;
        fs::create_dir_all(root_dir.join(Self::EXTRACTED_DIR))?;

        let graph = StudyGraph::new(title);
        let bundle = Self { root_dir };
        bundle.save_graph_atomic(&graph)?;

        Ok((bundle, graph))
    }

    /// Open an existing project bundle from its root directory.
    pub fn open(root_dir: impl Into<PathBuf>) -> Result<(Self, StudyGraph), BundleError> {
        let root_dir = root_dir.into();
        let project_file = root_dir.join(Self::PROJECT_FILE);

        if !project_file.exists() {
            return Err(BundleError::InvalidBundle(format!(
                "Missing {} in {}",
                Self::PROJECT_FILE,
                root_dir.display()
            )));
        }

        let mut file = File::open(&project_file)?;
        let mut content = String::new();
        file.read_to_string(&mut content)?;

        let graph: StudyGraph = serde_json::from_str(&content)?;

        // Schema migration guard
        if graph.schema_version > StudyGraph::CURRENT_SCHEMA_VERSION {
            return Err(BundleError::UnsupportedSchemaVersion(graph.schema_version));
        }

        // Ensure subdirectories exist
        let _ = fs::create_dir_all(root_dir.join(Self::ASSETS_DIR));
        let _ = fs::create_dir_all(root_dir.join(Self::EXTRACTED_DIR));

        Ok((Self { root_dir }, graph))
    }

    /// Atomically save the graph to `project.json` using a temporary file and atomic rename.
    pub fn save_graph_atomic(&self, graph: &StudyGraph) -> Result<(), BundleError> {
        let target_path = self.root_dir.join(Self::PROJECT_FILE);
        let tmp_path = self.root_dir.join(format!("{}.tmp", Self::PROJECT_FILE));

        let json = serde_json::to_string_pretty(graph)?;
        {
            let mut tmp_file = File::create(&tmp_path)?;
            tmp_file.write_all(json.as_bytes())?;
            tmp_file.flush()?;
            tmp_file.sync_all()?;
        }

        // Atomic rename (replaces existing file safely)
        fs::rename(tmp_path, target_path)?;
        Ok(())
    }

    /// Ingest an external file into the `assets/` directory using SHA-256 content addressing.
    /// Returns the SHA-256 hash string and the destination path within `assets/`.
    pub fn ingest_asset(&self, source_path: &Path) -> Result<(String, PathBuf), BundleError> {
        let mut source_file = File::open(source_path)?;
        let mut buffer = Vec::new();
        source_file.read_to_end(&mut buffer)?;

        let mut hasher = Sha256::new();
        hasher.update(&buffer);
        let hash = format!("{:x}", hasher.finalize());

        let ext = source_path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("bin");

        let asset_filename = format!("{}.{}", hash, ext);
        let dest_path = self.root_dir.join(Self::ASSETS_DIR).join(&asset_filename);

        if !dest_path.exists() {
            let mut dest_file = File::create(&dest_path)?;
            dest_file.write_all(&buffer)?;
            dest_file.flush()?;
        }

        Ok((hash, dest_path))
    }

    /// Prune unreferenced SHA-256 blobs from the `assets/` directory that are no longer used by any node.
    /// Returns the number of pruned orphan files.
    pub fn prune_orphan_assets(&self, graph: &StudyGraph) -> Result<usize, BundleError> {
        let assets_dir = self.root_dir.join(Self::ASSETS_DIR);
        if !assets_dir.exists() {
            return Ok(0);
        }

        // Collect all active asset hashes referenced by nodes
        let active_hashes: HashSet<&str> = graph
            .nodes
            .values()
            .filter_map(|n| n.asset_hash.as_deref())
            .collect();

        let mut pruned = 0;
        for entry in fs::read_dir(&assets_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_file() {
                if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                    if !active_hashes.contains(stem) {
                        let _ = fs::remove_file(&path);
                        pruned += 1;
                    }
                }
            }
        }

        Ok(pruned)
    }

    /// Save extracted plain text/markdown for a document hash into `cache/extracted/<hash>.md`.
    pub fn save_extracted_text(&self, asset_hash: &str, text: &str) -> Result<PathBuf, BundleError> {
        let dest_path = self
            .root_dir
            .join(Self::EXTRACTED_DIR)
            .join(format!("{}.md", asset_hash));
        let mut file = File::create(&dest_path)?;
        file.write_all(text.as_bytes())?;
        file.flush()?;
        Ok(dest_path)
    }

    /// Read extracted text/markdown if cached.
    pub fn read_extracted_text(&self, asset_hash: &str) -> Option<String> {
        let path = self
            .root_dir
            .join(Self::EXTRACTED_DIR)
            .join(format!("{}.md", asset_hash));
        fs::read_to_string(path).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_bundle_creation_and_atomic_save() {
        let dir = tempdir().unwrap();
        let (bundle, mut graph) =
            ProjectBundle::create(dir.path(), "my_test.studynodes", "Neuroscience 101").unwrap();

        assert_eq!(graph.title, "Neuroscience 101");
        assert!(bundle.root_dir.join(ProjectBundle::PROJECT_FILE).exists());

        // Modify and save
        graph.title = "Neuroscience Advanced".into();
        bundle.save_graph_atomic(&graph).unwrap();

        // Reopen and verify
        let (_, reopened) = ProjectBundle::open(&bundle.root_dir).unwrap();
        assert_eq!(reopened.title, "Neuroscience Advanced");
    }

    #[test]
    fn test_schema_version_guard() {
        let dir = tempdir().unwrap();
        let (bundle, mut graph) =
            ProjectBundle::create(dir.path(), "schema_test.studynodes", "Future Schema").unwrap();

        // Simulate future schema version
        graph.schema_version = 999;
        bundle.save_graph_atomic(&graph).unwrap();

        let open_res = ProjectBundle::open(&bundle.root_dir);
        assert!(matches!(open_res, Err(BundleError::UnsupportedSchemaVersion(999))));
    }

    #[test]
    fn test_orphan_asset_pruning() {
        let dir = tempdir().unwrap();
        let (bundle, graph) =
            ProjectBundle::create(dir.path(), "orphan_test.studynodes", "Prune Test").unwrap();

        // Ingest an asset that won't be attached to any node
        let dummy_path = dir.path().join("orphan.txt");
        fs::write(&dummy_path, "Unreferenced content").unwrap();
        let (_hash, dest) = bundle.ingest_asset(&dummy_path).unwrap();
        assert!(dest.exists());

        let pruned_count = bundle.prune_orphan_assets(&graph).unwrap();
        assert_eq!(pruned_count, 1);
        assert!(!dest.exists(), "Orphan asset must be pruned from disk");
    }
}
