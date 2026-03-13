//! Advanced memory engine for AI assistant
//!
//! Provides intelligent memory management with:
//! - Long-term and short-term memory layers
//! - Vector-based semantic search using fastembed
//! - Importance scoring and automatic cleanup
//! - Memory consolidation and deduplication

use {
    crate::config::{EmbeddingConfig, MemoryConfig, StorageFormat},
    anyhow::{Context, Result},
    chrono::{DateTime, Utc},
    fastembed::{EmbeddingModel, TextEmbedding, TextInitOptions},
    postcard::{from_bytes, to_allocvec},
    serde::{Deserialize, Serialize},
    serde_json::from_reader,
    std::{
        collections::HashMap,
        env,
        fs::{File, create_dir_all, remove_file},
        io::{BufReader, BufWriter, Read, Write},
        path::PathBuf,
        sync::{Arc, Mutex},
    },
    tracing::info,
    uuid::Uuid,
};

/// Initialize embedding model from configuration
fn init_embedding_model(config: &EmbeddingConfig) -> Result<Option<Arc<Mutex<TextEmbedding>>>> {
    if !config.enabled {
        info!("Embedding model disabled in configuration");
        return Ok(None);
    }

    // Set HF_ENDPOINT if configured
    if let Some(endpoint) = &config.hf_endpoint {
        unsafe {
            env::set_var("HF_ENDPOINT", endpoint);
        }
        info!("Using HuggingFace endpoint: {}", endpoint);
    }

    // Parse model name
    let model: EmbeddingModel = config.model.parse().map_err(|e: String| {
        anyhow::anyhow!("Invalid embedding model '{}': {}", config.model, e)
    })?;

    info!(
        "Downloading and initializing embedding model: {}",
        config.model
    );

    // Create init options with TextInitOptions
    let mut options = TextInitOptions::new(model);
    options.show_download_progress = true;
    if let Some(p) = &config.cache_dir {
        options.cache_dir = p.into()
    }

    let model = TextEmbedding::try_new(options)?;
    info!(
        "✓ Successfully initialized embedding model: {}",
        config.model
    );
    Ok(Some(Arc::new(Mutex::new(model))))
}

/// Memory importance score (0.0 to 1.0)
pub type ImportanceScore = f64;

/// Memory vector (semantic embedding representation)
pub type MemoryVector = Vec<f32>;

/// Memory entry with vector representation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEntry {
    /// Unique identifier
    pub id: String,
    /// Timestamp
    pub timestamp: DateTime<Utc>,
    /// Memory content
    pub content: String,
    /// Vector representation (semantic embedding)
    #[serde(skip)]
    pub vector: Option<MemoryVector>,
    /// Tags for categorization
    pub tags: Vec<String>,
    /// Optional key for quick lookup
    pub key: Option<String>,
    /// Importance score (0.0 to 1.0)
    pub importance: ImportanceScore,
    /// Access count
    pub access_count: u32,
    /// Last access timestamp
    pub last_accessed: DateTime<Utc>,
    /// Memory type
    pub memory_type: MemoryType,
}

/// Memory type classification
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum MemoryType {
    /// Short-term working memory
    ShortTerm,
    /// Long-term persistent memory
    LongTerm,
    /// Procedural knowledge
    Procedural,
    /// Personal information
    Personal,
}

impl Default for MemoryType {
    fn default() -> Self {
        Self::LongTerm
    }
}

/// Search result with similarity score
#[derive(Debug, Clone)]
pub struct SearchResult {
    pub entry: MemoryEntry,
    pub score: f64,
}

/// Advanced memory engine with semantic vector search
pub struct Memory {
    config: MemoryConfig,
    entries: HashMap<String, MemoryEntry>,
    embedding_model: Option<Arc<Mutex<TextEmbedding>>>,
    last_consolidation: DateTime<Utc>,
}

impl Memory {
    /// Create a new memory engine
    pub fn new(config: MemoryConfig) -> Result<Self> {
        let storage_path = PathBuf::from(&config.storage_path);
        create_dir_all(&storage_path).context("Failed to create memory storage directory")?;

        // Initialize embedding model from configuration
        let embedding_model = init_embedding_model(&config.embedding)?;

        let mut engine = Self {
            config,
            entries: HashMap::new(),
            embedding_model,
            last_consolidation: Utc::now(),
        };

        // Load existing data and regenerate embeddings
        engine.load_from_disk()?;
        engine.regenerate_embeddings()?;

        Ok(engine)
    }

