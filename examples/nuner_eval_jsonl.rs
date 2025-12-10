//! Minimal JSONL evaluation for NuNER.
//!
//! Usage:
//! `NUNER_ONNX_MODEL=deepanwa/NuNerZero_onnx cargo run --example nuner_eval_jsonl --features onnx -- tests/data/ner/coarse_smoke.jsonl`

#[cfg(feature = "onnx")]
fn main() -> anyhow::Result<()> {
    use anno::backends::nuner::{LabelPack, NuNER};
    use anno::Model;
    use serde::Deserialize;
    use std::env;
    use std::fs::File;
    use std::io::{BufRead, BufReader};
    use std::path::Path;

    #[derive(Deserialize)]
    struct Record {
        text: String,
        labels: Vec<String>,
    }

    let args: Vec<String> = env::args().collect();
    let path = args
        .get(1)
        .map(|s| s.as_str())
        .unwrap_or("tests/data/ner/coarse_smoke.jsonl");

    let model_id = env::var("NUNER_ONNX_MODEL").unwrap_or_else(|_| {
        "deepanwa/NuNerZero_onnx".to_string()
    });

    if !Path::new(path).exists() {
        anyhow::bail!("Fixture not found: {}", path);
    }

    let file = File::open(path)?;
    let reader = BufReader::new(file);

    let ner = NuNER::from_pretrained(&model_id)?;

    for (pack, name) in [
        (LabelPack::Coarse, "coarse"),
        (LabelPack::Fine, "fine"),
        (LabelPack::Cner, "cner"),
    ] {
        let ner = NuNER::from_pretrained(&model_id)?.with_label_pack(pack);
        println!("== Pack: {} ==", name);
        for line in reader.by_ref().lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            let rec: Record = serde_json::from_str(&line)?;
            let labels: Vec<&str> = rec.labels.iter().map(|s| s.as_str()).collect();
            let ents = ner.extract(rec.text.as_str(), &labels, 0.5)?;
            println!("{}", rec.text);
            for e in ents {
                println!(
                    "  span='{}' type={:?} conf={:.2}",
                    e.text, e.entity_type, e.confidence
                );
            }
        }
    }

    Ok(())
}

#[cfg(not(feature = "onnx"))]
fn main() {
    println!("Enable the `onnx` feature to run this example.");
}

