//! Smoke-load: fetch the INT8 ONNX, build the ort session, assert the
//! input/output contract, run one forward pass. Ignored by default —
//! downloads ~571 MB. Run: `cargo test -p anno-rag --features rerank
//! --test rerank_smoke -- --ignored --nocapture`.
#![cfg(feature = "rerank")]

#[tokio::test]
#[ignore = "downloads ~571 MB BGE-reranker-v2-m3 INT8 ONNX"]
async fn onnx_io_contract_holds() {
    use hf_hub::api::tokio::Api;
    use ort::session::{builder::GraphOptimizationLevel, Session};

    let api = Api::new().expect("hf api");
    let repo = api.model("onnx-community/bge-reranker-v2-m3-ONNX".to_string());
    let onnx = repo
        .get("onnx/model_int8.onnx")
        .await
        .expect("fetch model_int8.onnx");
    let _tok = repo.get("tokenizer.json").await.expect("fetch tokenizer");

    let session = Session::builder()
        .expect("builder")
        .with_optimization_level(GraphOptimizationLevel::Level3)
        .expect("opt level")
        .commit_from_file(&onnx)
        .expect("commit onnx");

    let in_names: Vec<String> = session.inputs().iter().map(|i| i.name().to_string()).collect();
    let out_names: Vec<String> = session.outputs().iter().map(|o| o.name().to_string()).collect();
    eprintln!("ONNX inputs={in_names:?} outputs={out_names:?}");

    assert!(
        in_names.iter().any(|n| n == "input_ids"),
        "expected an 'input_ids' input, got {in_names:?}"
    );
    assert!(
        in_names.iter().any(|n| n == "attention_mask"),
        "expected an 'attention_mask' input, got {in_names:?}"
    );
    assert_eq!(
        out_names.len(),
        1,
        "expected a single logits output, got {out_names:?}"
    );

    let ids = ndarray::Array2::<i64>::zeros((1, 4));
    let mask = ndarray::Array2::<i64>::from_elem((1, 4), 1i64);

    let ids_shape: Vec<usize> = ids.shape().to_vec();
    let (ids_data, _) = ids.into_raw_vec_and_offset();
    let ids_t = ort::value::Tensor::from_array((ids_shape, ids_data.into_boxed_slice()))
        .expect("ids tensor");

    let mask_shape: Vec<usize> = mask.shape().to_vec();
    let (mask_data, _) = mask.into_raw_vec_and_offset();
    let mask_t = ort::value::Tensor::from_array((mask_shape, mask_data.into_boxed_slice()))
        .expect("mask tensor");

    let mut session = session;
    let out = session
        .run(ort::inputs![
            "input_ids" => ids_t.into_dyn(),
            "attention_mask" => mask_t.into_dyn(),
        ])
        .expect("forward run");
    let logits_val = out.values().next().expect("one output");
    let (shape, _data) = logits_val
        .try_extract_tensor::<f32>()
        .expect("extract f32 logits");
    eprintln!("logits shape={shape:?}");
    assert_eq!(shape[0], 1, "batch dim must be 1");
}