    /// Regenerate embeddings for all entries (after loading from disk)
    fn regenerate_embeddings(&mut self) -> Result<()> {
        if self.embedding_model.is_none() {
            return Ok(());
        }

        let ids: Vec<String> = self.entries.keys().cloned().collect();
        let contents: Vec<String> = self.entries.values().map(|e| e.content.clone()).collect();

        if contents.is_empty() {
            return Ok(());
        }

        if let Some(model) = &self.embedding_model {
            let mut model = model
                .lock()
                .map_err(|e| anyhow::anyhow!("Failed to acquire embedding model lock: {}", e))?;
            let embeddings = model
                .embed(contents, None)
                .context("Failed to generate embeddings")?;

            for (id, embedding) in ids.into_iter().zip(embeddings.into_iter()) {
                if let Some(entry) = self.entries.get_mut(&id) {
                    entry.vector = Some(embedding);
                }
            }
        }

        Ok(())
    }

    /// Generate embedding for a single text
    fn generate_embedding(&self, text: &str) -> Result<Vec<f32>> {
        if let Some(model) = &self.embedding_model {
            let mut model = model
                .lock()
                .map_err(|e| anyhow::anyhow!("Failed to acquire embedding model lock: {}", e))?;
            let embeddings = model
                .embed(vec![text.to_string()], None)
                .context("Failed to generate embedding")?;
            Ok(embeddings.into_iter().next().unwrap_or_default())
        } else {
            // Fallback: return empty vector
            Ok(Vec::new())
        }
    }

    /// Add a new memory entry
    pub fn add(
        &mut self,
        content: String,
        key: Option<String>,
        tags: Vec<String>,
        memory_type: MemoryType,
    ) -> Result<String> {
        let id = Uuid::new_v4().to_string();

        // Generate semantic embedding
        let vector = self.generate_embedding(&content)?;
        let importance = self.calculate_importance(&tags, memory_type);

        let entry = MemoryEntry {
            id: id.clone(),
            timestamp: Utc::now(),
            content,
            vector: if vector.is_empty() {
                None
            } else {
                Some(vector)
            },
            tags,
            key,
            importance,
            access_count: 0,
            last_accessed: Utc::now(),
            memory_type,
        };

        // Add to entries
        self.entries.insert(id.clone(), entry.clone());

        // Cleanup if needed
        if self.config.auto_consolidation {
            self.cleanup_if_needed()?;
        }

        tracing::debug!("Added memory entry: {}", id);
        Ok(id)
    }

    /// Search memories by content (semantic search)
    pub fn search(&self, query: &str, limit: usize) -> Vec<SearchResult> {
        let query_vector = match self.generate_embedding(query) {
            Ok(v) if !v.is_empty() => v,
            _ => return Vec::new(), // No embedding model available
        };

        // Do linear search through all entries
        let mut results = Vec::new();

        for entry in self.entries.values() {
            if let Some(vec) = &entry.vector {
                let score = self.cosine_similarity(vec, &query_vector);
                if score > 0.1 {
                    results.push(SearchResult {
                        entry: entry.clone(),
                        score,
                    });
                }
            }
        }

        // Sort by score (descending)
        results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        results.truncate(limit);

        results
    }

    /// Get a memory entry by ID
    pub fn get(&mut self, id: &str) -> Option<MemoryEntry> {
        if let Some(entry) = self.entries.get_mut(id) {
            entry.access_count += 1;
            entry.last_accessed = Utc::now();
            Some(entry.clone())
        } else {
            None
        }
    }

    /// Get memory by key
    #[allow(dead_code)]
    pub fn get_by_key(&mut self, key: &str) -> Option<MemoryEntry> {
        if let Some(entry) = self
            .entries
            .values_mut()
            .find(|e| e.key.as_deref() == Some(key))
        {
            entry.access_count += 1;
            entry.last_accessed = Utc::now();
            Some(entry.clone())
        } else {
            None
        }
    }

