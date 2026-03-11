//! nb-claw: An autonomous AI assistant with planning and execution capabilities
//!
//! This is a Rust-based AI assistant that embeds a Python interpreter,
//! allowing the model to control the device through Python script execution.

mod config;
mod llm;
mod memory;
mod python;
mod uiauto;

use {
    clap::Parser,
    config::{Config, EMBEDDING_MODELS, LLM_PROVIDERS},
    futures_util::{StreamExt, pin_mut},
    llm::LlmManager,
    memory::Memory,
    python::PythonInterpreter,
    serde_json::json,
    std::{
        io::{self, BufRead, Write},
        path::Path,
        sync::{Arc, RwLock},
    },
    tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, BufWriter, stdin, stdout},
    tracing::{error, info},
    tracing_subscriber::{EnvFilter, fmt},
};

/// nb-claw: An autonomous AI assistant
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Path to configuration file
    #[arg(short, long)]
    config: Option<String>,

    /// Enable debug logging
    #[arg(short, long)]
    debug: bool,

    /// Run in test mode (no interactive loop)
    #[arg(long)]
    test: bool,

    /// Initialize config file with default values
    #[arg(long, value_name = "PATH")]
    init_config: Option<Option<String>>,

    /// Run interactive configuration wizard
    #[arg(long)]
    config_wizard: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    // Initialize logging
    let filter = EnvFilter::new(if args.debug {
        "debug"
    } else if cfg!(debug_assertions) {
        "info"
    } else {
        "error"
    });
    fmt().with_env_filter(filter).init();

    // Handle --init-config
    if let Some(path) = args.init_config {
        let config_path = path.unwrap_or_else(|| "config/config.toml".to_string());
        return init_config_with_defaults(&config_path);
    }

    // Handle --config-wizard
    if args.config_wizard {
        return run_config_wizard().await;
    }

    info!("🦀 nb-claw starting...");

    // Load configuration
    let config_path = args
        .config
        .unwrap_or_else(|| "config/config.toml".to_string());
    let config = Config::load(&config_path)?;
    info!("Loaded configuration from {}", config_path);
    let memory = Arc::new(RwLock::new(Memory::new(config.memory.clone())?));
    info!("Memory engine initialized");

    // Create LLM manager with memory engine
    let mut llm_manager = LlmManager::new(&config)?;
    llm_manager.set_memory(memory.clone());
    info!("LLM client initialized: {}", llm_manager.provider_name());

    // Create memory manager for Python
    let memory_module = memory::create_memory_module(Arc::downgrade(&memory))?;
    // Create and register UI automation module
    let uiauto_module = uiauto::create_uiauto_module()?;
    // Register memory module
    PythonInterpreter::register_module_global(memory_module);
    PythonInterpreter::register_module_global(uiauto_module);

    // Test mode
    if args.test {
        info!("Running in test mode...");
        run_test(&llm_manager).await?;
        return Ok(());
    }

    // Interactive mode
    info!("✓ nb-claw initialized successfully");
    info!("  - Provider: {}", config.llm.provider);
    info!("  - Model: {}", config.llm.model);

    run_interactive(&llm_manager).await?;

    Ok(())
}

/// Initialize config file with default values
fn init_config_with_defaults(path: &str) -> anyhow::Result<()> {
    let config_path = Path::new(path);

    // Check if file already exists
    if config_path.exists() {
        println!("Configuration file already exists: {}", path);
        println!();
        print!("Overwrite existing file? [y/N]: ");
        io::stdout().flush()?;

        let stdin = io::stdin();
        let mut input = String::new();
        stdin.lock().read_line(&mut input)?;
        let input = input.trim().to_lowercase();

        if input != "y" && input != "yes" {
            println!("Operation cancelled.");
            return Ok(());
        }
    }

    let config = Config::default();
    config.save(path)?;
    println!();
    println!("✓ Configuration file created: {}", path);
    println!();
    println!("Next steps:");
    println!("  1. Edit the config file to set your LLM provider and API key");
    println!("  2. Run 'nb-claw' to start the assistant");
    println!();
    println!("For interactive configuration, run: nb-claw --config-wizard");
    Ok(())
}

