//! Python bindings for VcsEngine
//!
//! Provides a Python module `vcs` that allows the AI assistant to:
//! - Create snapshots (commits) of file states
//! - List and retrieve snapshots
//! - Restore files from any snapshot
//!
//! # Usage Example
//! ```python
//! import vcs
//!
//! # Create a snapshot of files
//! vcs.snapshot("Initial version", ["D:\\project\\file.txt"])
//!
//! # List all snapshots
//! for snap in vcs.list():
//!     print(f"[{snap.short_id}] {snap.message}")
//!
//! # Restore a deleted file
//! vcs.restore("D:\\project\\file.txt")
//! ```

use {
    crate::{
        python::Module,
        vcs::engine::{FileStatus, FileStatusInfo, Snapshot, SnapshotInfo, TrackedFile, VcsEngine},
    },
    pyo3::{exceptions::PyRuntimeError, prelude::*},
    std::{path::Path, sync::Weak},
};

/// Version Control System Manager
///
/// Provides methods to create snapshots, list files, and restore files.
/// Files are tracked by their absolute path and stored in a local git repository.
///
/// # Example
/// ```python
/// import vcs
///
/// # Create snapshot of files
/// vcs.snapshot("Saved changes", ["D:\\work\\document.txt", "D:\\work\\config.json"])
///
/// # List snapshots
/// snapshots = vcs.list()
/// for snap in snapshots:
///     print(f"{snap.short_id}: {snap.message}")
///
/// # Restore a file (works even if deleted)
/// vcs.restore("D:\\work\\document.txt")
/// ```
#[pyclass]
pub struct PyVcsManager {
    inner: Weak<VcsEngine>,
}

