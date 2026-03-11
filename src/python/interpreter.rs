//! Embedded Python interpreter

use {
    super::Module,
    crate::config::PythonConfig,
    anyhow::Result,
    pyo3::{PyResult, prelude::*, types::PyDict},
    std::{
        ffi::CString,
        thread::spawn,
        time::{Duration, Instant},
    },
    tokio::{sync::oneshot, time::timeout},
    tracing::{debug, error, warn},
};

/// Result of Python script execution
#[derive(Debug, Clone)]
pub struct ExecutionResult {
    /// Standard output
    pub stdout: String,
    /// Standard error
    pub stderr: String,
    /// Variables returned by script (if any)
    pub vars: Option<String>,
    /// Whether execution succeeded
    pub success: bool,
    /// Execution time in milliseconds (if timed out, will be None)
    pub execution_time_ms: Option<u64>,
}

impl ExecutionResult {
    /// Create a successful execution result
    pub fn success(stdout: String, vars: Option<String>, execution_time_ms: u64) -> Self {
        Self {
            stdout,
            stderr: Default::default(),
            vars,
            success: true,
            execution_time_ms: Some(execution_time_ms),
        }
    }

    /// Create a failed execution result
    pub fn failure(stderr: String) -> Self {
        Self {
            stdout: Default::default(),
            stderr,
            vars: None,
            success: false,
            execution_time_ms: None,
        }
    }

    /// Create a timeout result
    pub fn timeout() -> Self {
        Self {
            stdout: Default::default(),
            stderr: "Execution timed out".into(),
            vars: None,
            success: false,
            execution_time_ms: None,
        }
    }
}

/// Embedded Python interpreter
pub struct PythonInterpreter {
    config: PythonConfig,
}

impl PythonInterpreter {
    /// Create a new Python interpreter with memory support
    pub fn new(config: PythonConfig) -> Result<Self> {
        debug!(
            "Python interpreter created with config: sandbox={}, max_time={}s, memory=enabled",
            config.sandbox, config.max_execution_time
        );

        Ok(Self { config })
    }

    /// Register a module globally in `sys.modules`
    pub fn register_module_global<M>(module: M)
    where
        M: for<'a> Module<'a>,
    {
        Python::attach(|py| {
            if let Ok(sys) = py.import("sys")
                && let Ok(sys_modules) = sys.getattr("modules")
                && let Ok(modules_dict) = sys_modules.cast_into::<PyDict>()
            {
                let _ = modules_dict.set_item(M::get_name(), module);
                debug!(
                    "Module `{}` registered globally in sys.modules",
                    M::get_name()
                );
            }
        });
    }

    /// Execute a Python script and return the result with timeout
    pub async fn execute(&self, code: &str) -> Result<ExecutionResult> {
        // Check sandbox restrictions
        if self.config.sandbox {
            if let Err(e) = self.check_sandbox_restrictions(code) {
                warn!("Sandbox restriction violated: {}", e);
                return Ok(ExecutionResult::failure(e));
            }
        }

        let config = self.config.clone();
        let code = code.to_string();
        let max_execution_time = config.max_execution_time;

        let start_time = Instant::now();

        // For short timeouts, use thread-based timeout mechanism
        // For long timeouts or in test environment, execute directly
        // Use channels for timeout handling
        let (tx, rx) = oneshot::channel();

        // Spawn a thread to execute Python code
        spawn(move || {
            let result = Self::execute_sync(&code);
            // Send result, ignore errors if receiver is dropped
            let _ = tx.send(result);
        });

        // Wait for execution with timeout
        let timeout_duration = Duration::from_secs(max_execution_time);
        let result = match timeout(timeout_duration, rx).await {
            Ok(Ok(execution_result)) => execution_result,
            Ok(Err(e)) => {
                warn!(?e, "Python execution error");
                return Ok(ExecutionResult::failure(format!("Execution error: {}", e)));
            }
            Err(e) => {
                warn!(
                    ?e,
                    "Python execution timed out after {}s", max_execution_time
                );
                return Ok(ExecutionResult::timeout());
            }
        };

        let execution_time_ms = start_time.elapsed().as_millis() as u64;

        match result {
            Ok(mut execution_result) => {
                if execution_result.execution_time_ms.is_none() {
                    execution_result.execution_time_ms = Some(execution_time_ms);
                }
                Ok(execution_result)
            }
            Err(e) => {
                error!(?e, "Task failed");
                Ok(ExecutionResult::failure(format!("Execution error: {}", e)))
            }
        }
    }

