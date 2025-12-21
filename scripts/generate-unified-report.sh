#!/usr/bin/env bash
# Generate unified static analysis report from all tools
# Aggregates results from multiple static analysis tools into a single report

set -euo pipefail

REPORT_FILE="unified-static-analysis-report.md"
rm -f "$REPORT_FILE"

echo "# Unified Static Analysis Report" > "$REPORT_FILE"
echo "" >> "$REPORT_FILE"
echo "Generated: $(date -u +"%Y-%m-%d %H:%M:%S UTC")" >> "$REPORT_FILE"
echo "" >> "$REPORT_FILE"

# 1. Cargo-deny results
echo "## Dependency Security (cargo-deny)" >> "$REPORT_FILE"
echo "" >> "$REPORT_FILE"
if command -v cargo-deny &> /dev/null; then
    cargo deny check > .deny-tmp.txt 2>&1 || true
    echo '```' >> "$REPORT_FILE"
    cat .deny-tmp.txt >> "$REPORT_FILE" || true
    echo '```' >> "$REPORT_FILE"
    rm -f .deny-tmp.txt
else
    echo "ERROR: cargo-deny not installed" >> "$REPORT_FILE"
fi
echo "" >> "$REPORT_FILE"

# 2. Unused dependencies
echo "## Unused Dependencies (cargo-machete)" >> "$REPORT_FILE"
echo "" >> "$REPORT_FILE"
if command -v cargo-machete &> /dev/null; then
    cargo machete > .machete-tmp.txt 2>&1 || true
    echo '```' >> "$REPORT_FILE"
    cat .machete-tmp.txt >> "$REPORT_FILE" || true
    echo '```' >> "$REPORT_FILE"
    rm -f .machete-tmp.txt
else
    echo "ERROR: cargo-machete not installed" >> "$REPORT_FILE"
fi
echo "" >> "$REPORT_FILE"

# 3. Unsafe code statistics
echo "## Unsafe Code Statistics (cargo-geiger)" >> "$REPORT_FILE"
echo "" >> "$REPORT_FILE"
if command -v cargo-geiger &> /dev/null; then
    cargo geiger --quiet > .geiger-tmp.txt 2>&1 || true
    echo '```' >> "$REPORT_FILE"
    cat .geiger-tmp.txt >> "$REPORT_FILE" || true
    echo '```' >> "$REPORT_FILE"
    rm -f .geiger-tmp.txt
else
    echo "ERROR: cargo-geiger not installed" >> "$REPORT_FILE"
fi
echo "" >> "$REPORT_FILE"