    /// Delete a memory entry
    pub fn delete(&mut self, id: &str) -> Result<bool> {
        if let Some(_entry) = self.entries.remove(id) {
            tracing::debug!("Deleted memory entry: {}", id);
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Update importance of a memory entry
    pub fn update_importance(&mut self, id: &str, new_importance: f64) -> Result<bool> {
        if let Some(entry) = self.entries.get_mut(id) {
            entry.importance = new_importance.clamp(0.0, 1.0);
            tracing::debug!("Updated importance for {}: {}", id, entry.importance);
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Get recent memories
    pub fn get_recent(&self, limit: usize, memory_type: Option<MemoryType>) -> Vec<MemoryEntry> {
        let mut entries: Vec<_> = self.entries.values().cloned().collect();

        if let Some(mt) = memory_type {
            entries.retain(|e| e.memory_type == mt);
        }

        entries.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        entries.truncate(limit);
        entries
    }

    /// Get all memories
    pub fn list(&self) -> Vec<MemoryEntry> {
        self.entries.values().cloned().collect()
    }

    /// Get memory statistics
    pub fn stats(&self) -> MemoryStats {
        let short_term_count = self
            .entries
            .values()
            .filter(|e| e.memory_type == MemoryType::ShortTerm)
            .count();
        let long_term_count = self
            .entries
            .values()
            .filter(|e| e.memory_type == MemoryType::LongTerm)
            .count();

        MemoryStats {
            total_entries: self.entries.len(),
            short_term_count,
            long_term_count,
            last_consolidation: self.last_consolidation,
        }
    }

    /// Calculate cosine similarity for f32 vectors
    fn cosine_similarity(&self, a: &[f32], b: &[f32]) -> f64 {
        let mut dot_product = 0.0_f32;
        let mut norm_a = 0.0_f32;
        let mut norm_b = 0.0_f32;

        let len = a.len().min(b.len());
        for i in 0..len {
            dot_product += a[i] * b[i];
            norm_a += a[i] * a[i];
            norm_b += b[i] * b[i];
        }

        norm_a = norm_a.sqrt();
        norm_b = norm_b.sqrt();

        if norm_a == 0.0 || norm_b == 0.0 {
            0.0
        } else {
            (dot_product / (norm_a * norm_b)) as f64
        }
    }

    /// Calculate initial importance score
    fn calculate_importance(&self, tags: &[String], memory_type: MemoryType) -> ImportanceScore {
        let mut score = 0.5; // Base score

        // Boost based on tags
        score += (tags.len() as f64 / 5.0).min(0.15);

        // Memory type adjustment
        match memory_type {
            MemoryType::Personal => score += 0.15,
            MemoryType::Procedural => score += 0.10,
            MemoryType::ShortTerm => score -= 0.1,
            _ => {}
        }

        score.clamp(0.0, 1.0)
    }

    /// Cleanup low-importance memories if needed
    fn cleanup_if_needed(&mut self) -> Result<()> {
        let now = Utc::now();
        let days_since_consolidation = (now - self.last_consolidation).num_days();

        // Only consolidate if it's been at least a day
        if days_since_consolidation < 1 {
            return Ok(());
        }

        self.consolidate_memories()?;
        Ok(())
    }

    /// Consolidate memories: update importance scores and remove old ones
    pub fn consolidate_memories(&mut self) -> Result<()> {
        let now = Utc::now();
        let mut to_remove = Vec::new();

        for (id, entry) in &mut self.entries {
            let days_old = (now - entry.timestamp).num_days() as f64;
            let days_since_access = (now - entry.last_accessed).num_days() as f64;

            // Decay importance based on time and access
            let time_decay = (days_old * self.config.time_decay_rate).exp();
            let access_boost = 1.0 / (1.0 + days_since_access * self.config.access_decay_rate);
            let access_factor = (entry.access_count as f64 / 10.0).min(1.0);

            entry.importance *= time_decay * access_boost * access_factor;

            // Remove low importance memories
            if entry.importance < self.config.min_importance
                && days_old > 7.0
                && entry.memory_type != MemoryType::Personal
            {
                to_remove.push(id.clone());
            }
        }

        // Remove marked entries
        for id in to_remove {
            self.delete(&id)?;
        }

        self.last_consolidation = now;
        info!("Memory consolidation completed");
        Ok(())
    }

    /// Save memory engine to disk
    pub fn save_to_disk(&self) -> Result<()> {
        let storage_path = PathBuf::from(&self.config.storage_path);
        let entries: Vec<&MemoryEntry> = self.entries.values().collect();

        match self.config.storage_format {
            StorageFormat::Json => {
                let entries_path = storage_path.join("entries.json");
                let entries_file =
                    File::create(&entries_path).context("Failed to create entries file")?;
                let writer = BufWriter::new(entries_file);
                serde_json::to_writer(writer, &entries)
                    .context("Failed to write entries as JSON")?;
            }
            StorageFormat::Binary => {
                let entries_path = storage_path.join("entries.bin");
                let encoded = to_allocvec(&entries).context("Failed to serialize entries")?;
                let mut file =
                    File::create(&entries_path).context("Failed to create entries file")?;
                file.write_all(&encoded)
                    .context("Failed to write entries as binary")?;
            }
        }

        tracing::debug!(
            "Saved {} memory entries to disk ({:?} format)",
            entries.len(),
            self.config.storage_format
        );
        Ok(())
    }

    /// Load memory engine from disk
    fn load_from_disk(&mut self) -> Result<()> {
        let storage_path = PathBuf::from(&self.config.storage_path);

        // Try binary format first if configured, then fall back to JSON
        let entries = match self.config.storage_format {
            StorageFormat::Binary => {
                let bin_path = storage_path.join("entries.bin");
                let json_path = storage_path.join("entries.json");

                if bin_path.exists() {
                    self.load_entries_binary(&bin_path)?
                } else if json_path.exists() {
                    // Migrate from JSON if binary file doesn't exist
                    info!("Migrating memory from JSON to binary format");
                    let entries = self.load_entries_json(&json_path)?;
                    // Save in binary format immediately
                    let encoded = to_allocvec(&entries).context("Failed to serialize entries")?;
                    let mut file =
                        File::create(&bin_path).context("Failed to create binary entries file")?;
                    file.write_all(&encoded)
                        .context("Failed to write binary entries")?;
                    // Remove old JSON file
                    let _ = remove_file(&json_path);
                    entries
                } else {
                    Vec::new()
                }
            }
            StorageFormat::Json => {
                let json_path = storage_path.join("entries.json");
                let bin_path = storage_path.join("entries.bin");

                if json_path.exists() {
                    self.load_entries_json(&json_path)?
                } else if bin_path.exists() {
                    // Migrate from binary if JSON file doesn't exist
                    info!("Migrating memory from binary to JSON format");
                    let entries = self.load_entries_binary(&bin_path)?;
                    // Save in JSON format immediately
                    let file =
                        File::create(&json_path).context("Failed to create JSON entries file")?;
                    let writer = BufWriter::new(file);
                    serde_json::to_writer(writer, &entries)
                        .context("Failed to write JSON entries")?;
                    // Remove old binary file
                    let _ = remove_file(&bin_path);
                    entries
                } else {
                    Vec::new()
                }
            }
        };

        // Rebuild entries (vectors will be regenerated)
        for entry in entries {
            let id = entry.id.clone();
            self.entries.insert(id, entry);
        }

        info!("Loaded {} memory entries", self.entries.len());
        Ok(())
    }

    /// Load entries from JSON file
    fn load_entries_json(&self, path: &PathBuf) -> Result<Vec<MemoryEntry>> {
        let file = File::open(path).context("Failed to open entries file")?;
        let reader = BufReader::new(file);
        from_reader(reader).context("Failed to parse entries as JSON")
    }

    /// Load entries from binary file
    fn load_entries_binary(&self, path: &PathBuf) -> Result<Vec<MemoryEntry>> {
        let mut file = File::open(path).context("Failed to open entries file")?;
        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer)
            .context("Failed to read entries file")?;
        from_bytes(&buffer).context("Failed to parse entries as binary")
    }
}

/// Memory statistics
#[derive(Debug, Clone)]
pub struct MemoryStats {
    pub total_entries: usize,
    pub short_term_count: usize,
    pub long_term_count: usize,
    pub last_consolidation: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::StorageFormat;

    #[test]
    fn test_memory_engine_creation() {
        let config = MemoryConfig {
            storage_path: tempfile::tempdir()
                .unwrap()
                .path()
                .to_string_lossy()
                .to_string(),
            embedding: EmbeddingConfig {
                enabled: false,
                ..Default::default()
            },
            ..Default::default()
        };

        let engine = Memory::new(config);
        assert!(engine.is_ok());
    }

    #[test]
    fn test_add_and_retrieve() {
        let temp_dir = tempfile::tempdir().unwrap();
        let config = MemoryConfig {
            storage_path: temp_dir.path().to_string_lossy().to_string(),
            embedding: EmbeddingConfig {
                enabled: false,
                ..Default::default()
            },
            ..Default::default()
        };

        let mut engine = Memory::new(config).unwrap();
        let id = engine
            .add(
                "Test memory content".to_string(),
                Some("test_key".to_string()),
                vec!["test".to_string()],
                MemoryType::LongTerm,
            )
            .unwrap();

        let entry = engine.get(&id);
        assert!(entry.is_some());
        assert_eq!(entry.unwrap().content, "Test memory content");
    }

    #[test]
    fn test_binary_storage_format() {
        let temp_dir = tempfile::tempdir().unwrap();
        let storage_path = temp_dir.path().to_string_lossy().to_string();

        // Create engine with binary format
        let config = MemoryConfig {
            storage_path: storage_path.clone(),
            storage_format: StorageFormat::Binary,
            embedding: EmbeddingConfig {
                enabled: false,
                ..Default::default()
            },
            ..Default::default()
        };

        let mut engine = Memory::new(config).unwrap();

        // Add some entries
        let id1 = engine
            .add(
                "Binary format test 1".to_string(),
                Some("key1".to_string()),
                vec!["test".to_string()],
                MemoryType::LongTerm,
            )
            .unwrap();
        let id2 = engine
            .add(
                "Binary format test 2".to_string(),
                None,
                vec!["test".to_string()],
                MemoryType::ShortTerm,
            )
            .unwrap();

        // Save to disk
        engine.save_to_disk().unwrap();

        // Verify binary file was created
        let bin_path = PathBuf::from(&storage_path).join("entries.bin");
        assert!(bin_path.exists(), "Binary file should exist");
        assert!(
            !PathBuf::from(&storage_path).join("entries.json").exists(),
            "JSON file should not exist"
        );

        // Load into new engine and verify
        let mut engine2 = Memory::new(MemoryConfig {
            storage_path: storage_path.clone(),
            storage_format: StorageFormat::Binary,
            embedding: EmbeddingConfig {
                enabled: false,
                ..Default::default()
            },
            ..Default::default()
        })
        .unwrap();

        let entry1 = engine2.get(&id1).unwrap();
        assert_eq!(entry1.content, "Binary format test 1");
        assert_eq!(entry1.key, Some("key1".to_string()));

        let entry2 = engine2.get(&id2).unwrap();
        assert_eq!(entry2.content, "Binary format test 2");
        assert_eq!(entry2.memory_type, MemoryType::ShortTerm);
    }

    #[test]
    fn test_json_to_binary_migration() {
        let temp_dir = tempfile::tempdir().unwrap();
        let storage_path = temp_dir.path().to_string_lossy().to_string();

        // Create engine with JSON format and add data
        let json_config = MemoryConfig {
            storage_path: storage_path.clone(),
            storage_format: StorageFormat::Json,
            embedding: EmbeddingConfig {
                enabled: false,
                ..Default::default()
            },
            ..Default::default()
        };

        let mut json_engine = Memory::new(json_config).unwrap();
        let id = json_engine
            .add(
                "Migration test".to_string(),
                Some("migrate_key".to_string()),
                vec!["migration".to_string()],
                MemoryType::Personal,
            )
            .unwrap();
        json_engine.save_to_disk().unwrap();

        // Verify JSON file exists
        assert!(PathBuf::from(&storage_path).join("entries.json").exists());

        // Now load with binary format - should migrate
        let mut binary_engine = Memory::new(MemoryConfig {
            storage_path: storage_path.clone(),
            storage_format: StorageFormat::Binary,
            embedding: EmbeddingConfig {
                enabled: false,
                ..Default::default()
            },
            ..Default::default()
        })
        .unwrap();

        // Verify data was migrated
        let entry = binary_engine.get(&id).unwrap();
        assert_eq!(entry.content, "Migration test");
        assert_eq!(entry.key, Some("migrate_key".to_string()));
        assert_eq!(entry.memory_type, MemoryType::Personal);

        // JSON file should be removed after migration
        assert!(!PathBuf::from(&storage_path).join("entries.json").exists());
    }
}
