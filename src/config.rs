//! Configuration management for nb-claw
//!
//! Handles loading and parsing TOML configuration files.

use {
    anyhow::{Context, Result},
    serde::{Deserialize, Serialize},
    std::{fs, io::Write, path::Path},
};

/// Available LLM providers with their default settings
/// Format: (provider_name, default_model, default_base_url)
pub const LLM_PROVIDERS: &[(&str, &str, &str)] = &[
    ("openai", "gpt-5.4", "https://api.openai.com/v1"),
    (
        "anthropic",
        "claude-opus-4-6-20260201",
        "https://api.anthropic.com",
    ),
    (
        "google",
        "gemini-3.1-pro-preview",
        "https://generativelanguage.googleapis.com/v1beta",
    ),
    (
        "longcat",
        "longcat-flash-thinking-2601",
        "https://api.longcat.chat/anthropic",
    ),
    ("moonshot", "kimi-k2.5", "https://api.moonshot.cn/v1"),
    ("zhipu", "glm-5", "https://open.bigmodel.cn/api/paas/v4"),
    (
        "aliyun",
        "qwen3-coder-next",
        "https://dashscope.aliyuncs.com",
    ),
    ("ollama", "llama3.2", "http://localhost:11434"),
    ("deepseek", "deepseek-v3", "https://api.deepseek.com"),
    ("xiaomi", "mimo-v2-flash", "https://api.xiaomimimo.com/v1"),
    (
        "volcengine",
        "doubao-2.0",
        "https://ark.cn-beijing.volces.com/api/v3",
    ),
    (
        "tencent",
        "hunyuan-image-3.0-instruct",
        "https://hunyuan.tencentcloudapi.com",
    ),
];

/// Available embedding models with descriptions
pub const EMBEDDING_MODELS: &[(&str, usize, &str)] = &[
    // BGE M3 (recommended default)
    (
        "BAAI/bge-m3",
        1024,
        "BGE M3 Multilingual (100+ languages, 8192 context) [DEFAULT]",
    ),
    // BGE English
    ("Xenova/bge-small-en-v1.5", 384, "BGE Small English (fast)"),
    (
        "Xenova/bge-base-en-v1.5",
        768,
        "BGE Base English (balanced)",
    ),
    (
        "Xenova/bge-large-en-v1.5",
        1024,
        "BGE Large English (best quality)",
    ),
    // BGE Chinese
    ("Xenova/bge-small-zh-v1.5", 512, "BGE Small Chinese"),
    ("Xenova/bge-large-zh-v1.5", 1024, "BGE Large Chinese"),
    // BGE M3
    (
        "BAAI/bge-m3",
        1024,
        "BGE M3 Multilingual (100+ languages, 8192 context)",
    ),
    // Multilingual E5
    (
        "intfloat/multilingual-e5-small",
        384,
        "E5 Small Multilingual",
    ),
    ("intfloat/multilingual-e5-base", 768, "E5 Base Multilingual"),
    // Snowflake Arctic
    (
        "snowflake/snowflake-arctic-embed-xs",
        384,
        "Snowflake Arctic XS",
    ),
    (
        "snowflake/snowflake-arctic-embed-s",
        384,
        "Snowflake Arctic S",
    ),
    (
        "Snowflake/snowflake-arctic-embed-m",
        768,
        "Snowflake Arctic M",
    ),
    (
        "snowflake/snowflake-arctic-embed-l",
        1024,
        "Snowflake Arctic L",
    ),
    // MiniLM
    ("Qdrant/all-MiniLM-L6-v2-onnx", 384, "MiniLM L6 v2 (fast)"),
    // Others
    (
        "nomic-ai/nomic-embed-text-v1.5",
        768,
        "Nomic Embed (8192 context)",
    ),
    ("Alibaba-NLP/gte-base-en-v1.5", 768, "Alibaba GTE Base"),
    (
        "mixedbread-ai/mxbai-embed-large-v1",
        1024,
        "MixedBread Large",
    ),
];

/// Default system prompt
pub const DEFAULT_SYSTEM_PROMPT: &str = r#"You are nb-claw, an autonomous AI assistant with the ability to execute Python code.