#[pymethods]
impl PyVcsManager {
    /// Create a snapshot (commit) of the specified files.
    ///
    /// Only creates a snapshot if there are changes since the last snapshot.
    ///
    /// # Arguments
    /// * `message` - Description of this snapshot
    /// * `paths` - List of file paths to include in the snapshot
    ///
    /// # Returns
    /// The commit ID (hash), or empty string if no changes
    ///
    /// # Example
    /// ```python
    /// commit_id = vcs.snapshot("Updated configuration", ["config.json", "data.csv"])
    /// if commit_id:
    ///     print(f"Created snapshot: {commit_id[:7]}")
    /// ```
    fn snapshot(&self, message: String, paths: Vec<String>) -> PyResult<String> {
        let engine = self
            .inner
            .upgrade()
            .ok_or_else(|| PyRuntimeError::new_err("VCS engine has been dropped"))?;

        let paths = paths.iter().map(Path::new).collect::<Vec<_>>();
        engine
            .create_snapshot(&message, &paths)
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))
    }

    /// List all snapshots.
    ///
    /// # Arguments
    /// * `limit` - Maximum number of snapshots to return (default: 100)
    ///
    /// # Returns
    /// List of `PySnapshotInfo` objects, newest first
    ///
    /// # Example
    /// ```python
    /// for snap in vcs.list(limit=10):
    ///     print(f"[{snap.short_id}] {snap.message} ({snap.file_count} files)")
    /// ```
    #[pyo3(signature = (limit=None))]
    fn list(&self, limit: Option<usize>) -> PyResult<Vec<PySnapshotInfo>> {
        let engine = self
            .inner
            .upgrade()
            .ok_or_else(|| PyRuntimeError::new_err("VCS engine has been dropped"))?;

        let snapshots = engine
            .list_snapshots(limit)
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))?;

        Ok(snapshots.into_iter().map(PySnapshotInfo::from).collect())
    }

    /// Get detailed information about a specific snapshot.
    ///
    /// # Arguments
    /// * `id` - Full or short commit hash (at least 7 characters)
    ///
    /// # Returns
    /// `PySnapshot` object if found, `None` if not found
    ///
    /// # Example
    /// ```python
    /// snapshot = vcs.get("abc1234")
    /// if snapshot:
    ///     print(f"Message: {snapshot.info.message}")
    ///     for f in snapshot.files:
    ///         print(f"  - {f.path} ({f.size} bytes)")
    /// ```
    fn get(&self, id: String) -> PyResult<Option<PySnapshot>> {
        let engine = self
            .inner
            .upgrade()
            .ok_or_else(|| PyRuntimeError::new_err("VCS engine has been dropped"))?;

        let snapshot = engine
            .get_snapshot(&id)
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))?;

        Ok(snapshot.map(PySnapshot::from))
    }

    /// Restore a file from the most recent snapshot that contains it.
    ///
    /// This method searches through all snapshots to find the file,
    /// making it possible to restore files that have been deleted.
    ///
    /// # Arguments
    /// * `file_path` - Path to restore (can be partial path for fuzzy matching)
    ///
    /// # Returns
    /// `True` if the file was restored successfully
    ///
    /// # Raises
    /// RuntimeError if the file cannot be found in any snapshot
    ///
    /// # Example
    /// ```python
    /// # Restore a deleted file
    /// try:
    ///     vcs.restore("D:\\work\\document.txt")
    ///     print("File restored!")
    /// except RuntimeError as e:
    ///     print(f"Restore failed: {e}")
    /// ```
    fn restore(&self, file_path: String) -> PyResult<bool> {
        let engine = self
            .inner
            .upgrade()
            .ok_or_else(|| PyRuntimeError::new_err("VCS engine has been dropped"))?;

        engine
            .restore_file("", Path::new(&file_path))
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))
    }

    /// Get the status of a file.
    ///
    /// # Arguments
    /// * `file_path` - Path to check
    ///
    /// # Returns
    /// `PyFileStatus` object with path and status
    ///
    /// # Example
    /// ```python
    /// status = vcs.status("config.json")
    /// print(f"{status.path}: {status.status}")
    /// # Status can be: "new", "modified", "deleted", "unmodified"
    /// ```
    fn status(&self, file_path: String) -> PyResult<PyFileStatus> {
        let engine = self
            .inner
            .upgrade()
            .ok_or_else(|| PyRuntimeError::new_err("VCS engine has been dropped"))?;

        let status = engine
            .get_file_status(Path::new(&file_path))
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))?;

        Ok(PyFileStatus::from(status))
    }

    /// List all tracked files from the latest snapshot.
    ///
    /// # Returns
    /// List of tracked file paths
    ///
    /// # Example
    /// ```python
    /// for path in vcs.tracked():
    ///     print(f"  - {path}")
    /// ```
    fn tracked(&self) -> PyResult<Vec<String>> {
        let engine = self
            .inner
            .upgrade()
            .ok_or_else(|| PyRuntimeError::new_err("VCS engine has been dropped"))?;

        engine
            .list_tracked_files()
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))
    }

    /// Get the number of tracked files from the latest snapshot.
    fn count(&self) -> PyResult<usize> {
        let engine = self
            .inner
            .upgrade()
            .ok_or_else(|| PyRuntimeError::new_err("VCS engine has been dropped"))?;

        Ok(engine.tracked_count())
    }

    fn __str__(&self) -> String {
        "VcsManager(版本控制器)".to_string()
    }

    fn __repr__(&self) -> String {
        self.__str__()
    }
}

/// Snapshot metadata (lightweight info without file contents).
///
/// # Attributes
/// * `id` - Full commit hash
/// * `short_id` - First 7 characters of hash
/// * `message` - Commit message
/// * `timestamp` - ISO 8601 timestamp
/// * `author` - Commit author
/// * `file_count` - Number of files in this snapshot
#[pyclass(skip_from_py_object)]
#[derive(Clone)]
pub struct PySnapshotInfo {
    #[pyo3(get)]
    pub id: String,
    #[pyo3(get)]
    pub short_id: String,
    #[pyo3(get)]
    pub message: String,
    #[pyo3(get)]
    pub timestamp: String,
    #[pyo3(get)]
    pub author: String,
    #[pyo3(get)]
    pub file_count: usize,
}

impl From<SnapshotInfo> for PySnapshotInfo {
    fn from(info: SnapshotInfo) -> Self {
        Self {
            id: info.id,
            short_id: info.short_id,
            message: info.message,
            timestamp: info.timestamp.to_rfc3339(),
            author: info.author,
            file_count: info.file_count,
        }
    }
}

