//! Tool definitions for the AI assistant
//!
//! Defines the tools that the LLM can use, including:
//! - `run_py`: Execute Python code
//! - `py_mods`: List available Python modules
//! - `run_cmd`: Execute shell commands

use {
    crate::{
        python::PythonInterpreter,
        vcs::{VcsEngine, extract_paths},
    },
    encoding_rs::GBK,
    serde::{Deserialize, Serialize},
    serde_json::{Value, json},
    std::{
        path::{Path, PathBuf},
        sync::Weak,
    },
    tokio::process::Command,
    tracing::{debug, info},
};

/// Maximum length for stdout/stderr output (keep tail)
const MAX_OUTPUT_LEN: usize = 4000;

/// Truncate output to keep the tail (most recent output is more important)
fn truncate_output(output: &str, max_len: usize) -> String {
    if output.len() <= max_len {
        output.to_string()
    } else {
        let skip = output.len() - max_len;
        // Find valid UTF-8 boundary at or after skip position
        // by finding the first char that starts at or after skip
        let boundary = output
            .char_indices()
            .find(|(idx, _)| *idx >= skip)
            .map(|(idx, _)| idx)
            .unwrap_or(skip);
        format!("<truncated {} bytes>...\n{}", boundary, &output[boundary..])
    }
}

/// Result of a tool call
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallResult {
    /// Tool name
    pub tool_name: String,
    /// Tool call ID
    pub call_id: String,
    /// Execution result
    pub result: String,
    /// Whether the call succeeded
    pub success: bool,
}

/// Tool definition
#[derive(Debug, Clone)]
pub struct ToolDefinition {
    /// Tool name
    pub name: String,
    /// Tool description
    pub description: String,
    /// Tool parameters schema (JSON Schema)
    pub parameters: Value,
}

impl ToolDefinition {
    /// Convert to llm-connector Tool format
    pub fn to_llm_tool(&self) -> llm_connector::types::Tool {
        llm_connector::types::Tool::function(
            self.name.clone(),
            Some(self.description.clone()),
            self.parameters.clone(),
        )
    }
}

/// Tool registry
pub struct ToolRegistry {
    interpreter: PythonInterpreter,
    vcs: Option<Weak<VcsEngine>>,
}

impl ToolRegistry {
    /// Create a new tool registry
    pub fn new(interpreter: PythonInterpreter, vcs: Option<Weak<VcsEngine>>) -> Self {
        Self { interpreter, vcs }
    }

    /// Auto-track files from code/command
    fn auto_track_files(&self, content: &str, is_python: bool) {
        if let Some(ref vcs) = self.vcs
            && let Some(vcs) = vcs.upgrade()
        {
            // Check if auto_track is enabled
            if !vcs.config().auto_track {
                return;
            }

            let paths = extract_paths(content, is_python);
            if !paths.is_empty() {
                info!(
                    "Detected {} file paths in {}",
                    paths.len(),
                    if is_python { "Python" } else { "CMD" }
                );

                // Collect existing files
                let existing_paths: Vec<&Path> = paths
                    .iter()
                    .filter(|p| PathBuf::from(p).exists())
                    .map(|p| Path::new(p))
                    .collect();

                if existing_paths.is_empty() {
                    debug!("No existing files to snapshot");
                    return;
                }

                // Generate meaningful commit message
                let msg = Self::generate_commit_message(content, &paths, is_python);

                match vcs.create_snapshot(&msg, &existing_paths) {
                    Ok(id) if !id.is_empty() => {
                        info!("Created snapshot [{}]: {}", &id[..7.min(id.len())], msg);
                    }
                    Ok(_) => {
                        debug!("No changes detected, skipped snapshot");
                    }
                    Err(e) => debug!("Snapshot error: {}", e),
                }
            }
        }
    }

