#!/bin/bash
# Upload model weights to S3 for reproducibility
# Run from project root

set -e

S3_BUCKET="${ANNO_S3_BUCKET:-arc-anno-data}"
HF_CACHE="${HF_HOME:-$HOME/.cache/huggingface}/hub"

echo "=== Uploading models to s3://$S3_BUCKET/models/ ==="
echo ""

# Models to upload (in priority order)
declare -A MODELS=(
    # Critical - for basic NER functionality
    ["protectai--bert-base-NER-onnx"]="ner/bert-ner-onnx"
    ["knowledgator--gliner-x-small"]="ner/gliner/gliner-x-small"
    ["juampahc--gliner_multi-v2.1-onnx"]="ner/gliner/gliner-multi-v2.1"
    
    # Important - for full NER features
    ["deepanwa--NuNerZero_onnx"]="ner/nuner/nuner-zero-onnx"
    ["dbmdz--bert-large-cased-finetuned-conll03-english"]="ner/bert-large-conll03"
    ["answerdotai--ModernBERT-base"]="encoders/modernbert-base"
    
    # Nice to have - additional NER models
    ["dslim--bert-base-NER"]="ner/bert-base-ner"
    ["BAAI--bge-large-en-v1.5"]="encoders/bge-large-en"
    
    # Coreference Resolution Models
    ["shtoshni--longformer_coreference_ontonotes"]="coref/longformer-ontonotes"
    ["shtoshni--spanbert_coreference_large"]="coref/spanbert-large"
    ["shtoshni--spanbert_coreference_base"]="coref/spanbert-base"
    
    # LLM-based Coreference (smaller models)
    ["hsiehpinghan--Qwen2-0.5B-Instruct-Coreference-Resolution"]="coref/qwen2-0.5b-coref"
)

# Check for s5cmd
if ! command -v s5cmd &> /dev/null; then
    echo "ERROR: s5cmd not found. Install with: brew install peak/tap/s5cmd"
    exit 1
fi

uploaded=0
skipped=0
failed=0

for model in "${!MODELS[@]}"; do
    local_path="$HF_CACHE/models--$model"
    s3_path="${MODELS[$model]}"
    
    if [ ! -d "$local_path" ]; then
        echo "SKIP: $model (not in local cache)"
        ((skipped++))
        continue
    fi
    
    # Get local size
    local_size=$(du -sh "$local_path" 2>/dev/null | cut -f1)
    
    echo "Uploading: $model ($local_size) -> s3://$S3_BUCKET/models/$s3_path/"
    
    if s5cmd sync "$local_path/*" "s3://$S3_BUCKET/models/$s3_path/" 2>&1; then
        echo "  OK"
        ((uploaded++))
    else
        echo "  FAILED"
        ((failed++))
    fi
done

echo ""
echo "=== Summary ==="
echo "Uploaded: $uploaded"
echo "Skipped:  $skipped"
echo "Failed:   $failed"

# Generate manifest
echo ""
echo "=== Generating models manifest ==="
cat > /tmp/models_manifest.json << 'EOF'
{
  "generated_at": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
  "bucket": "s3://arc-anno-data",
  "models": {
EOF

s5cmd ls "s3://$S3_BUCKET/models/**" 2>/dev/null | while read -r line; do
    size=$(echo "$line" | awk '{print $1}')
    path=$(echo "$line" | awk '{print $NF}')
    echo "    \"$path\": {\"size_bytes\": $size},"
done >> /tmp/models_manifest.json

echo '    "_end": {}
  }
}' >> /tmp/models_manifest.json

# Upload manifest
s5cmd cp /tmp/models_manifest.json "s3://$S3_BUCKET/manifests/models.json"
echo "Manifest uploaded to s3://$S3_BUCKET/manifests/models.json"

