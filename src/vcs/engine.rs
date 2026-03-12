//! Git2-based version control engine
//!
//! Provides a bare git repository for tracking file versions.
//! The repository is stored in the application's data directory
//! and can track files anywhere on the system.
//!
//! # Features
//! - Track arbitrary files on the system (not limited to a workdir)
//! - Create snapshots (commits) of tracked files
//! - Restore files from any snapshot
//! - Search history to restore deleted files
//!
//! # Design
//! File paths are encoded as base64 strings in the git tree,
//! allowing arbitrary paths to be stored without filesystem limitations.
//! The engine is stateless - all information is stored in the git repository.

use {
    crate::config::VcsConfig,
    anyhow::{Context, Result},
    base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD},
    chrono::{DateTime, Utc},
    git2::{
        Commit, ErrorCode, ObjectType, Oid, Repository, Signature, TreeWalkMode, TreeWalkResult,
    },
    serde::{Deserialize, Serialize},
    std::{
        fs,
        path::{Path, PathBuf},
    },
    tracing::{debug, info, warn},
};

/// Snapshot information for display
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotInfo {
    /// Unique snapshot ID (full commit hash)
    pub id: String,
    /// Short ID (first 7 characters of hash)
    pub short_id: String,
    /// Commit message describing the snapshot
    pub message: String,
    /// Creation timestamp (ISO 8601 format)
    pub timestamp: DateTime<Utc>,
    /// Author name
    pub author: String,
    /// Number of files in this snapshot
    pub file_count: usize,
}

/// Detailed snapshot with file information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    /// Snapshot metadata
    pub info: SnapshotInfo,
    /// Files included in this snapshot
    pub files: Vec<TrackedFile>,
}

/// A tracked file in a snapshot
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackedFile {
    /// Original file path (as tracked)
    pub path: String,
    /// File content as UTF-8 string (if decodable)
    pub content: Option<String>,
    /// File size in bytes
    pub size: usize,
    /// Content hash (blob SHA-1)
    pub hash: String,
}

/// File status for tracking
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FileStatus {
    /// File is new (not yet tracked)
    New,
    /// File has been modified since last snapshot
    Modified,
    /// File has been deleted (but was previously tracked)
    Deleted,
    /// File is unchanged
    Unmodified,
}

/// Status information for a tracked file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileStatusInfo {
    /// File path
    pub path: String,
    /// Current status
    pub status: FileStatus,
}

/// Version Control System engine using git2
///
/// Uses a bare git repository to track file versions without requiring
/// a specific working directory structure. Files are stored as blobs
/// with base64-encoded filenames to support arbitrary paths.
///
/// The engine is stateless - all tracking information is stored in the
/// git repository itself. The HEAD commit's tree represents the current
/// tracking state.
pub struct VcsEngine {
    /// Configuration settings
    config: VcsConfig,
    /// Git repository handle
    repo: Repository,
}

impl VcsEngine {
    /// Create a new VCS engine
    ///
    /// # Arguments
    /// * `config` - VCS configuration including database path
    ///
    /// # Returns
    /// A new VcsEngine instance, creating the repository if needed
    pub fn new(config: VcsConfig) -> Result<Self> {
        let db_path = PathBuf::from(&config.db_path);

        // Create directory if needed
        if !db_path.exists() {
            fs::create_dir_all(&db_path)
                .with_context(|| format!("Failed to create VCS directory: {:?}", db_path))?;
        }

        // Open or create repository
        let repo = match Repository::open(&db_path) {
            Ok(r) => {
                info!("Opened existing VCS repository at {:?}", db_path);
                r
            }
            Err(e) if e.code() == ErrorCode::NotFound => {
                // Create a new bare repository
                let repo = Repository::init(&db_path)
                    .with_context(|| format!("Failed to create VCS repository at {:?}", db_path))?;
                info!("Created new VCS repository at {:?}", db_path);

                // Create initial commit
                Self::create_initial_commit(&repo)?;
                repo
            }
            Err(e) => return Err(e).context("Failed to open VCS repository"),
        };

        Ok(Self { config, repo })
    }