    /// Generate a commit message from code/command content by extracting context around paths
    fn generate_commit_message(content: &str, paths: &[String], _is_python: bool) -> String {
        let lines: Vec<&str> = content.lines().collect();
        let total_lines = lines.len();

        // Find the first line that contains any of the paths
        let mut target_line = None;
        for (line_idx, line) in lines.iter().enumerate() {
            for path in paths {
                // Check if the line contains the path (handle both forward and backslash)
                let path_normalized = path.replace('\\', "/");
                let line_normalized = line.replace('\\', "/");
                if line_normalized.contains(&path_normalized)
                    || line.contains(path)
                    || line.contains(&path.replace('/', "\\"))
                {
                    target_line = Some(line_idx);
                    break;
                }
            }
            if target_line.is_some() {
                break;
            }
        }

        // If no path found in content, use the first meaningful line
        let target_line = target_line.unwrap_or_else(|| {
            // Find first non-empty, non-comment line
            lines
                .iter()
                .position(|l| {
                    let trimmed = l.trim();
                    !trimmed.is_empty() && !trimmed.starts_with('#')
                })
                .unwrap_or(0)
        });

        // Extract context: 5 lines before and after (or as many as available)
        let context_before = 5;
        let context_after = 5;

        let start_line = target_line.saturating_sub(context_before);
        let end_line = (target_line + context_after + 1).min(total_lines);

        // Build the message from context lines
        let context_lines: Vec<&str> = lines[start_line..end_line].to_vec();
        let msg = context_lines.join(" \\n ");

        // Truncate if too long (git commit message convention: ~72 chars for title)
        // Use chars() to handle UTF-8 boundaries properly
        let msg = if msg.chars().count() > 100 {
            format!("{}...", msg.chars().take(97).collect::<String>())
        } else if msg.trim().is_empty() {
            "Auto-snapshot".to_string()
        } else {
            msg
        };

        msg
    }