Remember:
- Only use Python built-in modules (os, sys, json, math, datetime, re, etc.)
- Never try to import third-party packages
- Keep your Python code simple and focused
- Use the tools to perform tasks rather than trying to guess the results

Be helpful, accurate, and concise in your responses."#;

/// Main configuration structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// LLM provider configuration
    pub llm: LlmConfig,
    /// Python interpreter configuration
    pub python: PythonConfig,
    /// Memory configuration
    pub memory: MemoryConfig,
    /// Assistant system prompt
    pub system: SystemConfig,
}

/// LLM provider configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmConfig {
    /// Provider name (openai, anthropic, zhipu, etc.)
    pub provider: String,
    /// Model name
    pub model: String,
    /// API key (can be overridden by environment variable)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    /// Base URL (optional, uses provider default if not specified)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    /// Request timeout in seconds
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
    /// Maximum number of retries
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,
    /// Secret ID for Tencent provider (can be overridden by environment variable)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub secret_id: Option<String>,
    /// Secret Key for Tencent provider (can be overridden by environment variable)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub secret_key: Option<String>,
}

/// Python interpreter configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PythonConfig {
    /// Enable sandbox mode (restrict access)
    #[serde(default = "default_sandbox")]
    pub sandbox: bool,
    /// Maximum execution time in seconds
    #[serde(
        default = "default_execution_timeout",
        rename = "max_execution_time",
        alias = "timeout_secs"
    )]
    pub max_execution_time: u64,
    /// Dangerous modules that are restricted in sandbox mode
    #[serde(default = "default_dangerous_modules")]
    pub dangerous_modules: Vec<String>,
    /// Safe modules allowed in sandbox mode
    #[serde(default = "default_safe_modules")]
    pub safe_modules: Vec<String>,
}

impl Default for PythonConfig {
    fn default() -> Self {
        Self {
            sandbox: true,
            max_execution_time: 30,
            dangerous_modules: vec![
                "os".into(),
                "subprocess".into(),
                "shutil".into(),
                "socket".into(),
                "ftplib".into(),
                "telnetlib".into(),
                "pickle".into(),
                "marshal".into(),
                "importlib".into(),
            ],
            safe_modules: vec![
                "math".into(),
                "json".into(),
                "re".into(),
                "datetime".into(),
                "collections".into(),
                "itertools".into(),
                "statistics".into(),
                "decimal".into(),
                "fractions".into(),
                "random".into(),
                "string".into(),
                "textwrap".into(),
                "urllib".into(),
                "memory".into(),
                "uiauto".into(), // UI Automation module
            ],
        }
    }
}

/// Memory storage format
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum StorageFormat {
    /// JSON format (human-readable, easy to debug)
    Json,
    /// Binary format using postcard (compact, faster, more secure)
    #[default]
    Binary,
}

/// Memory configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryConfig {
    /// Path to memory storage
    #[serde(default = "default_memory_path")]
    pub storage_path: String,
    /// Maximum number of conversations to keep
    #[serde(default = "default_max_conversations")]
    pub max_conversations: usize,
    /// Maximum short-term memories
    #[serde(default = "default_max_short_term")]
    pub max_short_term: usize,
    /// Maximum long-term memories
    #[serde(default = "default_max_long_term")]
    pub max_long_term: usize,
    /// Enable automatic memory consolidation
    #[serde(default = "default_auto_consolidation")]
    pub auto_consolidation: bool,
    /// Storage format (json or binary)
    #[serde(default)]
    pub storage_format: StorageFormat,
    /// Time decay rate (per day)
    #[serde(default = "default_time_decay_rate")]
    pub time_decay_rate: f64,
    /// Access count decay rate (per day)
    #[serde(default = "default_access_decay_rate")]
    pub access_decay_rate: f64,
    /// Minimum importance threshold
    #[serde(default = "default_min_importance")]
    pub min_importance: f64,
    /// Embedding model configuration
    #[serde(default)]
    pub embedding: EmbeddingConfig,
}