#[pymethods]
impl PySnapshotInfo {
    fn __str__(&self) -> String {
        format!(
            "[{}] {} - {} ({} files)",
            self.short_id, self.message, self.timestamp, self.file_count
        )
    }

    fn __repr__(&self) -> String {
        self.__str__()
    }
}

/// Detailed snapshot with file information.
///
/// # Attributes
/// * `info` - Snapshot metadata (`PySnapshotInfo`)
/// * `files` - List of files in this snapshot (`List[PyTrackedFile]`)
#[pyclass(skip_from_py_object)]
#[derive(Clone)]
pub struct PySnapshot {
    #[pyo3(get)]
    pub info: PySnapshotInfo,
    #[pyo3(get)]
    pub files: Vec<PyTrackedFile>,
}

impl From<Snapshot> for PySnapshot {
    fn from(snapshot: Snapshot) -> Self {
        Self {
            info: PySnapshotInfo::from(snapshot.info),
            files: snapshot
                .files
                .into_iter()
                .map(PyTrackedFile::from)
                .collect(),
        }
    }
}

#[pymethods]
impl PySnapshot {
    fn __str__(&self) -> String {
        format!(
            "Snapshot {} with {} files",
            self.info.short_id,
            self.files.len()
        )
    }

    fn __repr__(&self) -> String {
        self.__str__()
    }
}

/// A file in a snapshot.
///
/// # Attributes
/// * `path` - Original file path (as tracked)
/// * `size` - File size in bytes
/// * `hash` - Content hash (SHA-1)
/// * `content` - File content as string (if texted, None if binary)
#[pyclass(skip_from_py_object)]
#[derive(Clone)]
pub struct PyTrackedFile {
    #[pyo3(get)]
    pub path: String,
    #[pyo3(get)]
    pub size: usize,
    #[pyo3(get)]
    pub hash: String,
    pub content: Option<String>,
}

impl From<TrackedFile> for PyTrackedFile {
    fn from(file: TrackedFile) -> Self {
        Self {
            path: file.path,
            size: file.size,
            hash: file.hash,
            content: file.content,
        }
    }
}

#[pymethods]
impl PyTrackedFile {
    /// Get file content if available (text files only).
    #[pyo3(signature = (_encoding="utf-8"))]
    fn content(&self, _encoding: &str) -> Option<String> {
        self.content.clone()
    }

    fn __str__(&self) -> String {
        let short_hash = if self.hash.len() > 7 {
            &self.hash[..7]
        } else {
            &self.hash
        };
        format!("{} ({} bytes, {})", self.path, self.size, short_hash)
    }

    fn __repr__(&self) -> String {
        self.__str__()
    }
}

/// File status information.
///
/// # Attributes
/// * `path` - File path
/// * `status` - Status string: "new", "modified", "deleted", or "unmodified"
#[pyclass(skip_from_py_object)]
#[derive(Clone)]
pub struct PyFileStatus {
    #[pyo3(get)]
    pub path: String,
    #[pyo3(get)]
    pub status: String,
}

impl From<FileStatusInfo> for PyFileStatus {
    fn from(info: FileStatusInfo) -> Self {
        let status_str = match info.status {
            FileStatus::New => "new",
            FileStatus::Modified => "modified",
            FileStatus::Deleted => "deleted",
            FileStatus::Unmodified => "unmodified",
        };
        Self {
            path: info.path,
            status: status_str.to_string(),
        }
    }
}

#[pymethods]
impl PyFileStatus {
    fn __str__(&self) -> String {
        format!("{}: {}", self.path, self.status)
    }

    fn __repr__(&self) -> String {
        self.__str__()
    }
}

impl<'a> Module<'a> for Py<PyVcsManager> {
    fn get_name() -> &'static str {
        "vcs"
    }
}

/// Create a Python module for VCS access
pub fn create_vcs_module(vcs: Weak<VcsEngine>) -> PyResult<Py<PyVcsManager>> {
    Python::attach(|py| {
        let py_manager = PyVcsManager { inner: vcs };
        Py::new(py, py_manager)
    })
}
