use super::*;

#[test]
fn test_encoder_config_defaults() {
    let bert = EncoderConfig::bert_base();
    assert_eq!(bert.hidden_size, 768);
    assert_eq!(bert.max_position_embeddings, 512);
    assert!(!bert.use_rope);

    let modern = EncoderConfig::modernbert_base();
    assert_eq!(modern.hidden_size, 768);
    assert_eq!(modern.max_position_embeddings, 8192);
    assert!(modern.use_rope);
    assert!(modern.use_geglu);
}

#[test]
fn test_modernbert_large() {
    let config = EncoderConfig::modernbert_large();
    assert_eq!(config.hidden_size, 1024);
    assert_eq!(config.num_hidden_layers, 28);
}

#[cfg(feature = "candle")]
#[test]
fn test_geglu() {
    use candle_core::{Device, Tensor};

    let device = Device::Cpu;
    let x = Tensor::randn(0f32, 1., (2, 8), &device).unwrap();
    let result = super::implementations::candle_impl::geglu(&x);
    assert!(result.is_ok());
    let result = result.unwrap();
    assert_eq!(result.dims(), &[2, 4]);
}
