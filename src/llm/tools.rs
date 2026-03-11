//! Tool definitions for the AI assistant
//!
//! Defines the tools that the LLM can use, including:
//! - `run_py`: Execute Python code
//! - `py_mods`: List available Python modules
//! - `run_cmd`: Execute shell commands

use {
    crate::PythonInterpreter,
    encoding_rs::GBK,
    serde::{Deserialize, Serialize},
    serde_json::{Value, json},
    tracing::info,
};

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
}

impl ToolRegistry {
    /// Create a new tool registry
    pub fn new(interpreter: PythonInterpreter) -> Self {
        Self { interpreter }
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

Example with calculations:
a = sum(range(10))
b = a ** 2

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
            "run_cmd" => self.execute_run_cmd(arguments, call_id),
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

        tracing::debug!("Executing Python code: {}", code);

        match self.interpreter.execute(&code).await {
            Ok(result) => {
                let mut response = String::new();
                if !result.stdout.is_empty() {
                    response.push_str(&format!("STDOUT:\n{}\n", result.stdout));
                }
                if !result.stderr.is_empty() {
                    response.push_str(&format!("STDERR:\n{}\n", result.stderr));
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
    fn execute_run_cmd(&self, arguments: &Value, call_id: String) -> ToolCallResult {
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

        tracing::debug!("Executing shell command: {}", command);

        let output = if cfg!(target_os = "windows") {
            // On Windows, use cmd /c
            std::process::Command::new("cmd")
                .args(["/C", &command])
                .output()
        } else {
            // On Unix/Linux/Mac, use sh -c
            std::process::Command::new("sh")
                .args(["-c", &command])
                .output()
        };

        match output {
            Ok(output) => {
                let (stdout, stderr) = if cfg!(target_os = "windows") {
                    // On Windows, decode using GBK (CP936)
                    let (stdout, _, _) = GBK.decode(&output.stdout);
                    let (stderr, _, _) = GBK.decode(&output.stderr);
                    (stdout.into_owned(), stderr.into_owned())
                } else {
                    (
                        String::from_utf8_lossy(&output.stdout).to_string(),
                        String::from_utf8_lossy(&output.stderr).to_string(),
                    )
                };
                let exit_code = output.status.code();

                let mut response = String::new();
                if !stdout.is_empty() {
                    response.push_str(&format!("STDOUT:\n{}\n", stdout));
                }
                if !stderr.is_empty() {
                    response.push_str(&format!("STDERR:\n{}\n", stderr));
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