//noinspection SpellCheckingInspection
/// Run interactive configuration wizard
async fn run_config_wizard() -> anyhow::Result<()> {
    println!("╔══════════════════════════════════════════════════════════╗");
    println!("║           nb-claw Configuration Wizard                   ║");
    println!("╚══════════════════════════════════════════════════════════╝");
    println!();

    let mut std_in = BufReader::new(stdin());
    let mut config = Config::default();

    // Step 1: Select LLM provider
    println!("Step 1: Select LLM Provider");
    println!();
    for (i, (provider, default_model, _url)) in LLM_PROVIDERS.iter().enumerate() {
        println!(
            "  [{}] {} (default model: {})",
            i + 1,
            provider,
            default_model
        );
    }
    println!();

    let provider_index = prompt_select("Select provider", 1, LLM_PROVIDERS.len())? - 1;
    let (provider, default_model, _) = LLM_PROVIDERS[provider_index];
    config.llm.provider = provider.to_string();
    config.llm.model = default_model.to_string();

    println!();

    // Step 2: Model name
    println!("Step 2: Model Name");
    println!("  Default: {}", default_model);
    let model = prompt_string_optional("Enter model name (press Enter for default)");
    if let Some(m) = model {
        config.llm.model = m;
    }
    println!();

    // Step 3: API Key
    println!("Step 3: API Key");
    println!(
        "  (You can also set it via environment variable: {}_API_KEY)",
        provider.to_uppercase()
    );
    let api_key = prompt_string_optional("Enter API key (press Enter to skip)");
    config.llm.api_key = api_key;
    println!();

    // Step 4: Tencent credentials if needed
    if provider == "tencent" {
        println!("Step 3b: Tencent Credentials");
        let secret_id = prompt_string_optional("Enter TENCENT_SECRET_ID (press Enter to skip)");
        config.llm.secret_id = secret_id;
        let secret_key = prompt_string_optional("Enter TENCENT_SECRET_KEY (press Enter to skip)");
        config.llm.secret_key = secret_key;
        println!();
    }

    // Step 5: Embedding model
    println!("Step 4: Embedding Model (for semantic search)");
    println!("  [1] Skip (disable embedding)");
    println!("  [2] Use default (BAAI/bge-m3)");
    println!("  [3] Select from list");
    println!();

    let embedding_choice = prompt_select("Select option", 1, 3)?;
    match embedding_choice {
        1 => {
            config.memory.embedding.enabled = false;
        }
        2 => {
            config.memory.embedding.enabled = true;
        }
        3 => {
            config.memory.embedding.enabled = true;
            println!();
            println!("Available embedding models:");
            println!();

            // Group models
            let mut current_group = "";
            for (i, (model, dim, desc)) in EMBEDDING_MODELS.iter().enumerate() {
                // Detect group from model name
                let group = if model.starts_with("Xenova/bge") || model.starts_with("BAAI/bge") {
                    if model.contains("-zh-") {
                        "BGE Chinese"
                    } else if model.contains("-en-") {
                        "BGE English"
                    } else {
                        "BGE Multilingual"
                    }
                } else if model.starts_with("intfloat") {
                    "Multilingual E5"
                } else if model.contains("snowflake") {
                    "Snowflake Arctic"
                } else {
                    "Other"
                };

                if group != current_group {
                    println!("  --- {} ---", group);
                    current_group = group;
                }
                println!("  [{}] {} ({} dim) - {}", i + 1, model, dim, desc);
            }
            println!();

            let model_index = prompt_select("Select model", 1, EMBEDDING_MODELS.len())? - 1;
            config.memory.embedding.model = EMBEDDING_MODELS[model_index].0.to_string();
        }
        _ => {}
    }
    println!();

    // Step 6: HuggingFace mirror (for users in China)
    if config.memory.embedding.enabled {
        println!("Step 5: HuggingFace Mirror (for model downloads)");
        println!("  [1] Use default (https://huggingface.co)");
        println!("  [2] Use mirror (https://hf-mirror.com) - recommended for China");
        println!();

        let mirror_choice = prompt_select("Select option", 1, 2)?;
        if mirror_choice == 2 {
            config.memory.embedding.hf_endpoint = Some("https://hf-mirror.com".to_string());
        }
        println!();
    }

    // Step 7: Save location
    println!("Step 6: Save Configuration");
    println!("  Default: config/config.toml");
    let save_path = prompt_string_optional("Enter save path (press Enter for default)");
    let save_path = save_path.unwrap_or_else(|| "config/config.toml".to_string());
    println!();

    // Summary
    println!("Configuration Summary:");
    println!("  Provider: {}", config.llm.provider);
    println!("  Model: {}", config.llm.model);
    println!(
        "  API Key: {}",
        if config.llm.api_key.is_some() {
            "configured"
        } else {
            "not set"
        }
    );
    println!(
        "  Embedding: {}",
        if config.memory.embedding.enabled {
            &config.memory.embedding.model
        } else {
            "disabled"
        }
    );
    if let Some(ref endpoint) = config.memory.embedding.hf_endpoint {
        println!("  HF Mirror: {}", endpoint);
    }
    println!("  Save to: {}", save_path);
    println!();

    // Check if file exists and warn
    let file_exists = Path::new(&save_path).exists();
    if file_exists {
        println!("⚠ Configuration file already exists: {}", save_path);
        print!("Overwrite? [y/N]: ");
        io::stdout().flush()?;
        let mut overwrite = String::new();
        std_in.read_line(&mut overwrite).await?;
        let overwrite = overwrite.trim().to_lowercase();
        if overwrite != "y" && overwrite != "yes" {
            println!("Configuration cancelled.");
            return Ok(());
        }
        println!();
    }

    // Confirm
    print!("Save configuration? [Y/n]: ");
    io::stdout().flush()?;
    let mut confirm = String::new();
    std_in.read_line(&mut confirm).await?;
    let confirm = confirm.trim().to_lowercase();

    if confirm.is_empty() || confirm == "y" || confirm == "yes" {
        config.save(&save_path)?;
        println!();
        println!("✓ Configuration saved to: {}", save_path);
        println!();
        println!("You can now run 'nb-claw' to start the assistant.");
    } else {
        println!("Configuration cancelled.");
    }

    Ok(())
}

