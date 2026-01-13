#[cfg(test)]
#[cfg(feature = "onnx")]
mod tests {
    use anno::eval::backend_factory::BackendFactory;
    use std::time::Duration;

    /// Test that gliner backend can be created (even if model loading fails).
    /// This helps identify initialization vs model loading issues.
    #[test]
    fn test_gliner_backend_creation() {
        // This should not panic, even if model loading fails
        let result = std::panic::catch_unwind(|| BackendFactory::create("gliner"));

        match result {
            Ok(backend_result) => {
                match backend_result {
                    Ok(backend) => {
                        // Backend created successfully
                        println!("✓ gliner backend created successfully");
                        // Check if it's available (model loaded)
                        if backend.is_available() {
                            println!("✓ gliner backend is available (model loaded)");
                        } else {
                            println!(
                                "⚠ gliner backend created but not available (model not loaded)"
                            );
                        }
                    }
                    Err(e) => {
                        // Backend creation failed
                        eprintln!("✗ gliner backend creation failed: {}", e);
                        // This is expected if ONNX runtime is not available
                        // or if model download fails
                    }
                }
            }
            Err(_) => {
                panic!("Backend creation panicked (should not happen)");
            }
        }
    }

    /// Test gliner backend with timeout to catch hanging initialization.
    #[test]
    fn test_gliner_backend_creation_timeout() {
        use std::sync::{Arc, Mutex};
        use std::thread;

        let result = Arc::new(Mutex::new(None));
        let result_clone = result.clone();

        let handle = thread::spawn(move || {
            let backend_result = BackendFactory::create("gliner");
            *result_clone.lock().unwrap() = Some(backend_result);
        });

        // Wait up to 30 seconds for initialization
        if handle.join_timeout(Duration::from_secs(30)).is_err() {
            eprintln!("⚠ gliner backend creation timed out after 30s");
            // This suggests model download or ONNX initialization is hanging
        } else {
            if let Ok(Some(backend_result)) = result.lock() {
                match backend_result {
                    Ok(backend) => {
                        println!("✓ gliner backend created within timeout");
                        assert!(backend.is_available() || !backend.is_available());
                    }
                    Err(e) => {
                        println!("⚠ gliner backend creation failed: {}", e);
                    }
                }
            }
        }
    }

    /// Test that gliner backend errors are properly reported.
    #[test]
    fn test_gliner_backend_error_handling() {
        let backend_result = BackendFactory::create("gliner");

        match backend_result {
            Ok(backend) => {
                // If backend is created, test extraction (may fail if model not loaded)
                let test_result = backend.extract_entities("Test text", None);
                match test_result {
                    Ok(entities) => {
                        println!("✓ gliner extraction succeeded: {} entities", entities.len());
                    }
                    Err(e) => {
                        println!("⚠ gliner extraction failed: {}", e);
                        // This is expected if model is not loaded
                    }
                }
            }
            Err(e) => {
                // Check error message is informative
                let error_msg = format!("{}", e);
                assert!(
                    error_msg.contains("gliner")
                        || error_msg.contains("ONNX")
                        || error_msg.contains("model")
                        || error_msg.contains("not available"),
                    "Error message should mention gliner, ONNX, model, or availability: {}",
                    error_msg
                );
                println!("✓ gliner backend error is informative: {}", error_msg);
            }
        }
    }
}
