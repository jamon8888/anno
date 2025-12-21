# LLM CLI Integration

How to enable LLMs to use the `anno` CLI tool for entity extraction, verification, and analysis.

> Note: This is a design document. Some examples below are schematic and may not exactly match
> the current CLI flags/output. For current behavior, run `anno --help`.

## Overview

LLMs can interact with `anno` CLI in several ways:

1. **Direct shell access** - LLM agents with terminal access
2. **MCP (Model Context Protocol)** - Structured tool interface
3. **Subprocess wrapper** - API that calls CLI internally

## Benefits

| Benefit | Description |
|---------|-------------|
| **Verification** | LLM can verify its own entity extractions against `anno` |
| **Augmentation** | Enhance LLM responses with structured entity data |
| **Grounding** | Link entities to knowledge bases via `anno cross-doc` |
| **Evaluation** | LLM-as-evaluator for comparing annotation quality |

## 1. Direct Shell Access

LLM agents (like Claude with terminal tools) can call `anno` directly:

```bash
# LLM extracts entities and reasons about them
$ anno extract --format json "The CEO of Apple, Tim Cook, visited Paris."
{
  "provenance": {"tool": "anno", "version": "0.2.0", ...},
  "entities": [
    {"id": "e:abc123", "text": "Apple", "type": "ORG", ...},
    {"id": "e:def456", "text": "Tim Cook", "type": "PER", ...},
    {"id": "e:ghi789", "text": "Paris", "type": "LOC", ...}
  ]
}
```

The LLM can then analyze the JSON output, compare with its own understanding, and provide enriched responses.

## 2. MCP Server (Model Context Protocol)

Create an MCP server that exposes `anno` as tools:

```python
# anno_mcp_server.py
from mcp import Server
import subprocess
import json

server = Server("anno-mcp")

@server.tool()
async def extract_entities(text: str, types: list[str] = None) -> dict:
    """Extract named entities from text using anno CLI."""
    cmd = ["anno", "extract", "--format", "json"]
    if types:
        for t in types:
            cmd.extend(["--label", t])
    cmd.append(text)
    
    result = subprocess.run(cmd, capture_output=True, text=True)
    return json.loads(result.stdout)

@server.tool()
async def explain_entity(text: str, span: str = None) -> str:
    """Explain why an entity was classified a certain way."""
    cmd = ["anno", "explain", "--text", text]
    if span:
        cmd.extend(["--span", span])
    
    result = subprocess.run(cmd, capture_output=True, text=True)
    return result.stdout

@server.tool()
async def cross_doc_entities(directory: str, threshold: float = 0.6) -> dict:
    """Cluster entities across multiple documents."""
    cmd = ["anno", "cross-doc", directory,
           "--threshold", str(threshold), 
           "--format", "json"]
    
    result = subprocess.run(cmd, capture_output=True, text=True)
    return json.loads(result.stdout)
```

### MCP Server Installation

```bash
# Install MCP server (Python)
pip install mcp anno-mcp  # hypothetical package

# Run server
python -m anno_mcp_server

# Or use uv
uvx --from anno-mcp anno-mcp-server
```

### Cursor/Claude Desktop Integration

Add to `.cursor/mcp.json`:

```json
{
  "mcpServers": {
    "anno": {
      "command": "uvx",
      "args": ["--from", "anno-mcp", "anno-mcp-server"]
    }
  }
}
```

## 3. LLM-as-Evaluator Pattern

Use an LLM to evaluate `anno` output quality:

```bash
#!/bin/bash
# llm_eval.sh - Use LLM to evaluate anno extraction quality

TEXT="Dr. Sarah Chen from MIT won the Nobel Prize for her work on CRISPR."

# Get anno's extraction
ANNO_OUTPUT=$(anno extract --format json "$TEXT")

# Ask LLM to evaluate
cat << EOF | llm --model claude-3.5-sonnet
You are an expert NER evaluator. Analyze this entity extraction output:

Input text: $TEXT

Anno output:
$ANNO_OUTPUT

Evaluate:
1. Are all entities correctly identified?
2. Are there any missing entities?
3. Are entity types correct?
4. Rate overall quality (1-5)

Respond in JSON format with fields: correct_entities, missing_entities, type_errors, quality_score, explanation.
EOF
```

## 4. Self-Verification Workflow

LLMs can use `anno` to verify their own extractions:

```python
# llm_verification.py
import subprocess
import json

def verify_llm_extraction(text: str, llm_entities: list[dict]) -> dict:
    """
    Compare LLM-extracted entities against anno output.
    
    Returns dict with:
    - matched: entities both found
    - llm_only: entities only LLM found
    - anno_only: entities only anno found
    - type_mismatches: same span, different type
    """
    # Get anno extraction
    result = subprocess.run(
        ["anno", "extract", "--format", "json", text],
        capture_output=True, text=True
    )
    anno_entities = json.loads(result.stdout)["entities"]
    
    # Build comparison sets
    llm_set = {(e["text"], e["type"]) for e in llm_entities}
    anno_set = {(e["text"], e["type"]) for e in anno_entities}
    
    matched = llm_set & anno_set
    llm_only = llm_set - anno_set
    anno_only = anno_set - llm_set
    
    # Check type mismatches
    llm_texts = {e["text"]: e["type"] for e in llm_entities}
    anno_texts = {e["text"]: e["type"] for e in anno_entities}
    
    type_mismatches = []
    for text, llm_type in llm_texts.items():
        if text in anno_texts and anno_texts[text] != llm_type:
            type_mismatches.append({
                "text": text,
                "llm_type": llm_type,
                "anno_type": anno_texts[text]
            })
    
    return {
        "matched": list(matched),
        "llm_only": list(llm_only),
        "anno_only": list(anno_only),
        "type_mismatches": type_mismatches,
        "agreement_rate": len(matched) / max(len(llm_set), len(anno_set), 1)
    }
```

