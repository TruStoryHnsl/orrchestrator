use regex::Regex;
use sha2::{Digest, Sha256};
use std::sync::LazyLock;

/// Patterns to strip variable parts when fingerprinting.
static STRIP_PATTERNS: LazyLock<Vec<(Regex, &'static str)>> = LazyLock::new(|| {
    vec![
        (Regex::new(r", line \d+").unwrap(), ", line <N>"),
        (
            Regex::new(r#"File "/.+?/([^/]+\.py)""#).unwrap(),
            r#"File "<...>/$1""#,
        ),
        (Regex::new(r"0x[0-9a-fA-F]+").unwrap(), "0x<ADDR>"),
        (
            Regex::new(r"\d{4}-\d{2}-\d{2}[T ]\d{2}:\d{2}:\d{2}").unwrap(),
            "<TIMESTAMP>",
        ),
        (
            Regex::new(r"[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}").unwrap(),
            "<UUID>",
        ),
        (Regex::new(r":\d{4,5}([\s/])").unwrap(), ":<PORT>$1"),
        (Regex::new(r"(\s)\d{3,}(\s)").unwrap(), "$1<NUM>$2"),
        (Regex::new(r"'[^']{20,}'").unwrap(), "'<...>'"),
    ]
});

/// Create a stable fingerprint for an error by normalizing variable parts.
///
/// Returns a 16-character hex digest that groups "same class" errors together.
pub fn fingerprint(error_text: &str) -> String {
    let mut normalized = error_text.to_string();

    for (pattern, replacement) in STRIP_PATTERNS.iter() {
        normalized = pattern.replace_all(&normalized, *replacement).to_string();
    }

    // Collapse whitespace
    let collapsed = Regex::new(r"\s+")
        .unwrap()
        .replace_all(&normalized, " ")
        .trim()
        .to_string();

    let mut hasher = Sha256::new();
    hasher.update(collapsed.as_bytes());
    let result = hasher.finalize();
    hex::encode(&result[..8])
}

// We need hex encoding — add a minimal implementation to avoid a dep
mod hex {
    pub fn encode(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fingerprint_stability() {
        let text = "KeyError: 'missing_key'";
        let fp1 = fingerprint(text);
        let fp2 = fingerprint(text);
        assert_eq!(fp1, fp2);
        assert_eq!(fp1.len(), 16);
    }

    #[test]
    fn test_fingerprint_normalizes_line_numbers() {
        let a = "File \"app.py\", line 42, in foo\nKeyError: 'x'";
        let b = "File \"app.py\", line 99, in foo\nKeyError: 'x'";
        assert_eq!(fingerprint(a), fingerprint(b));
    }

    #[test]
    fn test_fingerprint_normalizes_timestamps() {
        let a = "Error at 2026-03-26T10:30:00: connection failed";
        let b = "Error at 2026-01-01T00:00:00: connection failed";
        assert_eq!(fingerprint(a), fingerprint(b));
    }
}
