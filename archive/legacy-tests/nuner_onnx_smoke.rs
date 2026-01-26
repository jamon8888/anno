//! ONNX-gated smoke test for NuNER. Skips unless NUNER_ONNX_MODEL is set.

#[cfg(feature = "onnx")]
#[test]
fn coarse_smoke_eval() {
    use anno::backends::nuner::NuNER;
    use anno::Model;
    use serde::Deserialize;
    use std::fs::File;
    use std::io::{BufRead, BufReader};

    let model_id = match std::env::var("NUNER_ONNX_MODEL") {
        Ok(v) => v,
        Err(_) => {
            eprintln!("Skipping: set NUNER_ONNX_MODEL to run ONNX smoke test");
            return;
        }
    };

    #[derive(Deserialize)]
    struct Record {
        text: String,
        labels: Vec<String>,
    }

    let path = "tests/data/ner/coarse_smoke.jsonl";
    let file = File::open(path).expect("coarse_smoke.jsonl missing");
    let reader = BufReader::new(file);

    let ner = NuNER::from_pretrained(&model_id).expect("load model");

    let mut total = 0usize;
    let mut non_empty = 0usize;

    for line in reader.lines() {
        let line = line.expect("read line");
        if line.trim().is_empty() {
            continue;
        }
        let rec: Record = serde_json::from_str(&line).expect("parse jsonl");
        let labels: Vec<&str> = rec.labels.iter().map(|s| s.as_str()).collect();
        let ents = ner
            .extract(rec.text.as_str(), &labels, 0.5)
            .expect("extract");
        total += 1;
        if !ents.is_empty() {
            non_empty += 1;
        }
    }

    assert_eq!(total, 3);
    assert!(non_empty >= 2, "expected at least 2 texts with hits");
}

#[cfg(not(feature = "onnx"))]
#[test]
fn coarse_smoke_eval_skipped_without_onnx() {
    eprintln!("Skipping ONNX smoke; enable `onnx` feature to run.");
}
