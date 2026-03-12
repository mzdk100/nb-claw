//! File path extraction from code and commands
//!
//! Uses a broad approach:
//! 1. Extract all quoted strings that look like paths
//! 2. Handle path concatenation (os.path.join, +, / operators)
//! 3. Track variable assignments for path resolution

use {regex::Regex, std::collections::HashMap};

/// Check if a string looks like a file path
fn looks_like_path(s: &str) -> bool {
    if s.is_empty() || s.len() > 500 {
        return false;
    }

    // Skip URLs
    if s.starts_with("http://") || s.starts_with("https://") || s.starts_with("ftp://") {
        return false;
    }

    // Windows absolute path: C:\, D:\, etc.
    if Regex::new(r"^[A-Za-z]:[/\\]").unwrap().is_match(s) {
        return true;
    }

    // Unix absolute path
    if s.starts_with('/') && s.len() > 1 {
        return true;
    }

    // Relative paths with separators
    if s.contains('/') || s.contains('\\') {
        // Skip if it looks like a URL without protocol
        if s.contains("://") || s.starts_with("www.") {
            return false;
        }
        return true;
    }

    // Files with common extensions (no path separators)
    const COMMON_EXTENSIONS: [&str; 52] = [
        ".txt", ".md", ".json", ".yaml", ".yml", ".xml", ".html", ".css", ".js", ".ts", ".py",
        ".rs", ".go", ".java", ".c", ".cpp", ".h", ".hpp", ".sh", ".bat", ".ps1", ".toml", ".ini",
        ".cfg", ".conf", ".log", ".csv", ".tsv", ".pdf", ".doc", ".docx", ".xls", ".xlsx", ".png",
        ".jpg", ".jpeg", ".gif", ".svg", ".zip", ".tar", ".gz", ".rar", ".7z", ".exe", ".dll",
        ".so", ".dylib", ".bin", ".dat", ".db", ".sqlite", ".sql",
    ];
    COMMON_EXTENSIONS
        .iter()
        .any(|ext| s.to_lowercase().ends_with(ext))
}