/// Embedding model configuration for semantic search
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingConfig {
    /// Enable embedding model for semantic search
    #[serde(default = "default_embedding_enabled")]
    pub enabled: bool,
    /// Embedding model name (fastembed model identifier)
    /// Examples: "BAAI/bge-small-en-v1.5", "BAAI/bge-base-en-v1.5", "sentence-transformers/all-MiniLM-L6-v2"
    #[serde(default = "default_embedding_model")]
    pub model: String,
    /// HuggingFace mirror URL for model downloads (optional)
    /// Examples: "https://huggingface.co", "https://hf-mirror.com"
    /// If not set, will use HF_ENDPOINT env var or fastembed default
    #[serde(default)]
    pub hf_endpoint: Option<String>,
    /// Local cache directory for model files (optional)
    #[serde(default = "default_embedding_cache_dir")]
    pub cache_dir: Option<String>,
}

impl Default for MemoryConfig {
    fn default() -> Self {
        Self {
            storage_path: "./data/memory".into(),
            max_conversations: 100,
            max_short_term: 50,
            max_long_term: 1000,
            auto_consolidation: true,
            storage_format: StorageFormat::default(),
            time_decay_rate: 0.05,
            access_decay_rate: 0.1,
            min_importance: 0.1,
            embedding: EmbeddingConfig::default(),
        }
    }
}

impl Default for EmbeddingConfig {
    fn default() -> Self {
        Self {
            enabled: default_embedding_enabled(),
            model: default_embedding_model(),
            hf_endpoint: None,
            cache_dir: default_embedding_cache_dir(),
        }
    }
}

/// System configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemConfig {
    /// System prompt for the AI assistant
    pub system_prompt: String,
    /// Maximum context length
    #[serde(default = "default_max_context")]
    pub max_context_length: usize,
    /// Enable thinking mode (show LLM's thinking process)
    #[serde(default = "default_thinking_mode")]
    pub thinking_mode: bool,
}

impl Default for SystemConfig {
    fn default() -> Self {
        Self {
            system_prompt: DEFAULT_SYSTEM_PROMPT.to_string(),
            max_context_length: default_max_context(),
            thinking_mode: default_thinking_mode(),
        }
    }
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            provider: "zhipu".to_string(),
            model: "glm-5".to_string(),
            api_key: None,
            base_url: None,
            timeout_secs: default_timeout(),
            max_retries: default_max_retries(),
            secret_id: None,
            secret_key: None,
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            llm: LlmConfig::default(),
            python: PythonConfig::default(),
            memory: MemoryConfig::default(),
            system: SystemConfig::default(),
        }
    }
}

impl Config {
    /// Load configuration from a TOML file
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref();
        let content = fs::read_to_string(path)
            .with_context(|| format!("Failed to read config file: {}", path.display()))?;

        let mut config: Config = toml::from_str(&content)
            .with_context(|| format!("Failed to parse TOML config: {}", path.display()))?;

        // Load API key from environment if not specified in config
        if config.llm.api_key.is_none() {
            let env_key = format!("{}_API_KEY", config.llm.provider.to_uppercase());
            if let Ok(api_key) = std::env::var(&env_key) {
                config.llm.api_key = Some(api_key);
                tracing::info!("Loaded API key from environment variable: {}", env_key);
            }
        }

        // Load Tencent-specific credentials from environment
        if config.llm.provider == "tencent" {
            if config.llm.secret_id.is_none() {
                if let Ok(secret_id) = std::env::var("TENCENT_SECRET_ID") {
                    config.llm.secret_id = Some(secret_id);
                    tracing::info!("Loaded Tencent secret_id from environment variable");
                }
            }
            if config.llm.secret_key.is_none() {
                if let Ok(secret_key) = std::env::var("TENCENT_SECRET_KEY") {
                    config.llm.secret_key = Some(secret_key);
                    tracing::info!("Loaded Tencent secret_key from environment variable");
                }
            }
        }

