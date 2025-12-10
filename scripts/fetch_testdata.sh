#!/bin/bash
# Fetch real-world test data from URLs for regression testing
# Uses curl to fetch actual content (HTML, PDF, JSON, text, etc.)

set -e

TESTDATA_DIR="testdata/real_world"
mkdir -p "$TESTDATA_DIR"

# List of URLs to fetch (diverse content types)
URLS=(
    # HTML pages
    "https://climate.ec.europa.eu/eu-action/european-climate-law_en"
    "https://www.consilium.europa.eu/en/policies/climate-change/"
    "https://openai.com/policies/usage-policies/"
    "https://news.un.org/en/story/2024/09/1154541"
    "https://arxiv.org/abs/2309.14084"
    
    # Research papers (PDFs)
    "https://arxiv.org/pdf/2309.14084"
    "https://arxiv.org/pdf/2101.00884"
    
    # Academic abstracts
    "https://arxiv.org/abs/2101.00884"
    
    # JSON datasets (if available)
    # "https://huggingface.co/datasets/milistu/Wikigold-NER-conll/raw/main/train.json"
)

echo "Fetching ${#URLS[@]} URLs for test data..."

for url in "${URLS[@]}"; do
    # Extract filename from URL
    filename=$(echo "$url" | sed 's|https\?://||' | sed 's|/|_|g' | sed 's|\.|_|g' | head -c 100)
    
    # Determine extension based on URL
    if [[ "$url" == *.pdf ]] || [[ "$url" == *"/pdf" ]]; then
        ext="pdf"
    elif [[ "$url" == *.json ]]; then
        ext="json"
    elif [[ "$url" == *.txt ]]; then
        ext="txt"
    else
        ext="html"
    fi
    
    output_file="${TESTDATA_DIR}/${filename}.${ext}"
    
    echo "Fetching: $url -> $output_file"
    curl -sL "$url" -o "$output_file" || {
        echo "Warning: Failed to fetch $url"
        continue
    }
    
    # Check if file is empty or too small (likely a redirect/error)
    if [ ! -s "$output_file" ] || [ $(stat -f%z "$output_file" 2>/dev/null || stat -c%s "$output_file" 2>/dev/null || echo 0) -lt 100 ]; then
        echo "Warning: $output_file is empty or too small, might be a redirect"
        rm -f "$output_file"
    else
        echo "  ✓ Saved $(wc -l < "$output_file" | tr -d ' ') lines"
    fi
done

echo ""
echo "Done! Test data saved to $TESTDATA_DIR"
echo "Files:"
ls -lh "$TESTDATA_DIR" | grep -v "^total" | awk '{print "  " $9 " (" $5 ")"}'