    /// Create an initial empty commit
    fn create_initial_commit(repo: &Repository) -> Result<()> {
        let sig = Signature::now("nb-claw", "nb-claw@local")?;
        let tree_id = {
            let mut index = repo.index()?;
            index.write_tree()?
        };
        let tree = repo.find_tree(tree_id)?;

        repo.commit(Some("HEAD"), &sig, &sig, "Initial commit", &tree, &[])?;
        debug!("Created initial commit");
        Ok(())
    }

    /// Create a snapshot (commit) of the specified files
    ///
    /// # Arguments
    /// * `message` - Commit message describing the snapshot
    /// * `paths` - Slice of file paths to include in this snapshot
    ///
    /// # Returns
    /// The commit ID (full hash), or empty string if no changes detected
    pub fn create_snapshot(&self, message: &str, paths: &[&Path]) -> Result<String> {
        if paths.is_empty() {
            debug!("No files to snapshot");
            return Ok(String::new());
        }

        // Create blobs for all files and build tree
        let mut tree_builder = self.repo.treebuilder(None)?;
        let mut file_count = 0;

        for path in paths {
            let canonical_path = path.canonicalize().with_context(|| {
                format!("File not found or cannot be accessed: {}", path.display())
            })?;

            // Check file size
            if self.config.max_file_size > 0 {
                let metadata =
                    fs::metadata(&canonical_path).context("Failed to get file metadata")?;
                if metadata.len() as usize > self.config.max_file_size {
                    warn!(
                        "File too large to track: {} ({} bytes)",
                        canonical_path.display(),
                        metadata.len()
                    );
                    continue;
                }
            }

            let path_str = canonical_path.to_string_lossy().to_string();
            let entry_name = Self::path_to_entry_name(&path_str);

            let content = fs::read(&canonical_path).context("Failed to read file")?;
            let oid = self.repo.blob(&content)?;
            tree_builder.insert(&entry_name, oid, 0o100644)?;
            file_count += 1;
        }

        if file_count == 0 {
            debug!("No valid files to snapshot");
            return Ok(String::new());
        }

        let tree_oid = tree_builder.write()?;
        let tree = self.repo.find_tree(tree_oid)?;

        // Get parent commit and check if tree is different
        let parent = self.get_head_commit()?;
        let parent_tree = parent.tree()?;

        if tree.id() == parent_tree.id() {
            debug!("No changes detected, skipping snapshot");
            return Ok(String::new());
        }

        let parents = vec![parent];

        // Create signature
        let sig = Signature::now("nb-claw", "nb-claw@local")?;

        // Create commit
        let commit_oid = self.repo.commit(
            Some("HEAD"),
            &sig,
            &sig,
            message,
            &tree,
            &parents.iter().collect::<Vec<_>>(),
        )?;

        let commit_id = commit_oid.to_string();
        info!("Created snapshot: {} - {}", &commit_id[..7], message);

        // Cleanup old snapshots if needed
        if self.config.max_snapshots > 0 {
            self.cleanup_old_snapshots()?;
        }

        Ok(commit_id)
    }

    /// Convert a file path to a unique tree entry name using base64 encoding
    ///
    /// This allows arbitrary paths to be stored in the git tree without
    /// filesystem naming restrictions.
    fn path_to_entry_name(path: &str) -> String {
        URL_SAFE_NO_PAD.encode(path.as_bytes())
    }

    /// Decode a tree entry name back to the original file path
    fn entry_name_to_path(entry_name: &str) -> Result<String> {
        let bytes = URL_SAFE_NO_PAD
            .decode(entry_name)
            .context("Failed to decode entry name")?;
        String::from_utf8(bytes).context("Invalid UTF-8 in decoded path")
    }

