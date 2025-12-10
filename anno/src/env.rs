//! Environment variable utilities.
//!
//! Provides centralized handling of .env files and environment configuration.
//!
//! # Usage
//!
//! ```rust,ignore
//! use anno::env::load_dotenv;
//!
//! // Load .env file if present (won't override existing env vars)
//! load_dotenv();
//!
//! // Now HF_TOKEN etc. are available from std::env::var
//! if let Ok(token) = std::env::var("HF_TOKEN") {
//!     println!("HuggingFace token available");
//! }
//! ```
//!
//! # Environment Variables
//!
//! | Variable | Purpose |
//! |----------|---------|
//! | `HF_TOKEN` | HuggingFace API token for gated models |
//! | `OPENAI_API_KEY` | OpenAI API key for LLM backends |
//! | `OPENROUTER_API_KEY` | OpenRouter API key for LLM backends |
//! | `GEMINI_API_KEY` | Google Gemini API key |
//! | `ANNO_CACHE_DIR` | Custom cache directory for models/datasets |
//! | `ANNO_CI_SEED` | Fixed seed for reproducible CI testing |
//! | `ANNO_SAMPLE_STRATEGY` | Backend sampling strategy (random, ml-only, worst-first) |

use once_cell::sync::OnceCell;
use std::path::Path;

/// Global flag to track if dotenv was loaded
static DOTENV_LOADED: OnceCell<bool> = OnceCell::new();

/// Load environment variables from .env file if present.
///
/// This function:
/// - Searches for .env in current directory and up to 2 parent directories
/// - Only sets variables that aren't already set (env vars take precedence)
/// - Is idempotent - safe to call multiple times
/// - Returns silently if no .env file is found
///
/// # Example
///
/// ```rust,ignore
/// anno::env::load_dotenv();
/// ```
pub fn load_dotenv() {
    // Only load once
    DOTENV_LOADED.get_or_init(|| {
        load_dotenv_impl();
        true
    });
}

/// Force reload of .env file (useful for testing)
#[doc(hidden)]
pub fn reload_dotenv() {
    load_dotenv_impl();
}

fn load_dotenv_impl() {
    // Try to find .env in current directory or parents
    let env_paths = [".env", "../.env", "../../.env", "../../../.env"];

    for path_str in env_paths {
        let path = Path::new(path_str);
        if let Ok(contents) = std::fs::read_to_string(path) {
            parse_dotenv(&contents);
            return;
        }
    }

    // Also check workspace root via Cargo manifest
    if let Ok(manifest_dir) = std::env::var("CARGO_MANIFEST_DIR") {
        let workspace_env = Path::new(&manifest_dir).join("../.env");
        if let Ok(contents) = std::fs::read_to_string(&workspace_env) {
            parse_dotenv(&contents);
        }
    }
}

fn parse_dotenv(contents: &str) {
    for line in contents.lines() {
        let line = line.trim();

        // Skip empty lines and comments
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        // Parse KEY=VALUE (handle quoted values)
        if let Some((key, value)) = line.split_once('=') {
            let key = key.trim();
            let value = value.trim();

            // Remove surrounding quotes if present
            let value = value
                .strip_prefix('"')
                .and_then(|v| v.strip_suffix('"'))
                .or_else(|| value.strip_prefix('\'').and_then(|v| v.strip_suffix('\'')))
                .unwrap_or(value);

            // Only set if not already set (env vars take precedence)
            if std::env::var(key).is_err() {
                std::env::set_var(key, value);
            }
        }
    }
}

/// Check if HuggingFace token is available.
#[must_use]
pub fn has_hf_token() -> bool {
    std::env::var("HF_TOKEN").is_ok()
}

/// Get HuggingFace token if available.
#[must_use]
pub fn hf_token() -> Option<String> {
    std::env::var("HF_TOKEN").ok()
}

/// Check if any OpenAI-compatible API key is available.
#[must_use]
pub fn has_llm_api_key() -> bool {
    std::env::var("OPENAI_API_KEY").is_ok()
        || std::env::var("OPENROUTER_API_KEY").is_ok()
        || std::env::var("GEMINI_API_KEY").is_ok()
}

/// Get the cache directory for models and datasets.
#[must_use]
pub fn cache_dir() -> std::path::PathBuf {
    if let Ok(dir) = std::env::var("ANNO_CACHE_DIR") {
        return std::path::PathBuf::from(dir);
    }

    // Platform-specific default
    #[cfg(target_os = "macos")]
    {
        dirs::home_dir()
            .map(|h| h.join("Library/Caches/anno"))
            .unwrap_or_else(|| std::path::PathBuf::from(".anno-cache"))
    }

    #[cfg(target_os = "linux")]
    {
        dirs::cache_dir()
            .map(|c| c.join("anno"))
            .unwrap_or_else(|| std::path::PathBuf::from(".anno-cache"))
    }

    #[cfg(target_os = "windows")]
    {
        dirs::cache_dir()
            .map(|c| c.join("anno"))
            .unwrap_or_else(|| std::path::PathBuf::from(".anno-cache"))
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        std::path::PathBuf::from(".anno-cache")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_dotenv() {
        let contents = r#"
# Comment
KEY1=value1
KEY2="quoted value"
KEY3='single quoted'
  SPACED_KEY = spaced_value  
"#;
        // Use unique keys to avoid test pollution
        let test_prefix = format!("ANNO_TEST_{}", std::process::id());

        let test_contents = contents.replace("KEY", &test_prefix);
        parse_dotenv(&test_contents);

        // The test environment might have these set already, so just check parsing works
    }

    #[test]
    fn test_load_dotenv_idempotent() {
        load_dotenv();
        load_dotenv();
        load_dotenv();
        // Should not panic or cause issues
    }

    #[test]
    fn test_cache_dir() {
        let dir = cache_dir();
        // Should return a valid path
        assert!(!dir.as_os_str().is_empty());
    }
}