/// Extract variable assignments from Python code
/// Store all string assignments (not just path-like) for potential path building
fn extract_python_variables(code: &str) -> HashMap<String, String> {
    let mut vars = HashMap::new();

    // Pattern: var = "value" or var = r"value" or var = 'value'
    let pattern = Regex::new(r#"([a-zA-Z_][a-zA-Z0-9_]*)\s*=\s*r?["']([^"']+)["']"#).unwrap();

    for cap in pattern.captures_iter(code) {
        let var_name = cap[1].to_string();
        let value = cap[2].to_string();
        // Store all string values (they might be used in path construction)
        vars.insert(var_name, value);
    }

    vars
}

/// Extract variable assignments from shell commands
fn extract_shell_variables(command: &str) -> HashMap<String, String> {
    let mut vars = HashMap::new();

    // Pattern: var=value or var="value" (shell style)
    let pattern = Regex::new(r#"([a-zA-Z_][a-zA-Z0-9_]*)=(?:"([^"]*)"|'([^']*)'|(\S+))"#).unwrap();

    for cap in pattern.captures_iter(command) {
        let var_name = cap[1].to_string();
        let value = cap
            .get(2)
            .or_else(|| cap.get(3))
            .or_else(|| cap.get(4))
            .map(|m| m.as_str().to_string());

        if let Some(v) = value {
            if looks_like_path(&v) {
                vars.insert(var_name, v);
            }
        }
    }

    vars
}

/// Extract all quoted strings that look like paths from Python code
fn extract_python_path_strings(code: &str) -> Vec<String> {
    let mut paths = Vec::new();

    // Pattern for all quoted strings: "..." or '...' or r"..." or r'...'
    let string_pattern = Regex::new(r#"r?["']([^"']+)["']"#).unwrap();

    for cap in string_pattern.captures_iter(code) {
        let s = cap[1].to_string();
        if looks_like_path(&s) {
            paths.push(s);
        }
    }

    paths
}

/// Extract all quoted strings that look like paths from shell commands
fn extract_shell_path_strings(command: &str) -> Vec<String> {
    let mut paths = Vec::new();

    // Pattern for quoted strings
    let quoted_pattern = Regex::new(r#"["']([^"']+)["']"#).unwrap();
    for cap in quoted_pattern.captures_iter(command) {
        let s = cap[1].to_string();
        if looks_like_path(&s) {
            paths.push(s);
        }
    }

    // Pattern for unquoted Windows absolute paths
    let win_path_pattern = Regex::new(r#"[A-Za-z]:[/\\][^\s&|<>"']*"#).unwrap();
    for cap in win_path_pattern.captures_iter(command) {
        paths.push(cap[0].to_string());
    }

    // Pattern for unquoted Unix paths (starting with /)
    let unix_path_pattern = Regex::new(r#"/[^\s&|<>"']*[^\s&|<>"'.]"#).unwrap();
    for cap in unix_path_pattern.captures_iter(command) {
        let s = cap[0].to_string();
        // Skip common non-path patterns
        if !s.starts_with("/dev/")
            && !s.starts_with("/proc/")
            && !s.starts_with("/sys/")
            && s.len() > 2
        {
            paths.push(s);
        }
    }

    paths
}

/// Handle os.path.join() calls - try to reconstruct the path
fn handle_os_path_join(code: &str, vars: &HashMap<String, String>) -> Vec<String> {
    let mut paths = Vec::new();

    // Pattern for os.path.join(...)
    let join_pattern = Regex::new(r#"os\.path\.join\s*\(([^)]+)\)"#).unwrap();

    for cap in join_pattern.captures_iter(code) {
        let args = &cap[1];
        let parts: Vec<String> = extract_join_args(args, vars);

        if !parts.is_empty() {
            // Try to join the parts
            let joined = join_path_parts(&parts);
            if looks_like_path(&joined) {
                paths.push(joined);
            }
            // Also add individual path-like parts
            for part in &parts {
                if looks_like_path(part) && !paths.contains(part) {
                    paths.push(part.clone());
                }
            }
        }
    }

    paths
}

/// Extract arguments from os.path.join or similar functions (preserving order)
fn extract_join_args(args_str: &str, vars: &HashMap<String, String>) -> Vec<String> {
    let mut parts = Vec::new();

    // Split by commas, but be careful about nested parentheses
    let args = split_args(args_str);

    for arg in args {
        let arg = arg.trim();
        if arg.is_empty() {
            continue;
        }

        // Try to match quoted string: "..." or '...'
        let quoted_pattern = Regex::new(r#"^["']([^"']+)["']$"#).unwrap();
        if let Some(cap) = quoted_pattern.captures(arg) {
            parts.push(cap[1].to_string());
            continue;
        }

        // Try to match variable name (no capture group, use get(0))
        let ident_pattern = Regex::new(r"^[a-zA-Z_][a-zA-Z0-9_]*$").unwrap();
        if ident_pattern.is_match(arg) {
            if let Some(value) = vars.get(arg) {
                parts.push(value.clone());
            }
            continue;
        }

        // Try to extract quoted string from within the argument
        let inner_quoted = Regex::new(r#"["']([^"']+)["']"#).unwrap();
        if let Some(cap) = inner_quoted.captures(arg) {
            parts.push(cap[1].to_string());
        }
    }

    parts
}

/// Split argument string by commas, respecting nested parentheses
fn split_args(args_str: &str) -> Vec<&str> {
    let mut args = Vec::new();
    let mut start = 0;
    let mut depth = 0;

    for (i, c) in args_str.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            ',' if depth == 0 => {
                args.push(&args_str[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }

    // Add the last argument
    if start < args_str.len() {
        args.push(&args_str[start..]);
    }

    args
}

/// Join path parts with appropriate separator
fn join_path_parts(parts: &[String]) -> String {
    if parts.is_empty() {
        return String::new();
    }

    // Detect if Windows-style path
    let is_windows = parts.iter().any(|p| {
        p.contains('\\')
            || p.starts_with(|c: char| {
                c.is_ascii_uppercase()
                    && p.starts_with(&format!("{}:", c.to_lowercase().next().unwrap()))
            })
    });

    let separator = if is_windows { "\\" } else { "/" };

    let mut result = String::new();
    for (i, part) in parts.iter().enumerate() {
        if i > 0
            && !result.ends_with('/')
            && !result.ends_with('\\')
            && !part.starts_with('/')
            && !part.starts_with('\\')
        {
            result.push_str(separator);
        }
        result.push_str(part);
    }

    result
}

/// Handle pathlib division operator: Path("dir") / "file"
fn handle_pathlib_division(code: &str) -> Vec<String> {
    let mut paths = Vec::new();

    // Pattern for Path("...") / "..."
    let pattern =
        Regex::new(r#"Path\s*\(\s*["']([^"']+)["']\s*\)\s*/\s*["']([^"']+)["']"#).unwrap();

    for cap in pattern.captures_iter(code) {
        let dir = &cap[1];
        let file = &cap[2];
        let joined = format!("{}/{}", dir, file);
        paths.push(joined);

        // Also add individual parts if they look like paths
        if looks_like_path(dir) && !paths.contains(&dir.to_string()) {
            paths.push(dir.to_string());
        }
        if looks_like_path(file) && !paths.contains(&file.to_string()) {
            paths.push(file.to_string());
        }
    }

    paths
}

/// Handle string concatenation: "dir" + "/" + "file" or "dir" + os.sep + "file"
fn handle_string_concat(code: &str) -> Vec<String> {
    let mut paths = Vec::new();

    // Pattern for "..." + "..." sequences
    let concat_pattern = Regex::new(r#"(["'][^"']*["']\s*\+\s*)+["'][^"']*["']"#).unwrap();

    for cap in concat_pattern.captures_iter(code) {
        let expr = &cap[0];
        // Extract all string parts
        let string_parts: Vec<&str> = Regex::new(r#"["']([^"']*)["']"#)
            .unwrap()
            .captures_iter(expr)
            .filter_map(|c| c.get(1).map(|m| m.as_str()))
            .collect();

        if !string_parts.is_empty() {
            let joined = string_parts.join("");
            if looks_like_path(&joined) {
                paths.push(joined);
            }
        }
    }

    paths
}

/// Extract file paths from Python code
pub fn extract_paths_from_python(code: &str) -> Vec<String> {
    let mut paths = Vec::new();

    // Extract variables for resolution
    let vars = extract_python_variables(code);

    // 1. Extract all quoted strings that look like paths
    paths.extend(extract_python_path_strings(code));

    // 2. Handle os.path.join() calls
    paths.extend(handle_os_path_join(code, &vars));

    // 3. Handle pathlib division: Path("dir") / "file"
    paths.extend(handle_pathlib_division(code));

    // 4. Handle string concatenation
    paths.extend(handle_string_concat(code));

    paths
}

/// Extract file paths from CMD/Shell commands
pub fn extract_paths_from_cmd(command: &str) -> Vec<String> {
    let mut paths = Vec::new();

    // Extract variables for resolution
    let _vars = extract_shell_variables(command);

    // 1. Extract all quoted strings and paths that look like paths
    paths.extend(extract_shell_path_strings(command));

    // 2. Handle string concatenation (less common in shell, but possible)
    paths.extend(handle_string_concat(command));

    paths
}

/// Extract all file paths from code (Python) or command
pub fn extract_paths(content: &str, is_python: bool) -> Vec<String> {
    let mut paths = if is_python {
        extract_paths_from_python(content)
    } else {
        extract_paths_from_cmd(content)
    };

    // Deduplicate
    paths.sort();
    paths.dedup();

    paths
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_looks_like_path() {
        assert!(looks_like_path("C:\\Users\\test\\file.txt"));
        assert!(looks_like_path("D:/data/config.json"));
        assert!(looks_like_path("/home/user/file.txt"));
        assert!(looks_like_path("data/output.json"));
        assert!(looks_like_path("config.txt"));
        assert!(looks_like_path("./relative/path.md"));

        assert!(!looks_like_path("http://example.com"));
        assert!(!looks_like_path("hello world"));
        assert!(!looks_like_path(""));
    }

    #[test]
    fn test_variable_assignment() {
        let code = r#"file_path = r'D:\baidu.md'
os.remove(file_path)"#;
        let vars = extract_python_variables(code);
        assert_eq!(vars.get("file_path"), Some(&"D:\\baidu.md".to_string()));
    }

    #[test]
    fn test_variable_path_extraction() {
        let code = r#"file_path = r'D:\baidu.md'
os.remove(file_path)"#;
        let paths = extract_paths_from_python(code);
        assert!(paths.contains(&"D:\\baidu.md".to_string()));
    }

    #[test]
    fn test_os_path_join() {
        let code = r#"path = os.path.join("data", "output.json")"#;
        let paths = extract_paths_from_python(code);
        assert!(paths.iter().any(|p| p.contains("output.json")));
    }

    #[test]
    fn test_pathlib_division() {
        let code = r#"p = Path("data") / "output.json""#;
        let paths = extract_paths_from_python(code);
        assert!(
            paths
                .iter()
                .any(|p| p.contains("data") && p.contains("output.json"))
        );
    }

    #[test]
    fn test_string_concat() {
        let code = r#"path = "data" + "/" + "output.json""#;
        let paths = extract_paths_from_python(code);
        assert!(paths.contains(&"data/output.json".to_string()));
    }

    #[test]
    fn test_cmd_windows_path() {
        let cmd = r#"del "D:\baidu.md""#;
        let paths = extract_paths_from_cmd(cmd);
        assert!(paths.contains(&"D:\\baidu.md".to_string()));
    }

    #[test]
    fn test_cmd_unquoted_path() {
        let cmd = r#"type D:\test.txt"#;
        let paths = extract_paths_from_cmd(cmd);
        assert!(paths.contains(&"D:\\test.txt".to_string()));
    }

    #[test]
    fn test_combined_extraction() {
        let code = r#"file1 = "data/input.txt"
file2 = r"D:\output.json"
path = os.path.join("config", "settings.yaml")
with open("log.txt", "w") as f:
    f.write("done")"#;
        let paths = extract_paths_from_python(code);
        assert!(paths.contains(&"data/input.txt".to_string()));
        assert!(paths.contains(&"D:\\output.json".to_string()));
        assert!(paths.contains(&"log.txt".to_string()));
    }

    #[test]
    fn test_os_path_join_with_vars_order() {
        // Test that order is preserved: path1, path2, 'abc'
        let code = r#"base = "data"
subdir = "output"
path = os.path.join(base, subdir, "file.txt")"#;
        let vars = extract_python_variables(code);
        let args_str = r#"base, subdir, "file.txt""#;
        let parts = extract_join_args(args_str, &vars);

        // Should be in correct order: ["data", "output", "file.txt"]
        assert_eq!(parts, vec!["data", "output", "file.txt"]);

        // Verify the full path is correct
        let paths = extract_paths_from_python(code);
        assert!(paths.contains(&"data/output/file.txt".to_string()));
    }

    #[test]
    fn test_split_args() {
        let args = split_args(r#""a", "b", "c""#);
        // split_args preserves spaces after commas
        assert_eq!(args, vec![r#""a""#, r#" "b""#, r#" "c""#]);

        let args = split_args("base, subdir, file");
        assert_eq!(args, vec!["base", " subdir", " file"]);
    }
}
