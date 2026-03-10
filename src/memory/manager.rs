//! Python bindings for MemoryEngine

use {
    crate::{
        memory::engine::{Memory, MemoryEntry, MemoryStats, MemoryType, SearchResult},
        python::Module,
    },
    pyo3::{exceptions::PyRuntimeError, prelude::*},
    std::sync::{RwLock, Weak},
};

/// Python wrapper for MemoryEngine
#[pyclass]
pub struct PyMemoryManager {
    inner: Weak<RwLock<Memory>>,
}

#[pymethods]
impl PyMemoryManager {
    /// Add a new memory entry
    #[pyo3(signature = (content, key=None, tags=None, memory_type="LongTerm".to_string()))]
    fn add(
        &mut self,
        content: String,
        key: Option<String>,
        tags: Option<Vec<String>>,
        memory_type: String,
    ) -> PyResult<String> {
        let memory = self
            .inner
            .upgrade()
            .ok_or_else(|| PyRuntimeError::new_err("Memory engine has been dropped"))?;
        let mut memory = memory
            .write()
            .map_err(|e| PyRuntimeError::new_err(format!("Failed to acquire write lock: {}", e)))?;

        let memory_type = match memory_type.as_str() {
            "ShortTerm" => MemoryType::ShortTerm,
            "LongTerm" => MemoryType::LongTerm,
            "Procedural" => MemoryType::Procedural,
            "Personal" => MemoryType::Personal,
            _ => MemoryType::LongTerm,
        };

        memory
            .add(content, key, tags.unwrap_or_default(), memory_type)
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))
    }

    /// Simple remember API - easiest for models to use
    #[pyo3(signature = (content, importance=None))]
    fn remember(&mut self, content: String, importance: Option<f64>) -> PyResult<String> {
        let memory = self
            .inner
            .upgrade()
            .ok_or_else(|| PyRuntimeError::new_err("Memory engine has been dropped"))?;
        let mut memory = memory
            .write()
            .map_err(|e| PyRuntimeError::new_err(format!("Failed to acquire write lock: {}", e)))?;

        let memory_type = match importance.unwrap_or(0.5) {
            x if x >= 0.8 => MemoryType::Personal,
            x if x >= 0.5 => MemoryType::LongTerm,
            _ => MemoryType::ShortTerm,
        };

        memory
            .add(content, None, vec![], memory_type)
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))
    }

    /// Recall memories - search and return relevant ones
    #[pyo3(signature = (query, limit=5))]
    fn recall(&mut self, query: String, limit: usize) -> PyResult<Vec<PyMemoryRecall>> {
        let memory = self
            .inner
            .upgrade()
            .ok_or_else(|| PyRuntimeError::new_err("Memory engine has been dropped"))?;
        let memory = memory
            .read()
            .map_err(|e| PyRuntimeError::new_err(format!("Failed to acquire read lock: {}", e)))?;

        let results = memory.search(&query, limit);
        Ok(results.into_iter().map(PyMemoryRecall::from).collect())
    }

    /// Get a memory entry by ID
    fn get(&mut self, id: &str) -> PyResult<Option<PyMemoryEntry>> {
        let memory = self
            .inner
            .upgrade()
            .ok_or_else(|| PyRuntimeError::new_err("Memory engine has been dropped"))?;
        let mut memory = memory
            .write()
            .map_err(|e| PyRuntimeError::new_err(format!("Failed to acquire write lock: {}", e)))?;

        Ok(memory.get(id).map(PyMemoryEntry::from))
    }

    /// Search memory entries by content
    fn search(&self, query: &str) -> PyResult<Vec<PyMemoryEntry>> {
        let memory = self
            .inner
            .upgrade()
            .ok_or_else(|| PyRuntimeError::new_err("Memory engine has been dropped"))?;
        let memory = memory
            .read()
            .map_err(|e| PyRuntimeError::new_err(format!("Failed to acquire read lock: {}", e)))?;

        let results = memory.search(query, 100);
        Ok(results
            .into_iter()
            .map(|r| PyMemoryEntry::from(r.entry))
            .collect())
    }

    /// Get recent memory entries
    #[pyo3(signature = (limit=10, memory_type=None))]
    fn get_recent(
        &self,
        limit: usize,
        memory_type: Option<String>,
    ) -> PyResult<Vec<PyMemoryEntry>> {
        let memory = self
            .inner
            .upgrade()
            .ok_or_else(|| PyRuntimeError::new_err("Memory engine has been dropped"))?;
        let memory = memory
            .read()
            .map_err(|e| PyRuntimeError::new_err(format!("Failed to acquire read lock: {}", e)))?;

        let mt = memory_type.and_then(|s| match s.as_str() {
            "ShortTerm" => Some(MemoryType::ShortTerm),
            "LongTerm" => Some(MemoryType::LongTerm),
            "Procedural" => Some(MemoryType::Procedural),
            "Personal" => Some(MemoryType::Personal),
            _ => None,
        });

        Ok(memory
            .get_recent(limit, mt)
            .into_iter()
            .map(PyMemoryEntry::from)
            .collect())
    }

    /// Forget memories by semantic query with importance decay
    /// Decays importance of matching entries by 0.1; deletes if importance drops below 0
    /// limit: max number of entries to process (default 3)
    /// Returns the number of deleted entries
    #[pyo3(signature = (query, limit=3))]
    fn forget(&mut self, query: &str, limit: usize) -> PyResult<usize> {
        let memory = self
            .inner
            .upgrade()
            .ok_or_else(|| PyRuntimeError::new_err("Memory engine has been dropped"))?;
        let mut memory = memory
            .write()
            .map_err(|e| PyRuntimeError::new_err(format!("Failed to acquire write lock: {}", e)))?;

        // Search for matching memories (same algorithm as recall)
        let results = memory.search(query, limit);

        if results.is_empty() {
            return Ok(0);
        }

        // Decay importance by 0.1 for each entry, delete if importance < 0
        let mut deleted_count = 0;
        for result in results {
            let id = result.entry.id.clone();
            let current_importance = result.entry.importance;
            let new_importance = current_importance - 0.1;

            if new_importance < 0.0 {
                // Delete entry completely
                memory
                    .delete(&id)
                    .map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
                deleted_count += 1;
            } else {
                // Update importance
                memory
                    .update_importance(&id, new_importance)
                    .map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
            }
        }

        Ok(deleted_count)
    }

    /// Get the number of memory entries
    fn len(&self) -> PyResult<usize> {
        let memory = self
            .inner
            .upgrade()
            .ok_or_else(|| PyRuntimeError::new_err("Memory engine has been dropped"))?;
        let memory = memory
            .read()
            .map_err(|e| PyRuntimeError::new_err(format!("Failed to acquire read lock: {}", e)))?;

        Ok(memory.stats().total_entries)
    }

    /// List all memory entries
    fn list(&self) -> PyResult<Vec<PyMemoryEntry>> {
        let memory = self
            .inner
            .upgrade()
            .ok_or_else(|| PyRuntimeError::new_err("Memory engine has been dropped"))?;
        let memory = memory
            .read()
            .map_err(|e| PyRuntimeError::new_err(format!("Failed to acquire read lock: {}", e)))?;

        Ok(memory.list().into_iter().map(PyMemoryEntry::from).collect())
    }

    /// Get memory statistics
    fn stats(&self) -> PyResult<PyMemoryStats> {
        let memory = self
            .inner
            .upgrade()
            .ok_or_else(|| PyRuntimeError::new_err("Memory engine has been dropped"))?;
        let memory = memory
            .read()
            .map_err(|e| PyRuntimeError::new_err(format!("Failed to acquire read lock: {}", e)))?;

        Ok(PyMemoryStats::from(memory.stats()))
    }

    fn __str__(&self) -> String {
        "MemoryManager(记忆管理器)".to_string()
    }

    fn __repr__(&self) -> String {
        self.__str__()
    }

    /// Consolidate memories (cleanup and optimize)
    fn consolidate(&mut self) -> PyResult<bool> {
        let memory = self
            .inner
            .upgrade()
            .ok_or_else(|| PyRuntimeError::new_err("Memory engine has been dropped"))?;
        let mut memory = memory
            .write()
            .map_err(|e| PyRuntimeError::new_err(format!("Failed to acquire write lock: {}", e)))?;

        memory
            .consolidate_memories()
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
        memory
            .save_to_disk()
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
        Ok(true)
    }
}