        Ok(config)
    }

    /// Get the API key (returns error if not configured)
    pub fn get_api_key(&self) -> Result<String> {
        self.llm.api_key.clone().ok_or_else(|| {
            anyhow::anyhow!(
                "API key not configured. Set it in config.toml or {}_API_KEY environment variable",
                self.llm.provider.to_uppercase()
            )
        })
    }

    /// Get Tencent secret_id (returns error if not configured)
    pub fn get_tencent_secret_id(&self) -> Result<String> {
        self.llm.secret_id.clone()
            .ok_or_else(|| anyhow::anyhow!(
                "Tencent secret_id not configured. Set it in config.toml or TENCENT_SECRET_ID environment variable"
            ))
    }

    /// Get Tencent secret_key (returns error if not configured)
    pub fn get_tencent_secret_key(&self) -> Result<String> {
        self.llm.secret_key.clone()
            .ok_or_else(|| anyhow::anyhow!(
                "Tencent secret_key not configured. Set it in config.toml or TENCENT_SECRET_KEY environment variable"
            ))
    }

    /// Save configuration to a TOML file
    pub fn save<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let path = path.as_ref();

        // Create parent directory if needed
        if let Some(parent) = path.parent() {
            if !parent.exists() {
                fs::create_dir_all(parent)
                    .with_context(|| format!("Failed to create directory: {}", parent.display()))?;
            }
        }

        // Generate TOML with comments
        let content = self.to_toml_with_comments()?;

        let mut file = fs::File::create(path)
            .with_context(|| format!("Failed to create config file: {}", path.display()))?;
        file.write_all(content.as_bytes())
            .with_context(|| format!("Failed to write config file: {}", path.display()))?;

        Ok(())
    }

    /// Convert configuration to TOML with helpful comments
    fn to_toml_with_comments(&self) -> Result<String> {
        let mut output = String::new();

        // LLM section
        output.push_str("# nb-claw Configuration File\n\n");
        output.push_str("# LLM Provider Configuration\n");
        output.push_str("# Available providers: openai, anthropic, google, longcat, moonshot, zhipu, aliyun, ollama, deepseek, xiaomi, volcengine, tencent\n");
        output.push_str("[llm]\n");
        output.push_str(&format!("provider = \"{}\"\n", self.llm.provider));
        output.push_str(&format!("model = \"{}\"\n", self.llm.model));

        if let Some(api_key) = &self.llm.api_key {
            output.push_str(&format!("api_key = \"{}\"\n", api_key));
        } else {
            output.push_str("# api_key = \"your-api-key\"  # Or set via environment variable\n");
        }

        if let Some(base_url) = &self.llm.base_url {
            output.push_str(&format!("base_url = \"{}\"\n", base_url));
        } else {
            output.push_str("# base_url = \"\"  # Optional: override default endpoint\n");
        }

        output.push_str(&format!("timeout_secs = {}\n", self.llm.timeout_secs));
        output.push_str(&format!("max_retries = {}\n", self.llm.max_retries));

        if self.llm.provider == "tencent" {
            output.push_str("\n# Tencent-specific credentials\n");
            if let Some(secret_id) = &self.llm.secret_id {
                output.push_str(&format!("secret_id = \"{}\"\n", secret_id));
            } else {
                output.push_str("# secret_id = \"\"  # Or set TENCENT_SECRET_ID env var\n");
            }
            if let Some(secret_key) = &self.llm.secret_key {
                output.push_str(&format!("secret_key = \"{}\"\n", secret_key));
            } else {
                output.push_str("# secret_key = \"\"  # Or set TENCENT_SECRET_KEY env var\n");
            }
        }

        // Python section
        output.push_str("\n# Python Interpreter Configuration\n");
        output.push_str("[python]\n");
        output.push_str(&format!("sandbox = {}\n", self.python.sandbox));
        output.push_str(&format!(
            "timeout_secs = {}\n",
            self.python.max_execution_time
        ));
        output.push_str("# dangerous_modules and safe_modules use defaults\n");

        // Memory section
        output.push_str("\n# Memory System Configuration\n");
        output.push_str("[memory]\n");
        output.push_str(&format!(
            "storage_path = \"{}\"\n",
            self.memory.storage_path
        ));
        output.push_str(&format!(
            "max_conversations = {}\n",
            self.memory.max_conversations
        ));
        output.push_str(&format!(
            "max_short_term = {}\n",
            self.memory.max_short_term
        ));
        output.push_str(&format!("max_long_term = {}\n", self.memory.max_long_term));
        output.push_str(&format!(
            "auto_consolidation = {}\n",
            self.memory.auto_consolidation
        ));
        output.push_str(&format!(
            "storage_format = \"{}\"  # json or binary\n",
            match self.memory.storage_format {
                StorageFormat::Json => "json",
                StorageFormat::Binary => "binary",
            }
        ));

        // Embedding section
        output.push_str("\n# Embedding Model Configuration (for semantic search)\n");
        output.push_str("[memory.embedding]\n");
        output.push_str(&format!("enabled = {}\n", self.memory.embedding.enabled));
        output.push_str(&format!("model = \"{}\"\n", self.memory.embedding.model));

        if let Some(hf_endpoint) = &self.memory.embedding.hf_endpoint {
            output.push_str(&format!("hf_endpoint = \"{}\"\n", hf_endpoint));
        } else {
            output.push_str("# hf_endpoint = \"https://hf-mirror.com\"  # For users in China\n");
        }

        // System section
        output.push_str("\n# System Configuration\n");
        output.push_str("[system]\n");
        output.push_str("system_prompt = \"\"\"\n");
        output.push_str(&self.system.system_prompt);
        output.push_str("\n\"\"\"\n");
        output.push_str(&format!(
            "max_context_length = {}\n",
            self.system.max_context_length
        ));
        output.push_str(&format!("thinking_mode = {}\n", self.system.thinking_mode));

        Ok(output)
    }
}