    /// Get all available tools
    pub fn get_all_tools(&self) -> Vec<ToolDefinition> {
        vec![
            ToolDefinition {
                name: "run_py".to_string(),
                description: r#"Execute Python code using only built-in modules (no third-party packages). 

Key usage guidelines:
- Multi-line code is fully supported
- All defined variables will be returned as results automatically
- Do not add comments or explanations in the code, just the code itself

Example usage:
x = 2 + 2
y = x * 3
print(f"Result: {y}")

The tool returns STDOUT, STDERR, and all defined variables as results."#.into(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "code": {
                            "type": "string",
                            "description": "Python code to execute. All defined variables will be automatically returned as results."
                        }
                    },
                    "required": ["code"]
                }),
            },
            ToolDefinition {
                name: "py_mods".to_string(),
                description: r#"List all available Python built-in modules that can be used in the `run_py` tool.

This tool helps you discover which Python modules are available before executing code. It returns a comma-separated list of module names.
You can call `help(module_name)` for a module while running the python code to view specific help instructions.

Use this tool when you need to:
- Check if a specific module is available
- Discover what built-in modules can be imported
- Plan your Python code before execution"#.into(),
                parameters: json!({
                    "type": "object",
                    "properties": {},
                    "required": []
                }),
            },
            ToolDefinition {
                name: "run_cmd".to_string(),
                description: r#"Execute shell commands to interact with the operating system. Returns the command's stdout, stderr output, and exit code.

Platform-specific usage:
- On Windows: Use Windows commands (e.g., dir, type, del, copy, echo, etc.)
- On Unix/Linux/Mac: Use Unix commands (e.g., ls, cat, rm, cp, echo, etc.)

The tool returns:
- STDOUT: The standard output of the command
- STDERR: Any error messages
- EXIT CODE: The exit status (0 = success, non-zero = error)

Example usage on Unix:
ls -la

Example usage on Windows:
dir /A"#.to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "command": {
                            "type": "string",
                            "description": "The shell command to execute."
                        }
                    },
                    "required": ["command"]
                }),
            },
        ]
    }

    /// Execute a tool call
    pub async fn execute_tool(
        &self,
        tool_name: &str,
        arguments: &Value,
        call_id: String,
    ) -> ToolCallResult {
        info!("Executing tool: {} {}", tool_name, arguments);
        match tool_name {
            "run_py" => self.execute_run_py(arguments, call_id).await,
            "py_mods" => self.execute_py_mods(arguments, call_id),
            "run_cmd" => self.execute_run_cmd(arguments, call_id).await,
            _ => ToolCallResult {
                tool_name: tool_name.to_string(),
                call_id,
                result: format!("Unknown tool: {}", tool_name),
                success: false,
            },
        }
    }

    /// Execute Python code (run_py tool)
    async fn execute_run_py(&self, arguments: &Value, call_id: String) -> ToolCallResult {
        let code = match arguments.get("code") {
            Some(Value::String(s)) => s.clone(),
            _ => {
                return ToolCallResult {
                    tool_name: "run_py".to_string(),
                    call_id,
                    result: "Missing or invalid 'code' parameter".to_string(),
                    success: false,
                };
            }
        };

        // Auto-track files before execution
        self.auto_track_files(&code, true);

        debug!("Executing Python code: {}", code);

        match self.interpreter.execute(&code).await {
            Ok(result) => {
                let mut response = String::new();
                if !result.stdout.is_empty() {
                    response.push_str(&format!(
                        "STDOUT:\n{}\n",
                        truncate_output(&result.stdout, MAX_OUTPUT_LEN)
                    ));
                }
                if !result.stderr.is_empty() {
                    response.push_str(&format!(
                        "STDERR:\n{}\n",
                        truncate_output(&result.stderr, MAX_OUTPUT_LEN)
                    ));
                }
                if let Some(vars) = result.vars {
                    response.push_str(&format!("VARS:\n{}", vars));
                }

                ToolCallResult {
                    tool_name: "run_py".to_string(),
                    call_id,
                    result: response.trim().to_string(),
                    success: result.success,
                }
            }
            Err(e) => ToolCallResult {
                tool_name: "run_py".to_string(),
                call_id,
                result: format!("Execution error: {}", e),
                success: false,
            },
        }
    }

    /// List available modules (py_mods tool)
    fn execute_py_mods(&self, _arguments: &Value, call_id: String) -> ToolCallResult {
        let modules = self.interpreter.list_modules();

        let response = if modules.is_empty() {
            "No modules available".to_string()
        } else {
            format!("Available Python built-in modules:\n{}", modules.join(", "))
        };

        ToolCallResult {
            tool_name: "py_mods".to_string(),
            call_id,
            result: response,
            success: true,
        }
    }

    /// Execute shell command (run_cmd tool)
    async fn execute_run_cmd(&self, arguments: &Value, call_id: String) -> ToolCallResult {
        let command = match arguments.get("command") {
            Some(Value::String(s)) => s.clone(),
            _ => {
                return ToolCallResult {
                    tool_name: "run_cmd".to_string(),
                    call_id,
                    result: "Missing or invalid 'command' parameter".to_string(),
                    success: false,
                };
            }
        };

        // Auto-track files before execution
        self.auto_track_files(&command, false);

        debug!("Executing shell command: {}", command);

        let output = if cfg!(target_os = "windows") {
            // On Windows, use cmd /c
            Command::new("cmd").args(["/C", &command]).output()
        } else {
            // On Unix/Linux/Mac, use sh -c
            Command::new("sh").args(["-c", &command]).output()
        };

        match output.await {
            Ok(output) => {
                let (stdout, stderr) = if cfg!(target_os = "windows") {
                    // On Windows, try UTF-8 first, fallback to GBK
                    fn decode_windows(data: &[u8]) -> String {
                        // Try UTF-8 first (Python outputs UTF-8 by default)
                        if let Ok(s) = String::from_utf8(data.to_vec()) {
                            return s;
                        }
                        // Fallback to GBK (CP936) for legacy commands
                        let (decoded, _, had_errors) = GBK.decode(data);
                        if had_errors {
                            // If GBK also fails, use lossy UTF-8
                            String::from_utf8_lossy(data).to_string()
                        } else {
                            decoded.into_owned()
                        }
                    }
                    (
                        decode_windows(&output.stdout),
                        decode_windows(&output.stderr),
                    )
                } else {
                    (
                        String::from_utf8_lossy(&output.stdout).to_string(),
                        String::from_utf8_lossy(&output.stderr).to_string(),
                    )
                };
                let exit_code = output.status.code();

                let mut response = String::new();
                if !stdout.is_empty() {
                    response.push_str(&format!(
                        "STDOUT:\n{}\n",
                        truncate_output(&stdout, MAX_OUTPUT_LEN)
                    ));
                }
                if !stderr.is_empty() {
                    response.push_str(&format!(
                        "STDERR:\n{}\n",
                        truncate_output(&stderr, MAX_OUTPUT_LEN)
                    ));
                }
                match exit_code {
                    Some(code) => response.push_str(&format!("EXIT CODE: {}", code)),
                    None => response.push_str("EXIT CODE: <terminated by signal>"),
                }

                ToolCallResult {
                    tool_name: "run_cmd".to_string(),
                    call_id,
                    result: response.trim().to_string(),
                    success: output.status.success(),
                }
            }
            Err(e) => ToolCallResult {
                tool_name: "run_cmd".to_string(),
                call_id,
                result: format!("Failed to execute command: {}", e),
                success: false,
            },
        }
    }
}