/// Python wrapper for memory recall result
#[pyclass(skip_from_py_object)]
#[derive(Clone)]
pub struct PyMemoryRecall {
    pub content: String,
    pub relevance: f64,
    pub importance: f64,
    pub id: String,
}

impl From<SearchResult> for PyMemoryRecall {
    fn from(result: SearchResult) -> Self {
        Self {
            content: result.entry.content,
            relevance: result.score,
            importance: result.entry.importance,
            id: result.entry.id,
        }
    }
}

#[pymethods]
impl PyMemoryRecall {
    #[getter]
    fn content(&self) -> String {
        self.content.clone()
    }

    #[getter]
    fn relevance(&self) -> f64 {
        self.relevance
    }

    #[getter]
    fn importance(&self) -> f64 {
        self.importance
    }

    #[getter]
    fn id(&self) -> String {
        self.id.clone()
    }

    fn __str__(&self) -> String {
        format!(
            "{} [相关度: {:.0}% | 重要性: {:.1}]",
            self.content,
            self.relevance * 100.0,
            self.importance
        )
    }

    fn __repr__(&self) -> String {
        self.__str__()
    }
}

/// Python wrapper for memory statistics
#[pyclass(skip_from_py_object)]
#[derive(Clone)]
pub struct PyMemoryStats {
    pub total_entries: usize,
    pub short_term_count: usize,
    pub long_term_count: usize,
}

