//! Minimal NuNER evaluation example.
//!
//! This reads a few hard-coded sentences and runs NuNER in zero-shot mode to
//! illustrate coarse vs. fine label packs.

#[cfg(feature = "onnx")]
fn main() -> anyhow::Result<()> {
    use anno::backends::nuner::{LabelPack, NuNER};
    use anno::Model;

    let ner = NuNER::from_pretrained("deepanwa/NuNerZero_onnx")?;

    let sentences = [
        "Marie Curie discovered radium in Paris.",
        "The Dalmatian chased a frisbee in the park.",
    ];

    for (pack, name) in [
        (LabelPack::Coarse, "coarse"),
        (LabelPack::Fine, "fine"),
        (LabelPack::Cner, "cner"),
    ] {
        let ner = NuNER::from_pretrained("deepanwa/NuNerZero_onnx")?.with_label_pack(pack);
        println!("== Pack: {} ==", name);
        for s in sentences {
            let entities = ner.extract_entities(s, None)?;
            println!("{s}");
            for e in entities {
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
    println!("Enable the `onnx` feature to run the NuNER eval example.");
}