    /// Check if code violates sandbox restrictions
    fn check_sandbox_restrictions(&self, code: &str) -> Result<(), String> {
        // Convert to lowercase for case-insensitive matching
        let code_lower = code.to_lowercase();

        // Check for dangerous import patterns
        for module in &self.config.dangerous_modules {
            let pattern = format!("import {}", module);
            let pattern_from = format!("from {} import", module);

            if code_lower.contains(&pattern) || code_lower.contains(&pattern_from) {
                return Err(format!(
                    "Sandbox mode: Module '{}' is not allowed. Safe modules: {:?}",
                    module, self.config.safe_modules
                ));
            }
        }

        // Check for __import__ function (which can bypass restrictions)
        if code_lower.contains("__import__") {
            return Err(
                "Sandbox mode: __import__ function is not allowed. Use import statements instead."
                    .to_string(),
            );
        }

        // Check for exec or eval (can be dangerous)
        if code_lower.contains("exec(") {
            return Err("Sandbox mode: exec() function is not allowed".to_string());
        }

        if code_lower.contains("eval(") && code_lower.contains("__") {
            // Allow safe eval, but warn about potential dangers
            debug!("Code contains eval() - this may be dangerous in sandbox mode");
        }

        Ok(())
    }

    /// Synchronous Python code execution
    fn execute_sync(code: &str) -> Result<ExecutionResult> {
        Python::attach(|py| {
            // Import sys module to capture stdout/stderr
            let sys = py.import("sys")?;
            let io = py.import("io")?;

            // Create StringIO objects for stdout and stderr
            let stdout_capture = io.call_method0("StringIO")?;
            let stderr_capture = io.call_method0("StringIO")?;

            // Replace sys.stdout and sys.stderr temporarily
            let original_stdout = sys.getattr("stdout")?;
            let original_stderr = sys.getattr("stderr")?;

            sys.setattr("stdout", stdout_capture.clone())?;
            sys.setattr("stderr", stderr_capture.clone())?;

            // Create locals dict to capture return value
            let locals = PyDict::new(py);

            // Execute code
            if let Err(e) = Self::execute_code_internal(py, code, &locals) {
                error!(?e, "Python execution error");
                // Restore stdout/stderr before returning
                let _ = sys.setattr("stdout", original_stdout);
                let _ = sys.setattr("stderr", original_stderr);

                let stderr_str = stderr_capture
                    .call_method0("getvalue")?
                    .extract::<String>()?;
                return Ok(ExecutionResult::failure(format!("{}\n{}", stderr_str, e)));
            }

            // Capture output
            let stdout_str = stdout_capture
                .call_method0("getvalue")?
                .extract::<String>()?;
            let _stderr_str = stderr_capture
                .call_method0("getvalue")?
                .extract::<String>()?;

            // Restore stdout/stderr
            sys.setattr("stdout", original_stdout)?;
            sys.setattr("stderr", original_stderr)?;

            // Get all user-defined variables (exclude internal/magic variables)
            const MAX_VAR_LEN: usize = 128;
            let mut variables = Vec::new();
            for (key, value) in locals.iter() {
                if let Ok(var_name) = key.extract::<String>() {
                    // Skip internal/magic variables (starting with _)
                    if !var_name.starts_with('_')
                        && let Ok(var_repr) = value.repr()
                    {
                        let repr_str = var_repr.to_string();
                        let truncated = if repr_str.len() > MAX_VAR_LEN {
                            // Find valid UTF-8 boundary
                            let boundary = repr_str
                                .char_indices()
                                .take_while(|(idx, _)| *idx < MAX_VAR_LEN)
                                .last()
                                .map(|(idx, c)| idx + c.len_utf8())
                                .unwrap_or(0);
                            format!(
                                "{}... <truncated, {} bytes>",
                                &repr_str[..boundary],
                                repr_str.len()
                            )
                        } else {
                            repr_str
                        };
                        variables.push(format!("{} = {}", var_name, truncated));
                    }
                }
            }

            // Format all variables as result string
            let result_str = if variables.is_empty() {
                None
            } else {
                Some(variables.join("\n"))
            };

            Ok(ExecutionResult::success(stdout_str, result_str, 0))
        })
    }