    /// Get the HEAD commit
    fn get_head_commit(&self) -> Result<Commit<'_>> {
        let head = self.repo.head().context("Failed to get HEAD")?;
        let oid = head.target().context("HEAD has no target")?;
        self.repo
            .find_commit(oid)
            .context("Failed to find HEAD commit")
    }

    /// List all snapshots
    ///
    /// # Arguments
    /// * `limit` - Maximum number of snapshots to return (default: 100)
    ///
    /// # Returns
    /// Vector of snapshot information, newest first
    pub fn list_snapshots(&self, limit: Option<usize>) -> Result<Vec<SnapshotInfo>> {
        let mut rev_walk = self.repo.revwalk()?;
        rev_walk.push_head()?;

        let limit = limit.unwrap_or(100);
        let mut snapshots = Vec::new();

        for (i, oid_result) in rev_walk.enumerate() {
            if i >= limit {
                break;
            }

            if let Ok(oid) = oid_result {
                if let Ok(commit) = self.repo.find_commit(oid) {
                    let message = commit.message().unwrap_or("").to_string();

                    // Skip initial commit
                    if message == "Initial commit" {
                        continue;
                    }

                    let timestamp = DateTime::from_timestamp(commit.time().seconds(), 0)
                        .unwrap_or_else(|| Utc::now());

                    let tree = commit.tree().ok();
                    let file_count = tree.map(|t| t.len()).unwrap_or(0);

                    snapshots.push(SnapshotInfo {
                        id: oid.to_string(),
                        short_id: oid.to_string()[..7].into(),
                        message,
                        timestamp,
                        author: commit.author().name().unwrap_or("unknown").to_string(),
                        file_count,
                    });
                }
            }
        }

        Ok(snapshots)
    }

    /// Get a specific snapshot by ID
    ///
    /// # Arguments
    /// * `id` - Full or short commit hash (at least 7 characters)
    ///
    /// # Returns
    /// The snapshot if found, or None if not found
    pub fn get_snapshot(&self, id: &str) -> Result<Option<Snapshot>> {
        // Try full OID first
        let oid = if id.len() >= 7 {
            // Try as full OID
            match Oid::from_str(id) {
                Ok(o) => o,
                Err(_) => {
                    // Try as short OID by searching revwalk
                    let mut rev_walk = self.repo.revwalk()?;
                    rev_walk.push_head()?;

                    let mut found = None;
                    for oid_result in rev_walk {
                        if let Ok(oid) = oid_result {
                            let oid_str = oid.to_string();
                            if oid_str.starts_with(id) {
                                found = Some(oid);
                                break;
                            }
                        }
                    }

                    match found {
                        Some(o) => o,
                        None => return Ok(None),
                    }
                }
            }
        } else {
            return Ok(None);
        };

        let commit = match self.repo.find_commit(oid) {
            Ok(c) => c,
            Err(_) => return Ok(None),
        };

        let tree = commit.tree()?;
        let mut files = Vec::new();

        tree.walk(TreeWalkMode::PreOrder, |_root, entry| {
            if entry.kind() == Some(ObjectType::Blob) {
                let entry_name = entry.name().unwrap_or("");

                // Decode entry_name back to original path
                let original_path =
                    Self::entry_name_to_path(entry_name).unwrap_or_else(|_| entry_name.to_string());

                let blob = self.repo.find_blob(entry.id()).ok();

                let content = blob
                    .as_ref()
                    .and_then(|b| std::str::from_utf8(b.content()).ok().map(|s| s.to_string()));

                files.push(TrackedFile {
                    path: original_path,
                    content,
                    size: blob.as_ref().map(|b| b.content().len()).unwrap_or(0),
                    hash: entry.id().to_string(),
                });
            }
            TreeWalkResult::Ok
        })?;

        let timestamp =
            DateTime::from_timestamp(commit.time().seconds(), 0).unwrap_or_else(|| Utc::now());

        let info = SnapshotInfo {
            id: oid.to_string(),
            short_id: oid.to_string()[..7].to_string(),
            message: commit.message().unwrap_or("").to_string(),
            timestamp,
            author: commit.author().name().unwrap_or("unknown").to_string(),
            file_count: files.len(),
        };

        Ok(Some(Snapshot { info, files }))
    }

    /// Restore a file from snapshots
    ///
    /// Searches through commit history to find the file, even if it was deleted.
    ///
    /// # Arguments
    /// * `_snapshot_id` - Reserved for future use (currently searches all history)
    /// * `file_path` - Path to restore (can be partial path for fuzzy matching)
    ///
    /// # Returns
    /// `true` if the file was successfully restored
    ///
    /// # Errors
    /// Returns error if the file cannot be found in any snapshot
    pub fn restore_file(&self, _snapshot_id: &str, file_path: &Path) -> Result<bool> {
        let path_str = file_path.to_string_lossy().to_string();
        let path_lower = path_str.to_lowercase();

        // Compute entry_name for direct lookup
        let entry_name_for_path = Self::path_to_entry_name(&path_str);

        // Search through commit history to find the file
        let mut rev_walk = self.repo.revwalk()?;
        rev_walk.push_head()?;

        for oid_result in rev_walk {
            if let Ok(oid) = oid_result {
                if let Ok(commit) = self.repo.find_commit(oid) {
                    if let Ok(tree) = commit.tree() {
                        // Try direct lookup by entry_name
                        if let Some(entry) = tree.get_name(&entry_name_for_path) {
                            return self.restore_file_from_entry(&entry, &path_str);
                        }

                        // Scan all entries and decode paths for matching
                        for entry in tree.iter() {
                            let entry_name = entry.name().unwrap_or("");

                            // Try to decode the path
                            if let Ok(decoded_path) = Self::entry_name_to_path(entry_name) {
                                if decoded_path == path_str
                                    || decoded_path.ends_with(&path_str)
                                    || decoded_path.to_lowercase().ends_with(&path_lower)
                                {
                                    return self.restore_file_from_entry(&entry, &decoded_path);
                                }
                            }
                        }
                    }
                }
            }
        }

        Err(anyhow::anyhow!(
            "File not found in any snapshot: {}",
            file_path.display()
        ))
    }

    /// Restore a file from a tree entry
    fn restore_file_from_entry(
        &self,
        entry: &git2::TreeEntry<'_>,
        original_path: &str,
    ) -> Result<bool> {
        let blob = self.repo.find_blob(entry.id())?;
        let content = blob.content();

        let actual_path = PathBuf::from(original_path);

        // Create parent directories if needed
        if let Some(parent) = actual_path.parent() {
            if !parent.exists() {
                fs::create_dir_all(parent)?;
            }
        }

        fs::write(&actual_path, content)?;
        info!("Restored file from history: {}", actual_path.display());
        Ok(true)
    }

    /// Get the status of a file
    ///
    /// # Arguments
    /// * `file_path` - Path to check
    ///
    /// # Returns
    /// Status information including current state
    pub fn get_file_status(&self, file_path: &Path) -> Result<FileStatusInfo> {
        let path_str = file_path.to_string_lossy().to_string();

        // Compute entry_name for this path
        let entry_name = Self::path_to_entry_name(&path_str);

        // Check if file exists in HEAD tree
        let stored_content = self.get_file_content_by_entry(&entry_name)?;

        // Check if file exists on disk
        let current_content = fs::read(file_path).ok();

        let status = match (current_content, stored_content) {
            (None, Some(_)) => FileStatus::Deleted,
            (Some(current), Some(stored)) if current != stored => FileStatus::Modified,
            (Some(_), None) => FileStatus::New,
            _ => FileStatus::Unmodified,
        };

        Ok(FileStatusInfo {
            path: path_str,
            status,
        })
    }

    /// Get file content from HEAD by entry name
    fn get_file_content_by_entry(&self, entry_name: &str) -> Result<Option<Vec<u8>>> {
        let commit = self.get_head_commit().ok();
        if let Some(commit) = commit {
            let tree = commit.tree().ok();
            if let Some(tree) = tree {
                if let Some(entry) = tree.get_name(entry_name) {
                    let blob = self.repo.find_blob(entry.id())?;
                    return Ok(Some(blob.content().to_vec()));
                }
            }
        }
        Ok(None)
    }

    /// List all tracked files from the HEAD snapshot
    ///
    /// # Returns
    /// Vector of tracked file paths
    pub fn list_tracked_files(&self) -> Result<Vec<String>> {
        let commit = self.get_head_commit().ok();
        if let Some(commit) = commit {
            let tree = commit.tree().ok();
            if let Some(tree) = tree {
                let mut files = Vec::new();
                tree.walk(TreeWalkMode::PreOrder, |_root, entry| {
                    if entry.kind() == Some(ObjectType::Blob) {
                        let entry_name = entry.name().unwrap_or("");
                        if let Ok(path) = Self::entry_name_to_path(entry_name) {
                            files.push(path);
                        }
                    }
                    TreeWalkResult::Ok
                })?;
                return Ok(files);
            }
        }
        Ok(Vec::new())
    }

    /// Get the number of tracked files from the HEAD snapshot
    pub fn tracked_count(&self) -> usize {
        self.get_head_commit()
            .ok()
            .and_then(|c| c.tree().ok())
            .map(|t| t.len())
            .unwrap_or(0)
    }

    /// Get configuration reference
    pub fn config(&self) -> &VcsConfig {
        &self.config
    }

    /// Cleanup old snapshots (currently just logs a warning)
    fn cleanup_old_snapshots(&self) -> Result<()> {
        let snapshots = self.list_snapshots(None)?;
        if snapshots.len() <= self.config.max_snapshots {
            return Ok(());
        }

        // Git doesn't support easy deletion of old commits
        // For now, we just log a warning
        warn!(
            "Snapshot count ({}) exceeds limit ({}). Consider manual cleanup.",
            snapshots.len(),
            self.config.max_snapshots
        );

        Ok(())
    }
}