## 5. Use Cases

### Knowledge Graph Construction
```bash
# LLM extracts entities, anno provides structured output
anno extract --format json "$TEXT" | \
  llm "Convert these entities to Cypher statements for Neo4j"
```

### Document Summarization with Entity Linking
```bash
# Extract entities with KB links
anno extract --format json --link-kb "$TEXT" | \
  llm "Summarize this document, using the entity links for context"
```

### Fact-Checking Assistance
```bash
# LLM claim → anno extraction → verification
CLAIM="Tim Cook founded Apple in 1976"
anno extract --expected-types PER,ORG,DATE "$CLAIM" 2>&1 | \
  llm "This claim has these entities. Is the claim factually accurate?"
```

### Research Paper Analysis
```bash
# Batch process papers, LLM synthesizes
find papers/ -name "*.pdf" | xargs -P4 -I{} anno extract --format jsonl {} | \
  llm "Synthesize the key findings across these papers based on the entities"
```

## 6. Output Formats for LLMs

Different formats optimize for different LLM tasks:

| Format | Best For |
|--------|----------|
| `--format json` | Structured reasoning, tool use |
| `--format jsonl` | Streaming, large documents |
| `--format tsv` | Simple extraction, spreadsheet analysis |
| `-v` (human) | Context snippets, debugging |
| `-vv` | Coreference chains, deeper analysis |

## 7. Prompt Engineering Tips

When having LLMs work with `anno` output:

1. **Always include provenance** - Helps LLM understand tool version and config
2. **Use JSON format** - Most reliably parsed by LLMs
3. **Include confidence scores** - LLMs can weight uncertain extractions
4. **Provide entity IDs** - Enables cross-referencing in multi-step workflows

### Example Prompt Template

```
You are analyzing text with entity annotations from the anno NER tool.

Input text:
{text}

Entity annotations (JSON):
{anno_json_output}

Provenance: {provenance_info}

Task: {specific_task_description}

Guidelines:
- Entity IDs (e:xxxxx) are content-addressed and stable
- Spans are byte offsets with exclusive end
- Confidence <0.9 indicates uncertainty
- Source field shows which backend produced each entity

{task_specific_instructions}
```

## 8. Security Considerations

When exposing `anno` to LLMs:

- **Sandbox file access** - Limit `--file` to specific directories
- **Rate limit** - Prevent abuse via request throttling  
- **Timeout** - Kill long-running extractions
- **Input validation** - Limit text size

```python
# Safe wrapper
MAX_TEXT_LENGTH = 100_000
TIMEOUT_SECONDS = 30

def safe_extract(text: str) -> dict:
    if len(text) > MAX_TEXT_LENGTH:
        raise ValueError(f"Text too long: {len(text)} > {MAX_TEXT_LENGTH}")
    
    result = subprocess.run(
        ["anno", "extract", "--format", "json", text],
        capture_output=True,
        text=True,
        timeout=TIMEOUT_SECONDS
    )
    return json.loads(result.stdout)
```

## LLM Agent Experience Testing

A comprehensive test script is available for LLM agents to explore `anno` capabilities:

```bash
# Run the experience test script
bash scripts/llm_agent_experience.sh
```

This script tests:
1. **Basic extraction** - Text, file, and stdin inputs
2. **Output formats** - JSON, TSV, JSONL
3. **Backend comparison** - Pattern, heuristic, stacked, GLiNER
4. **Zero-shot types** - Custom entity types (disease, drug, gene, etc.)
5. **Cross-document** - Entity resolution across multiple documents
6. **Debugging tools** - Explain, domain detection, singleton analysis
7. **Privacy** - PII detection and redaction
8. **Batch processing** - Directory-level extraction
9. **Advanced filtering** - Type filtering, confidence threshold
10. **Export/Import** - Brat format for annotation workflows

### Backend Summary

| Backend | Entities Found | Best For |
|---------|----------------|----------|
| pattern | Low | Dates, emails, URLs, money |
| heuristic | High (noisy) | Quick extraction, high recall |
| gliner | Medium (precise) | Accurate extraction, custom types |
| stacked | Medium | Balanced (default) |

### Backends Requiring Setup

- `w2ner`: Requires HuggingFace authentication (set `HF_TOKEN` env var)
- `llm`: Requires external LLM API endpoint configuration

## See Also

- [CLI Design](../cli/CLI_DESIGN.md) - Output format rationale
- [Backends](../../../BACKENDS.md) - Backend selection for different use cases
- [Evaluation](../../../EVALUATION.md) - Quality metrics