/// Prompt user to select from numbered options
fn prompt_select(prompt: &str, min: usize, max: usize) -> anyhow::Result<usize> {
    let stdin = io::stdin();

    loop {
        print!("{} [{}-{}]: ", prompt, min, max);
        io::stdout().flush()?;

        let mut input = String::new();
        stdin.lock().read_line(&mut input)?;
        let input = input.trim();

        match input.parse::<usize>() {
            Ok(n) if n >= min && n <= max => return Ok(n),
            _ => println!(
                "Invalid input. Please enter a number between {} and {}.",
                min, max
            ),
        }
    }
}

/// Prompt user for optional string input
fn prompt_string_optional(prompt: &str) -> Option<String> {
    let stdin = io::stdin();

    print!("{}: ", prompt);
    io::stdout().flush().ok()?;

    let mut input = String::new();
    stdin.lock().read_line(&mut input).ok()?;

    let input = input.trim();
    if input.is_empty() {
        None
    } else {
        Some(input.to_string())
    }
}

/// Run test mode
async fn run_test(llm_manager: &LlmManager) -> anyhow::Result<()> {
    info!("Testing Python interpreter...");
    let interpreter = llm_manager.tool_registry();

    // Test run_py tool
    let result = interpreter
        .execute_tool(
            "run_py",
            &json!({"code": "ret = 2 + 2"}),
            "test-1".to_string(),
        )
        .await;
    info!("run_py test result: {}", result.result);

    // Test py_mods tool
    let result = interpreter
        .execute_tool("py_mods", &json!({}), "test-2".to_string())
        .await;
    info!("py_mods test result: {}", result.result);

    // Test run_cmd tool
    let result = interpreter
        .execute_tool(
            "run_cmd",
            &json!({"command": "echo test"}),
            "test-3".to_string(),
        )
        .await;
    info!("run_cmd test result: {}", result.result);

    Ok(())
}

/// Run interactive mode
async fn run_interactive(llm_manager: &LlmManager) -> anyhow::Result<()> {
    let mut std_in = BufReader::new(stdin());
    let mut std_out = BufWriter::new(stdout());
    std_out
        .write_all(
            "Starting interactive mode (type '/quit' to exit, '/help' to get help)\n\n".as_bytes(),
        )
        .await?;

    std_out
        .write_all("🤖 nb-claw AI Assistant\n".as_bytes())
        .await?;
    std_out
        .write_all("───────────────────────\n\n".as_bytes())
        .await?;

    loop {
        std_out.write_all("You: ".as_bytes()).await?;
        std_out.flush().await?;

        let mut input = String::new();
        std_in.read_line(&mut input).await?;

        let input = input.trim();

        if input.is_empty() {
            continue;
        }

        if input.eq_ignore_ascii_case("/quit") || input.eq_ignore_ascii_case("/exit") {
            info!("Exiting...");
            break;
        }

        if input.eq_ignore_ascii_case("/help") {
            print_help();
            continue;
        }

        // Get response from LLM (with tool calling support) - streaming version
        std_out.write_all("Assistant: ".as_bytes()).await?;
        std_out.flush().await?;

        match llm_manager.chat_stream(input).await {
            Ok(stream) => {
                pin_mut!(stream);
                while let Some(chunk) = stream.next().await {
                    match chunk {
                        Ok(content) => {
                            std_out.write_all(content.as_bytes()).await?;
                            std_out.flush().await?;
                        }
                        Err(e) => error!(?e, "Stream error"),
                    }
                }
                std_out.write_all(b"\n\n").await?;
            }
            Err(e) => {
                error!(?e, "Error getting response");
                std_out
                    .write_all(format!("Error: {}\n", e).as_bytes())
                    .await?;
            }
        }
    }

    Ok(())
}

/// Print help information
fn print_help() {
    println!("Available commands:");
    println!("  /quit or /exit   - Exit the assistant");
    println!("  /help        - Show this help message");
    println!();
}