unsafe impl Send for VcsEngine {}
unsafe impl Sync for VcsEngine {}

#[cfg(test)]
mod tests {
    use {super::*, tempfile::TempDir};

    fn create_test_engine() -> (VcsEngine, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let config = VcsConfig {
            enabled: true,
            db_path: temp_dir.path().join("vcs").to_string_lossy().to_string(),
            max_snapshots: 10,
            auto_track: true,
            max_file_size: 1024 * 1024,
        };
        let engine = VcsEngine::new(config).unwrap();
        (engine, temp_dir)
    }

    #[test]
    fn test_create_engine() {
        let (engine, _temp) = create_test_engine();
        assert_eq!(engine.tracked_count(), 0);
    }

    #[test]
    fn test_create_snapshot() {
        let (engine, temp) = create_test_engine();
        let file_path = temp.path().join("test.txt");
        fs::write(&file_path, "test content").unwrap();

        let snapshot_id = engine
            .create_snapshot("Test snapshot", &[&file_path])
            .unwrap();
        assert!(!snapshot_id.is_empty());
        assert_eq!(engine.tracked_count(), 1);
    }

    #[test]
    fn test_list_snapshots() {
        let (engine, temp) = create_test_engine();
        let file_path = temp.path().join("test.txt");
        fs::write(&file_path, "test content").unwrap();

        engine
            .create_snapshot("Test snapshot", &[&file_path])
            .unwrap();

        let snapshots = engine.list_snapshots(None).unwrap();
        assert_eq!(snapshots.len(), 1);
        assert_eq!(snapshots[0].message, "Test snapshot");
    }

