use regex::Regex;
use std::sync::LazyLock;

/// Broad error category for classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ErrorCategory {
    Import,
    ApiDrift,
    Lookup,
    Type,
    Syntax,
    Value,
    MissingFile,
    Permission,
    MissingCommand,
    Network,
    TestFailure,
    Runtime,
    Unknown,
}

impl ErrorCategory {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Import => "import",
            Self::ApiDrift => "api-drift",
            Self::Lookup => "lookup",
            Self::Type => "type",
            Self::Syntax => "syntax",
            Self::Value => "value",
            Self::MissingFile => "missing-file",
            Self::Permission => "permission",
            Self::MissingCommand => "missing-command",
            Self::Network => "network",
            Self::TestFailure => "test-failure",
            Self::Runtime => "runtime",
            Self::Unknown => "unknown",
        }
    }
}

/// Compiled error detection patterns.
static ERROR_PATTERNS: LazyLock<Vec<Regex>> = LazyLock::new(|| {
    vec![
        // Python tracebacks
        Regex::new(r"(?m)^Traceback \(most recent call last\):").unwrap(),
        // Generic error/exception lines
        Regex::new(r"(?m)^(\w+Error|\w+Exception):\s+.+").unwrap(),
        // Node.js errors
        Regex::new(r"(?m)^(TypeError|ReferenceError|SyntaxError|RangeError):\s+.+").unwrap(),
        // Claude Code tool errors
        Regex::new(r"(?m)^Error:?\s+.+").unwrap(),
        // Test failures
        Regex::new(r"(?m)^(FAILED|FAIL|ERROR)\s+.+").unwrap(),
        // Bash errors
        Regex::new(r"(?m)^.+: command not found$").unwrap(),
        Regex::new(r"(?m)^.+: No such file or directory$").unwrap(),
        Regex::new(r"(?m)^.+: Permission denied$").unwrap(),
        // Docker errors
        Regex::new(r"(?m)^ERROR \[.+\]").unwrap(),
        // pip/package errors
        Regex::new(r"(?m)^(?:ERROR|error):\s+(?:Could not|Failed to|No matching).+").unwrap(),
        // Import errors
        Regex::new(r"(?m)^(?:ImportError|ModuleNotFoundError):\s+.+").unwrap(),
        // Attribute errors
        Regex::new(r"(?m)^AttributeError:\s+.+").unwrap(),
    ]
});

/// Extract error blocks from session output text.
///
/// Returns context strings — each is the error line plus surrounding lines.
pub fn extract_errors(text: &str) -> Vec<String> {
    let lines: Vec<&str> = text.lines().collect();
    let mut errors: Vec<String> = Vec::new();

    for pattern in ERROR_PATTERNS.iter() {
        for mat in pattern.find_iter(text) {
            let match_start = mat.start();
            let line_num = text[..match_start].matches('\n').count();

            // Context: up to 15 lines before, 3 after
            let start = line_num.saturating_sub(15);
            let end = (line_num + 4).min(lines.len());
            let context: String = lines[start..end].join("\n");

            // Dedup: if new context overlaps existing, keep longer
            let mut replaced = false;
            for existing in errors.iter_mut() {
                if existing.contains(&context) {
                    // Existing is a superset — skip
                    replaced = true;
                    break;
                }
                if context.contains(existing.as_str()) {
                    // New is a superset — replace
                    *existing = context.clone();
                    replaced = true;
                    break;
                }
            }
            if !replaced {
                errors.push(context);
            }
        }
    }

    errors
}

/// Classify an error into a broad category.
///
/// Checks specific exception types first, then falls back to generic patterns.
pub fn classify_error(error_text: &str) -> ErrorCategory {
    let lower = error_text.to_lowercase();

    // Specific exception types first
    if lower.contains("importerror") || lower.contains("modulenotfounderror") {
        return ErrorCategory::Import;
    }
    if lower.contains("attributeerror") {
        return ErrorCategory::ApiDrift;
    }
    if lower.contains("keyerror") || lower.contains("indexerror") {
        return ErrorCategory::Lookup;
    }
    if lower.contains("typeerror") {
        return ErrorCategory::Type;
    }
    if lower.contains("syntaxerror") {
        return ErrorCategory::Syntax;
    }
    if lower.contains("valueerror") {
        return ErrorCategory::Value;
    }
    if lower.contains("filenotfounderror") || lower.contains("no such file") {
        return ErrorCategory::MissingFile;
    }
    if lower.contains("permissionerror") || lower.contains("permission denied") {
        return ErrorCategory::Permission;
    }
    if lower.contains("command not found") {
        return ErrorCategory::MissingCommand;
    }
    if lower.contains("connectionerror") || lower.contains("timeout") {
        return ErrorCategory::Network;
    }
    // Generic patterns last
    if lower.contains("fail") || error_text.contains("FAILED") {
        return ErrorCategory::TestFailure;
    }
    if lower.contains("traceback") || lower.contains("error") {
        return ErrorCategory::Runtime;
    }
    ErrorCategory::Unknown
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_python_traceback() {
        let text = r#"
Traceback (most recent call last):
  File "app.py", line 42, in index
    return data['missing']
KeyError: 'missing'
"#;
        let errors = extract_errors(text);
        assert!(!errors.is_empty());
        assert!(errors[0].contains("KeyError"));
    }

    #[test]
    fn test_classify_keyerror() {
        let text = "Traceback...\nKeyError: 'missing'";
        assert_eq!(classify_error(text), ErrorCategory::Lookup);
    }

    #[test]
    fn test_classify_attributeerror() {
        let text = "AttributeError: type object 'Label' has no attribute 'Click'";
        assert_eq!(classify_error(text), ErrorCategory::ApiDrift);
    }

    #[test]
    fn test_classify_import() {
        let text = "ImportError: cannot import name 'Foo' from 'bar'";
        assert_eq!(classify_error(text), ErrorCategory::Import);
    }

    #[test]
    fn test_classify_command_not_found() {
        let text = "docker: command not found";
        assert_eq!(classify_error(text), ErrorCategory::MissingCommand);
    }

    #[test]
    fn test_extract_node_error() {
        let text = "TypeError: Cannot read properties of undefined (reading 'map')";
        let errors = extract_errors(text);
        assert!(!errors.is_empty());
        assert_eq!(classify_error(&errors[0]), ErrorCategory::Type);
    }

    #[test]
    fn test_dedup_overlapping() {
        let text = r#"
Traceback (most recent call last):
  File "app.py", line 10, in foo
    x = bar()
  File "app.py", line 20, in bar
    return baz['key']
KeyError: 'key'
"#;
        let errors = extract_errors(text);
        // Should produce exactly 1 error (the longer context), not 2
        assert_eq!(errors.len(), 1);
        assert!(errors[0].contains("KeyError"));
        assert!(errors[0].contains("Traceback"));
    }
}