impl From<MemoryStats> for PyMemoryStats {
    fn from(stats: MemoryStats) -> Self {
        Self {
            total_entries: stats.total_entries,
            short_term_count: stats.short_term_count,
            long_term_count: stats.long_term_count,
        }
    }
}

#[pymethods]
impl PyMemoryStats {
    #[getter]
    fn total_entries(&self) -> usize {
        self.total_entries
    }

    #[getter]
    fn short_term_count(&self) -> usize {
        self.short_term_count
    }

    #[getter]
    fn long_term_count(&self) -> usize {
        self.long_term_count
    }

    fn __str__(&self) -> String {
        format!(
            "记忆统计: 共{}条 (短期{}, 长期{})",
            self.total_entries, self.short_term_count, self.long_term_count
        )
    }

    fn __repr__(&self) -> String {
        self.__str__()
    }
}

/// Python wrapper for MemoryEntry
#[pyclass(from_py_object)]
#[derive(Clone)]
pub struct PyMemoryEntry {
    inner: MemoryEntry,
}

impl From<MemoryEntry> for PyMemoryEntry {
    fn from(entry: MemoryEntry) -> Self {
        Self { inner: entry }
    }
}

#[pymethods]
impl PyMemoryEntry {
    #[getter]
    fn id(&self) -> String {
        self.inner.id.clone()
    }

    #[getter]
    fn timestamp(&self) -> String {
        self.inner.timestamp.to_rfc3339()
    }

    #[getter]
    fn key(&self) -> Option<String> {
        self.inner.key.clone()
    }

    #[getter]
    fn content(&self) -> String {
        self.inner.content.clone()
    }

    #[getter]
    fn tags(&self) -> Vec<String> {
        self.inner.tags.clone()
    }

    #[getter]
    fn memory_type(&self) -> String {
        match self.inner.memory_type {
            MemoryType::ShortTerm => "ShortTerm",
            MemoryType::LongTerm => "LongTerm",
            MemoryType::Procedural => "Procedural",
            MemoryType::Personal => "Personal",
        }
        .to_string()
    }

    #[getter]
    fn importance(&self) -> f64 {
        self.inner.importance
    }

    /// Convert to string representation
    fn __str__(&self) -> String {
        let preview: String = self.inner.content.chars().take(80).collect();
        let key_str = self
            .inner
            .key
            .as_ref()
            .map(|k| format!("[{}] ", k))
            .unwrap_or_default();
        format!(
            "{}{} ({}条目)",
            key_str,
            preview,
            match self.inner.memory_type {
                MemoryType::ShortTerm => "短期",
                MemoryType::LongTerm => "长期",
                MemoryType::Procedural => "过程",
                MemoryType::Personal => "个人",
            }
        )
    }

    /// Detailed representation
    fn __repr__(&self) -> String {
        self.__str__()
    }
}

impl<'a> Module<'a> for Py<PyMemoryManager> {
    fn get_name() -> &'static str {
        "memory"
    }
}

/// Create a Python module for memory access
pub fn create_memory_module(memory: Weak<RwLock<Memory>>) -> PyResult<Py<PyMemoryManager>> {
    Python::attach(|py| {
        let py_manager = PyMemoryManager { inner: memory };

        Py::new(py, py_manager)
    })
}
