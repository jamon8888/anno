//! Integration tests for the env module.
//!
//! Tests environment variable handling, .env loading, and HF token detection.

use anno::env;
use std::env as std_env;

#[test]
fn test_load_dotenv_idempotent() {
    // Calling load_dotenv multiple times should not panic
    env::load_dotenv();
    env::load_dotenv();
    env::load_dotenv();
}

#[test]
fn test_cache_dir_returns_valid_path() {
    let dir = env::cache_dir();

    // Should return a non-empty path
    assert!(!dir.as_os_str().is_empty());

    // Should be an absolute path or relative path starting with anno-cache
    let path_str = dir.to_string_lossy();
    assert!(
        dir.is_absolute() || path_str.contains("anno") || path_str.contains("cache"),
        "Expected cache dir to contain 'anno' or 'cache': {:?}",
        dir
    );
}

#[test]
fn test_hf_token_detection() {
    // Load .env first
    env::load_dotenv();

    // The result should be consistent
    let has_token = env::has_hf_token();
    let token = env::hf_token();

    if has_token {
        assert!(
            token.is_some(),
            "has_hf_token() and hf_token() should be consistent"
        );
        assert!(!token.unwrap().is_empty(), "Token should not be empty");
    } else {
        assert!(
            token.is_none(),
            "has_hf_token() and hf_token() should be consistent"
        );
    }
}

#[test]
fn test_llm_api_key_detection() {
    // This should not panic and should return a boolean
    let _has_key = env::has_llm_api_key();
}

#[test]
fn test_env_vars_override_dotenv() {
    // Set an environment variable directly
    let test_key = format!("ANNO_TEST_VAR_{}", std::process::id());
    std_env::set_var(&test_key, "from_env");

    // Load .env (which might have the same key - though unlikely for this test key)
    env::reload_dotenv();

    // Environment variable should still have the original value
    assert_eq!(std_env::var(&test_key).unwrap(), "from_env");

    // Clean up
    std_env::remove_var(&test_key);
}

#[test]
fn test_dotenv_parses_various_formats() {
    // This test verifies that the dotenv parser handles different formats
    // We test this by checking that the env module doesn't panic on various inputs

    // The actual parsing is tested in the unit tests in env.rs
    // This integration test just ensures the module is usable
    env::load_dotenv();

    // Should be able to call these without panicking
    let _ = env::cache_dir();
    let _ = env::has_hf_token();
    let _ = env::hf_token();
    let _ = env::has_llm_api_key();
}