    /// Internal code execution
    fn execute_code_internal<'py>(
        py: Python<'py>,
        code: &str,
        locals: &Bound<'py, PyDict>,
    ) -> PyResult<()> {
        // Convert &str to CString, then to CStr for py.run()
        let c_code = CString::new(code)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(format!("Invalid code: {}", e)))?;

        // Use py.run() with locals as both globals and locals
        // This ensures imported modules are accessible inside functions
        py.run(c_code.as_c_str(), Some(locals), Some(locals))?;
        Ok(())
    }

    /// Check if a Python module is available
    #[allow(dead_code)]
    pub fn has_module(&self, module_name: &str) -> bool {
        // In sandbox mode, only allow checking safe modules
        if self.config.sandbox && !self.config.safe_modules.contains(&module_name.into()) {
            debug!("Sandbox mode: module '{}' is not allowed", module_name);
            return false;
        }

        Python::attach(|py| match py.import(module_name) {
            Ok(_) => {
                debug!("Module '{}' is available", module_name);
                true
            }
            Err(e) => {
                debug!("Module '{}' is not available: {}", module_name, e);
                false
            }
        })
    }

    /// List available built-in modules
    pub fn list_modules(&self) -> Vec<String> {
        Python::attach(|py| {
            let sys = match py.import("sys") {
                Ok(s) => s,
                Err(e) => {
                    error!("Failed to import sys: {}", e);
                    return vec![];
                }
            };

            let modules_dict: Bound<'_, PyDict> = match sys.getattr("modules") {
                Ok(m) => match m.cast_into() {
                    Ok(d) => d,
                    Err(e) => {
                        error!("Failed to cast modules dict: {}", e);
                        return vec![];
                    }
                },
                Err(e) => {
                    error!("Failed to get modules: {}", e);
                    return vec![];
                }
            };

            debug!("Total modules in sys.modules: {}", modules_dict.len());

            let mut available_modules = Vec::new();
            for (key, value) in modules_dict.iter() {
                if let Ok(name) = key.extract::<String>() {
                    // Filter out internal modules and third-party packages
                    if !name.starts_with("_") && !name.contains('.') {
                        // In sandbox mode, check if module is allowed
                        if !self.config.sandbox {
                            // Non-sandbox: show all non-internal modules
                            available_modules.push(name);
                        } else {
                            // Sandbox mode: show safe modules OR dynamically registered modules
                            if self.config.safe_modules.contains(&name) {
                                debug!("Adding safe module: {}", name);
                                available_modules.push(name);
                            } else if value.is_truthy().unwrap_or(false) {
                                // Module is loaded and accessible, might be dynamically registered
                                debug!("Adding dynamically registered module: {}", name);
                                available_modules.push(name);
                            }
                        }
                    }
                }
            }

            available_modules.sort();
            available_modules
        })
    }

    /// Get current configuration
    #[allow(dead_code)]
    pub fn config(&self) -> &PythonConfig {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use {super::*, tokio::time::sleep};

    #[tokio::test]
    async fn test_simple_execution() -> Result<()> {
        let interpreter = PythonInterpreter::new(Default::default())?;

        // Test simple math (single variable)
        let result = interpreter.execute("x = 2 + 2").await?;
        assert!(result.success);
        assert!(result.vars.unwrap().contains("x = 4"));

        // Test string operations
        let result = interpreter.execute("s = 'hello' + ' world'").await?;
        assert!(result.success);
        assert!(result.vars.unwrap().contains("s = 'hello world'"));

        Ok(())
    }

    #[tokio::test]
    async fn test_stdout_capture() -> Result<()> {
        sleep(Duration::from_secs(1)).await;
        let config = PythonConfig {
            sandbox: false,
            max_execution_time: 30,
            dangerous_modules: vec![],
            safe_modules: vec![],
        };
        let interpreter = PythonInterpreter::new(config)?;

        let result = interpreter
            .execute("import sys; print('test output')")
            .await?;
        assert!(
            result.success,
            "stdout: {}, stderr: {}",
            result.stdout, result.stderr
        );
        assert!(
            result.stdout.contains("test output"),
            "stdout: {}",
            result.stdout
        );

        Ok(())
    }

    #[test]
    fn test_list_modules() {
        // Test without sandbox to see all modules
        let config = PythonConfig {
            sandbox: false,
            max_execution_time: 30,
            dangerous_modules: vec![],
            safe_modules: vec![],
        };
        let interpreter = PythonInterpreter::new(config).unwrap();
        let modules = interpreter.list_modules();

        // Should have some modules
        assert!(!modules.is_empty());
    }

    #[tokio::test]
    async fn test_sandbox_restriction() -> Result<()> {
        let config = PythonConfig {
            sandbox: true,
            max_execution_time: 30,
            dangerous_modules: vec!["os".to_string()],
            safe_modules: vec!["math".to_string()],
        };
        let interpreter = PythonInterpreter::new(config)?;

        // Dangerous module should be blocked
        let result = interpreter.execute("import os").await?;
        assert!(!result.success);
        assert!(result.stderr.contains("not allowed"));

        Ok(())
    }

    #[tokio::test]
    async fn test_sandbox_allows_safe_modules() -> Result<()> {
        let config = PythonConfig {
            sandbox: true,
            max_execution_time: 30,
            dangerous_modules: vec!["os".to_string()],
            safe_modules: vec!["math".to_string()],
        };
        let interpreter = PythonInterpreter::new(config)?;

        // Safe modules should work
        let result = interpreter
            .execute("import math; x = math.sqrt(16)")
            .await?;
        assert!(result.success);
        assert!(result.vars.unwrap().contains("x = 4.0"));

        Ok(())
    }

    #[tokio::test]
    async fn test_timeout() -> Result<()> {
        let config = PythonConfig {
            sandbox: false,
            max_execution_time: 1, // 1 second timeout
            dangerous_modules: vec![],
            safe_modules: vec![],
        };
        let interpreter = PythonInterpreter::new(config)?;

        // Long-running code should time out
        let result = interpreter.execute("import time; time.sleep(5)").await?;
        assert!(!result.success);
        assert!(result.stderr.contains("timed out"));

        Ok(())
    }

    #[tokio::test]
    async fn test_return_value() -> Result<()> {
        let interpreter = PythonInterpreter::new(Default::default())?;

        // Code that sets variables should return all of them
        let result = interpreter.execute("x = 42").await?;
        assert!(result.success);
        assert!(result.vars.unwrap().contains("x = 42"));

        // Code with multiple variables
        let result = interpreter.execute("a = 1\nb = 2").await?;
        assert!(result.success);
        let result_str = result.vars.unwrap();
        assert!(result_str.contains("a = 1"));
        assert!(result_str.contains("b = 2"));

        // Code without any variable definitions should return None
        let result = interpreter.execute("print('hello')").await?;
        assert!(result.success);
        assert_eq!(result.vars, None);

        Ok(())
    }

    #[test]
    fn test_has_module() -> Result<()> {
        let interpreter = PythonInterpreter::new(PythonConfig {
            safe_modules: vec!["json".into()],
            ..Default::default()
        })?;
        assert!(!interpreter.has_module("memory"));
        assert!(interpreter.has_module("json"));

        Ok(())
    }

    #[tokio::test]
    async fn test_module_accessible_in_function() -> Result<()> {
        // Test that imported modules are accessible inside functions
        let interpreter = PythonInterpreter::new(PythonConfig {
            sandbox: false,
            ..Default::default()
        })?;

        let code = r#"
import re

def test_func():
    # This should work - re module should be accessible
    return re.sub(r'\d+', 'X', 'abc123def')

result = test_func()
"#;
        let result = interpreter.execute(code).await?;
        assert!(result.success, "Execution failed: {}", result.stderr);
        assert!(
            result.stdout.contains("abcXdef")
                || result
                    .vars
                    .as_ref()
                    .map_or(false, |v| v.contains("abcXdef")),
            "Expected 'abcXdef' in output, got stdout: {}, vars: {:?}",
            result.stdout,
            result.vars
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_global_keyword() -> Result<()> {
        // Test that global keyword works correctly when globals == locals
        let interpreter = PythonInterpreter::new(PythonConfig::default())?;

        let code = r#"
counter = 0

def increment():
    global counter
    counter += 1

increment()
increment()
"#;
        let result = interpreter.execute(code).await?;
        assert!(result.success, "Execution failed: {}", result.stderr);
        assert!(
            result
                .vars
                .as_ref()
                .map_or(false, |v: &String| v.contains("counter = 2")),
            "Expected counter=2, got: {:?}",
            result.vars
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_nonlocal_keyword() -> Result<()> {
        // Test that nonlocal keyword works correctly (closure behavior)
        let interpreter = PythonInterpreter::new(PythonConfig::default())?;

        let code = r#"
def outer():
    count = 0
    def inner():
        nonlocal count
        count += 1
        return count
    return inner()

result = outer()
"#;
        let result = interpreter.execute(code).await?;
        assert!(result.success, "Execution failed: {}", result.stderr);
        assert!(
            result
                .vars
                .as_ref()
                .map_or(false, |v: &String| v.contains("result = 1")),
            "Expected result=1, got: {:?}",
            result.vars
        );

        Ok(())
    }
}
