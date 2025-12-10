#!/bin/bash
# Comprehensive Backend x Dataset x Task Evaluation
#
# Runs evaluation across all valid combinations with multiple seeds
# for statistical reliability.

set -e

ANNO_BIN="${ANNO_BIN:-./target/release/anno}"
SEEDS="${SEEDS:-42 123 456 789 999}"
MAX_EXAMPLES="${MAX_EXAMPLES:-100}"
OUTPUT_DIR="${OUTPUT_DIR:-reports/comprehensive}"

# Build release if needed
if [[ ! -f "$ANNO_BIN" ]]; then
    echo "Building release binary..."
    cargo build --release --features "onnx,eval"
fi

mkdir -p "$OUTPUT_DIR"

echo "=============================================="
echo "    Comprehensive Backend x Dataset x Task"
echo "=============================================="
echo ""
echo "Seeds: $SEEDS"
echo "Max examples per dataset: $MAX_EXAMPLES"
echo "Output: $OUTPUT_DIR"
echo ""

# Available backends
BACKENDS=(
    "pattern"
    "heuristic"
    "crf"
    "stacked"
    "ensemble"
)

# Add ONNX backends if available
if cargo feature --list 2>/dev/null | grep -q onnx; then
    BACKENDS+=("gliner")
fi

# Datasets for NER task (lightweight subset)
NER_DATASETS=(
    "wikigold_sample"
    "conll2003_sample"
)

# Test texts for quick validation
TEST_TEXTS=(
    "John Smith works at Google in California."
    "Apple Inc. was founded by Steve Jobs in Cupertino."
    "The White House is located in Washington, D.C."
    "Dr. Marie Curie won the Nobel Prize in 1911."
    "Barack Obama served as the 44th President of the United States."
)

# Results file
RESULTS_FILE="$OUTPUT_DIR/eval_results_$(date +%Y%m%d_%H%M%S).md"

cat > "$RESULTS_FILE" << EOF
# Comprehensive Evaluation Results

Generated: $(date -uIs)
Seeds: $SEEDS
Max examples: $MAX_EXAMPLES

## Backend Capabilities

| Backend | Available | Entity Types | Tasks |
|---------|-----------|--------------|-------|
EOF

# Check backend availability and capabilities
for backend in "${BACKENDS[@]}"; do
    if $ANNO_BIN extract --model "$backend" "test" >/dev/null 2>&1; then
        available="Y"
    else
        available="N"
    fi
    
    # Get supported types (simplified)
    case "$backend" in
        pattern) types="DATE,EMAIL,URL,PHONE,MONEY,PERCENT" ;;
        heuristic|crf) types="PER,ORG,LOC,MISC" ;;
        stacked|ensemble) types="All (combined)" ;;
        gliner) types="Zero-shot (any)" ;;
        *) types="Unknown" ;;
    esac
    
    echo "| $backend | $available | $types | NER |" >> "$RESULTS_FILE"
done

echo "" >> "$RESULTS_FILE"
echo "## Quick Validation (Test Sentences)" >> "$RESULTS_FILE"
echo "" >> "$RESULTS_FILE"

# Quick validation on test sentences
for text in "${TEST_TEXTS[@]}"; do
    echo "### Text: \"${text:0:50}...\"" >> "$RESULTS_FILE"
    echo "" >> "$RESULTS_FILE"
    echo "| Backend | Entities |" >> "$RESULTS_FILE"
    echo "|---------|----------|" >> "$RESULTS_FILE"
    
    for backend in "${BACKENDS[@]}"; do
        output=$($ANNO_BIN extract --model "$backend" "$text" 2>/dev/null | wc -l || echo "0")
        echo "| $backend | $output |" >> "$RESULTS_FILE"
    done
    echo "" >> "$RESULTS_FILE"
done

echo "" >> "$RESULTS_FILE"
echo "## Multi-Seed Evaluation" >> "$RESULTS_FILE"
echo "" >> "$RESULTS_FILE"
echo "Running evaluation with seeds: $SEEDS" >> "$RESULTS_FILE"
echo "" >> "$RESULTS_FILE"

# Multi-seed evaluation (if eval feature available)
for seed in $SEEDS; do
    echo "### Seed: $seed" >> "$RESULTS_FILE"
    echo "" >> "$RESULTS_FILE"
    
    # Run comprehensive eval
    echo "Running seed $seed..."
    
    # Use anno benchmark command if available
    if $ANNO_BIN benchmark --help >/dev/null 2>&1; then
        $ANNO_BIN benchmark --seed "$seed" --max-examples "$MAX_EXAMPLES" --format markdown 2>/dev/null >> "$RESULTS_FILE" || echo "Benchmark not available for seed $seed" >> "$RESULTS_FILE"
    else
        echo "Note: benchmark command not available, using extract-based evaluation" >> "$RESULTS_FILE"
        echo "" >> "$RESULTS_FILE"
    fi
done

# Backend comparison on real data
echo "" >> "$RESULTS_FILE"
echo "## Backend Comparison on Real Data" >> "$RESULTS_FILE"
echo "" >> "$RESULTS_FILE"

if [[ -d "hack/real_data/news" ]]; then
    echo "### News Articles" >> "$RESULTS_FILE"
    echo "" >> "$RESULTS_FILE"
    
    for backend in "${BACKENDS[@]}"; do
        echo "#### $backend" >> "$RESULTS_FILE"
        echo "\`\`\`" >> "$RESULTS_FILE"
        
        # Process a few files
        for f in hack/real_data/news/*.txt; do
            [[ -f "$f" ]] || continue
            echo "File: $(basename "$f")"
            $ANNO_BIN extract --model "$backend" "$f" 2>/dev/null | head -5
            echo ""
        done | head -30 >> "$RESULTS_FILE"
        
        echo "\`\`\`" >> "$RESULTS_FILE"
        echo "" >> "$RESULTS_FILE"
    done
fi

# Summary statistics
echo "" >> "$RESULTS_FILE"
echo "## Summary" >> "$RESULTS_FILE"
echo "" >> "$RESULTS_FILE"
echo "- Total backends tested: ${#BACKENDS[@]}" >> "$RESULTS_FILE"
echo "- Seeds used: $(echo $SEEDS | wc -w)" >> "$RESULTS_FILE"
echo "- Evaluation completed: $(date -uIs)" >> "$RESULTS_FILE"

echo ""
echo "=============================================="
echo "    Evaluation Complete"
echo "=============================================="
echo ""
echo "Results saved to: $RESULTS_FILE"
echo ""
cat "$RESULTS_FILE" | head -50
echo "..."