    #[test]
    fn test_restore_file() {
        let (engine, temp) = create_test_engine();
        let file_path = temp.path().join("test.txt");
        fs::write(&file_path, "original content").unwrap();

        let canonical_path = file_path.canonicalize().unwrap();
        let snapshot_id = engine
            .create_snapshot("Original", &[&canonical_path])
            .unwrap();

        // Modify file
        fs::write(&file_path, "modified content").unwrap();

        // Restore
        let result = engine.restore_file(&snapshot_id, &canonical_path);
        assert!(result.is_ok());

        let content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "original content");
    }

    #[test]
    fn test_get_snapshot_returns_original_path() {
        let (engine, temp) = create_test_engine();
        let file_path = temp.path().join("test.txt");
        fs::write(&file_path, "test content").unwrap();

        let snapshot_id = engine.create_snapshot("Test", &[&file_path]).unwrap();

        let snapshot = engine.get_snapshot(&snapshot_id).unwrap().unwrap();
        assert_eq!(snapshot.files.len(), 1);

        // The path should be the original path
        let file = &snapshot.files[0];
        assert!(file.path.contains("test.txt"));
    }

    #[test]
    fn test_list_tracked_files() {
        let (engine, temp) = create_test_engine();
        let file1 = temp.path().join("file1.txt");
        let file2 = temp.path().join("file2.txt");
        fs::write(&file1, "content1").unwrap();
        fs::write(&file2, "content2").unwrap();

        engine.create_snapshot("Test", &[&file1, &file2]).unwrap();

        let tracked = engine.list_tracked_files().unwrap();
        assert_eq!(tracked.len(), 2);
    }
}
