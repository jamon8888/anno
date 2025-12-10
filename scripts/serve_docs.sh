#!/bin/bash
# Serve workspace documentation locally
# Usage: ./scripts/serve_docs.sh [port]

set -e

PORT="${1:-8000}"
DOC_DIR="target/doc"

# Build docs if needed
if [ ! -d "$DOC_DIR" ]; then
    echo "Building workspace documentation..."
    cargo doc --workspace --no-deps
fi

echo "Serving docs at http://localhost:$PORT"
echo "Main crates:"
echo "  - http://localhost:$PORT/anno/index.html"
echo "  - http://localhost:$PORT/anno_core/index.html"
echo "  - http://localhost:$PORT/anno_cli/index.html"
echo ""
echo "Press Ctrl+C to stop"

# Use Python's built-in HTTP server (available on macOS)
cd "$DOC_DIR"
python3 -m http.server "$PORT"

