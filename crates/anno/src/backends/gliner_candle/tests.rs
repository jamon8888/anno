#[cfg(feature = "candle")]
use super::layers::l2_normalize;
use super::*;

#[test]
fn test_stub_without_feature() {
    #[cfg(not(feature = "candle"))]
    {
        let result = GLiNERCandle::new("test");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("candle"));
    }
}

#[cfg(feature = "candle")]
#[test]
fn test_span_label_matcher() {
    let device = Device::Cpu;
    let matcher = SpanLabelMatcher::new(1.0);

    let span_embs = Tensor::randn(0f32, 1., (1, 10, 64), &device).unwrap();
    let label_embs = Tensor::randn(0f32, 1., (3, 64), &device).unwrap();

    let scores = matcher.forward(&span_embs, &label_embs).unwrap();
    assert_eq!(scores.dims(), &[1, 10, 3]);
}

#[cfg(feature = "candle")]
#[test]
fn test_l2_normalize() {
    let device = Device::Cpu;
    let x = Tensor::from_vec(vec![3.0f32, 4.0], (1, 2), &device).unwrap();
    let normed = l2_normalize(&x, D::Minus1).unwrap();

    // Should be [0.6, 0.8] (3/5, 4/5)
    let values = normed.flatten_all().unwrap().to_vec1::<f32>().unwrap();
    assert!((values[0] - 0.6).abs() < 0.01);
    assert!((values[1] - 0.8).abs() < 0.01);
}