# 4. OpenGrep results summary
echo "## Security Pattern Detection (OpenGrep)" >> "$REPORT_FILE"
echo "" >> "$REPORT_FILE"
if command -v opengrep &> /dev/null; then
    echo "### Default Security Rules" >> "$REPORT_FILE"
    opengrep scan --config auto --quiet --json --output opengrep-results.json anno/ anno-core/ anno-coalesce/ anno-strata/ tests/ examples/ 2>/dev/null || echo '{"results":[]}' > opengrep-results.json
    if command -v jq &> /dev/null; then
        jq -r '.results[0:20][] | "\(.check_id): \(.path):\(.start.line)"' opengrep-results.json >> "$REPORT_FILE" 2>/dev/null || true
    else
        echo "Install jq to summarize opengrep-results.json" >> "$REPORT_FILE"
    fi
    echo "" >> "$REPORT_FILE"
    
    echo "### Custom Rules Summary" >> "$REPORT_FILE"
    for rule_file in .opengrep/rules/*.yaml; do
        if [ -f "$rule_file" ]; then
            case "$(basename "$rule_file")" in
                rust-unicode-offsets.yaml|rust-candle-metal.yaml)
                    # These are ast-grep rules (not OpenGrep/Semgrep rules).
                    continue
                    ;;
            esac
            rule_name=$(basename "$rule_file" .yaml)
            count=$(opengrep scan -f "$rule_file" --quiet --json anno/ anno-core/ anno-coalesce/ anno-strata/ 2>/dev/null | jq '.results | length' || echo "0")
            echo "- $rule_name: $count findings" >> "$REPORT_FILE"
        fi
    done
else
    echo "ERROR: opengrep not installed" >> "$REPORT_FILE"
fi
echo "" >> "$REPORT_FILE"

# 5. Docs hygiene checks (fast, offline)
echo "## Docs Hygiene (docs-audit)" >> "$REPORT_FILE"
echo "" >> "$REPORT_FILE"
if [ -f "scripts/docs_audit.py" ]; then
    python3 scripts/docs_audit.py > .docs-audit-tmp.txt 2>&1 || true
    echo '```' >> "$REPORT_FILE"
    cat .docs-audit-tmp.txt >> "$REPORT_FILE" || true
    echo '```' >> "$REPORT_FILE"
    rm -f .docs-audit-tmp.txt
else
    echo "INFO: scripts/docs_audit.py not found" >> "$REPORT_FILE"
fi
echo "" >> "$REPORT_FILE"

# 6. AST Pattern Checks (ast-grep)
echo "## AST Pattern Checks (ast-grep)" >> "$REPORT_FILE"
echo "" >> "$REPORT_FILE"
if command -v ast-grep &> /dev/null; then
    echo "### Unicode/Offset Safety (ast-grep)" >> "$REPORT_FILE"
    echo "" >> "$REPORT_FILE"
    ast-grep scan --rule .opengrep/rules/rust-unicode-offsets.yaml --json=compact anno/src/ anno-core/src/ anno-coalesce/src/ anno-strata/src/ > .ast-grep-unicode-tmp.json 2>/dev/null || echo "[]" > .ast-grep-unicode-tmp.json
    if command -v jq &> /dev/null; then
        UNICODE_COUNT=$(jq -r 'length' .ast-grep-unicode-tmp.json 2>/dev/null || echo 0)
        echo "- **Findings**: ${UNICODE_COUNT}" >> "$REPORT_FILE"
        echo "" >> "$REPORT_FILE"
        echo "Top matches:" >> "$REPORT_FILE"
        echo '```' >> "$REPORT_FILE"
        jq -r '.[0:20][] | "\(.ruleId): \(.file):\(.range.start.line + 1):\(.range.start.column + 1)  \(.message | split("\n")[0])"' .ast-grep-unicode-tmp.json 2>/dev/null >> "$REPORT_FILE" || true
        echo '```' >> "$REPORT_FILE"
    else
        echo "Install jq to summarize ast-grep results." >> "$REPORT_FILE"
    fi
    rm -f .ast-grep-unicode-tmp.json
    echo "" >> "$REPORT_FILE"
    
    echo "### Metal/Candle Contiguity (ast-grep)" >> "$REPORT_FILE"
    echo "" >> "$REPORT_FILE"
    ast-grep scan --rule .opengrep/rules/rust-candle-metal.yaml --json=compact anno/src/ > .ast-grep-metal-tmp.json 2>/dev/null || echo "[]" > .ast-grep-metal-tmp.json
    if command -v jq &> /dev/null; then
        METAL_COUNT=$(jq -r 'length' .ast-grep-metal-tmp.json 2>/dev/null || echo 0)
        echo "- **Findings**: ${METAL_COUNT}" >> "$REPORT_FILE"
        echo "" >> "$REPORT_FILE"
        echo "Top matches:" >> "$REPORT_FILE"
        echo '```' >> "$REPORT_FILE"
        jq -r '.[0:20][] | "\(.ruleId): \(.file):\(.range.start.line + 1):\(.range.start.column + 1)  \(.message | split("\n")[0])"' .ast-grep-metal-tmp.json 2>/dev/null >> "$REPORT_FILE" || true
        echo '```' >> "$REPORT_FILE"
    else
        echo "Install jq to summarize ast-grep results." >> "$REPORT_FILE"
    fi
    rm -f .ast-grep-metal-tmp.json
else
    echo "INFO: ast-grep not installed" >> "$REPORT_FILE"
fi
echo "" >> "$REPORT_FILE"

# 7. Repo-specific checks
echo "## Repo-Specific Pattern Checks" >> "$REPORT_FILE"
echo "" >> "$REPORT_FILE"
if [ -f "repo-specific-analysis.md" ]; then
    cat repo-specific-analysis.md >> "$REPORT_FILE"
else
    echo "INFO: Repo-specific analysis not available" >> "$REPORT_FILE"
fi
echo "" >> "$REPORT_FILE"

# 8. Summary
echo "## Summary" >> "$REPORT_FILE"
echo "" >> "$REPORT_FILE"
echo "This report aggregates results from:" >> "$REPORT_FILE"
echo "- cargo-deny (dependency security)" >> "$REPORT_FILE"
echo "- cargo-machete (unused dependencies)" >> "$REPORT_FILE"
echo "- cargo-geiger (unsafe code statistics)" >> "$REPORT_FILE"
echo "- opengrep (security pattern detection)" >> "$REPORT_FILE"
echo "- docs-audit (docs hygiene)" >> "$REPORT_FILE"
echo "- ast-grep (AST checks)" >> "$REPORT_FILE"
echo "- Repo-specific pattern checks" >> "$REPORT_FILE"
echo "" >> "$REPORT_FILE"
echo "For detailed results, see individual tool outputs in CI artifacts." >> "$REPORT_FILE"

echo "Unified report generated: $REPORT_FILE"