// Default values
fn default_timeout() -> u64 {
    60
}
fn default_max_retries() -> u32 {
    3
}
fn default_sandbox() -> bool {
    true
}
fn default_execution_timeout() -> u64 {
    30
}
fn default_dangerous_modules() -> Vec<String> {
    vec![
        "os".into(),
        "subprocess".into(),
        "shutil".into(),
        "socket".into(),
        "http".into(),
        "ftplib".into(),
        "telnetlib".into(),
        "pickle".into(),
        "marshal".into(),
        "importlib".into(),
    ]
}
fn default_safe_modules() -> Vec<String> {
    vec![
        "math".into(),
        "json".into(),
        "re".into(),
        "datetime".into(),
        "collections".into(),
        "itertools".into(),
        "statistics".into(),
        "decimal".into(),
        "fractions".into(),
        "random".into(),
        "string".into(),
        "textwrap".into(),
        "urllib".into(),
        "memory".into(),
        "uiauto".into(), // UI Automation module
    ]
}
fn default_memory_path() -> String {
    "./data/memory".into()
}
fn default_max_conversations() -> usize {
    100
}
fn default_max_short_term() -> usize {
    50
}
fn default_max_long_term() -> usize {
    1000
}
fn default_auto_consolidation() -> bool {
    true
}
fn default_time_decay_rate() -> f64 {
    0.05
}
fn default_access_decay_rate() -> f64 {
    0.1
}
fn default_min_importance() -> f64 {
    0.1
}
fn default_embedding_enabled() -> bool {
    true
}
fn default_embedding_model() -> String {
    "BAAI/bge-m3".to_string()
}
fn default_embedding_cache_dir() -> Option<String> {
    default_memory_path().into()
}
fn default_max_context() -> usize {
    16000
}
fn default_thinking_mode() -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_values() {
        assert_eq!(default_timeout(), 60);
        assert_eq!(default_max_retries(), 3);
        assert!(default_sandbox());
    }

    #[test]
    fn test_config_parse() {
        let toml_str = r#"
            [llm]
            provider = "openai"
            model = "gpt-4"
            api_key = "sk-test"
            timeout_secs = 90

            [python]
            sandbox = false

            [memory]
            storage_path = "/tmp/memory"

            [system]
            system_prompt = "You are a helpful assistant"
            max_context_length = 32000
        "#;

        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.llm.provider, "openai");
        assert_eq!(config.llm.model, "gpt-4");
        assert_eq!(config.llm.api_key, Some("sk-test".to_string()));
        assert_eq!(config.llm.timeout_secs, 90);
        assert!(!config.python.sandbox);
        assert_eq!(config.memory.storage_path, "/tmp/memory");
        assert_eq!(config.system.max_context_length, 32000);
    }
}
