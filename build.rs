//! Build script to regenerate dataset_registry.rs from source files.
//!
//! The dataset registry is organized across multiple source files in
//! `anno/src/eval/dataset_registry_src/` for maintainability. This build script
//! concatenates them into the final `dataset_registry.rs` file.
//!
//! To regenerate manually: `cargo build` or run `scripts/generate_dataset_registry.py`

use std::fs;
use std::path::Path;

fn main() {
    // Rerun if any source files change
    println!("cargo:rerun-if-changed=anno/src/eval/dataset_registry_src/");
    println!("cargo:rerun-if-changed=anno/src/eval/dataset_registry.rs");

    let src_dir = Path::new("anno/src/eval/dataset_registry_src");
    let out_file = Path::new("anno/src/eval/dataset_registry.rs");

    // Read the existing file to extract header and impl block
    let existing = match fs::read_to_string(out_file) {
        Ok(content) => content,
        Err(_) => {
            // File doesn't exist yet - this is fine, we'll generate it
            return;
        }
    };

    let macro_start = match existing.find("define_datasets! {") {
        Some(pos) => pos,
        None => {
            eprintln!("cargo:warning=Could not find define_datasets! macro in existing file");
            return;
        }
    };
    let header = &existing[..macro_start];

    // Read all source files in order
    let source_files = [
        "ner_core.rs",
        "ner_biomedical.rs",
        "coref.rs",
        "re.rs",
        "el.rs",
    ];

    let mut dataset_content = String::new();
    dataset_content.push_str("define_datasets! {\n");

    for file in &source_files {
        let path = src_dir.join(file);
        let content = match fs::read_to_string(&path) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("cargo:warning=Failed to read {}: {}", path.display(), e);
                return;
            }
        };
        dataset_content.push_str(&content);
        if !content.ends_with('\n') {
            dataset_content.push('\n');
        }
    }

    // Only add closing brace if the last source file doesn't already have it
    // (el.rs ends with the macro closing brace)
    if !dataset_content.trim_end().ends_with('}') {
        dataset_content.push_str("}\n");
    }

    // Extract the impl block from the existing file
    let impl_start = match existing.find("#[allow(clippy::items_after_test_module)]") {
        Some(pos) => pos,
        None => {
            eprintln!("cargo:warning=Could not find impl block in existing file");
            return;
        }
    };
    let impl_block = &existing[impl_start..];

    // Combine everything
    let full_content = format!("{}{}\n{}", header, dataset_content, impl_block);

    // Only write if content changed (to avoid unnecessary recompilation)
    if full_content != existing {
        fs::write(out_file, full_content).expect("Failed to write dataset_registry.rs");
        println!("cargo:warning=Regenerated dataset_registry.rs from source files");
    }
}
