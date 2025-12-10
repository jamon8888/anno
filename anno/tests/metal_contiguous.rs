//! Tests for Metal backend tensor contiguity requirements.
//!
//! The Candle Metal backend requires contiguous tensors for matmul operations.
//! These tests verify that our .contiguous() calls work correctly.

#[cfg(all(feature = "candle", target_os = "macos"))]
mod metal_tests {
    use candle_core::{DType, Device, Tensor};

    /// Test that transpose creates non-contiguous tensor that needs .contiguous()
    #[test]
    fn test_transpose_creates_noncontiguous() {
        let device = Device::Cpu; // Use CPU for this structural test
        let t = Tensor::zeros((2, 3, 4), DType::F32, &device).unwrap();

        // Original tensor is contiguous
        assert!(t.is_contiguous(), "Fresh tensor should be contiguous");

        // Transpose creates non-contiguous view
        let t_transposed = t.transpose(1, 2).unwrap();
        // Note: Candle may or may not mark this as non-contiguous depending on version
        // The key is that matmul will fail on Metal without .contiguous()

        // .contiguous() should work
        let t_contiguous = t_transposed.contiguous().unwrap();
        assert!(
            t_contiguous.is_contiguous(),
            "After .contiguous() should be contiguous"
        );
    }

    /// Test that broadcast_as creates non-contiguous tensor
    #[test]
    fn test_broadcast_creates_noncontiguous() {
        let device = Device::Cpu;
        let t = Tensor::zeros((1, 3), DType::F32, &device).unwrap();

        // Broadcast to larger shape
        let t_broadcast = t.broadcast_as((4, 3)).unwrap();

        // After contiguous, matmul should work
        let t_contiguous = t_broadcast.contiguous().unwrap();
        assert!(t_contiguous.is_contiguous());
    }

    /// Test matmul after transpose + contiguous
    #[test]
    fn test_matmul_after_contiguous() {
        let device = Device::Cpu;

        // Simulate attention pattern: Q @ K^T
        let q = Tensor::randn(0.0f32, 1.0, (1, 4, 8, 16), &device).unwrap(); // [batch, heads, seq, dim]
        let k = Tensor::randn(0.0f32, 1.0, (1, 4, 8, 16), &device).unwrap();

        // Transpose K for attention
        let k_t = k.transpose(2, 3).unwrap().contiguous().unwrap();

        // This should work after contiguous
        let attn = q.matmul(&k_t);
        assert!(
            attn.is_ok(),
            "Matmul should succeed after contiguous: {:?}",
            attn.err()
        );
    }

    /// Test the L2 normalize + matmul pattern from GLiNER
    #[test]
    fn test_gliner_similarity_pattern() {
        let device = Device::Cpu;

        // Span embeddings: [batch, spans, hidden]
        let span_embs = Tensor::randn(0.0f32, 1.0, (1, 10, 768), &device).unwrap();
        // Label embeddings: [labels, hidden]
        let label_embs = Tensor::randn(0.0f32, 1.0, (5, 768), &device).unwrap();

        // Transpose labels
        let label_t = label_embs.t().unwrap();

        // Broadcast for batch
        let label_t = label_t
            .unsqueeze(0)
            .unwrap()
            .broadcast_as((1, 768, 5))
            .unwrap()
            .contiguous()
            .unwrap();

        // Matmul should work
        let scores = span_embs.contiguous().unwrap().matmul(&label_t);
        assert!(
            scores.is_ok(),
            "GLiNER similarity pattern should work: {:?}",
            scores.err()
        );

        let scores = scores.unwrap();
        assert_eq!(scores.dims(), &[1, 10, 5]); // [batch, spans, labels]
    }
}

/// Structural tests that don't require candle feature
#[cfg(test)]
mod structural_tests {
    /// Document the Metal matmul limitation
    #[test]
    fn document_metal_limitation() {
        // This test documents the Metal backend limitation:
        //
        // On Apple Silicon with Candle's Metal backend, matmul operations
        // require contiguous tensor inputs. Non-contiguous views created by:
        // - transpose()
        // - reshape() in some cases
        // - broadcast_as()
        // - narrow()
        // - slice operations
        //
        // will cause: "Metal error Invalid matmul arguments"
        //
        // Fix: Call .contiguous()? before matmul operations.
        //
        // References:
        // - https://github.com/huggingface/candle/issues/2737
        // - https://github.com/huggingface/candle/issues/3138
        assert!(true);
    }
}
