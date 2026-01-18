# URL and Document Reference Support

**Related**: See [`docs/notes/design/pipeline/DOCUMENT_INGESTION_DESIGN.md`](../notes/design/pipeline/DOCUMENT_INGESTION_DESIGN.md) for design notes.

## Current State

### URL Resolution (Implemented)

The `anno` CLI supports fetching content from URLs via the `--url` flag:

```bash
anno extract --url https://example.com/article --model stacked
anno debug --url https://arxiv.org/abs/1706.03762
```

**Implementation**: `anno/src/ingest/url_resolver.rs`
- `CompositeResolver`: Chains multiple URL resolvers
- `HttpResolver`: Fetches HTTP/HTTPS URLs and extracts text from HTML
- Extensible architecture for adding PDF, arXiv, GitHub resolvers

**Features**:
- ✅ Direct URL fetching via `--url` flag
- ✅ HTML text extraction
- ✅ Error handling for network failures
- ✅ Requires `eval-advanced` feature

### URL Entity Extraction (Implemented)

URLs mentioned **within** text are automatically extracted as entities:

```bash
echo "See https://example.com for details" | anno extract --model stacked
# Output: URL:1 "https://example.com"
```

**Current behavior**:
- URLs are detected as entities with type `URL`
- No automatic resolution/following of URLs found in text
- URLs are just extracted as text spans

## Missing: Document Reference Following

### What's Not Implemented

**Automatic URL Following**: When a document mentions URLs (citations, references), there's no automatic resolution to:
1. Fetch the referenced document
2. Extract entities from it
3. Link entities across documents (cross-doc coref)

**Example Use Case**:
```text
According to Smith (2020), machine learning has advanced.
See: https://arxiv.org/abs/1706.03762
```

**Current behavior**: Extracts "Smith" (PER), "2020" (DATE), and "https://arxiv.org/abs/1706.03762" (URL), but doesn't fetch the arXiv paper.

**Desired behavior**: Optionally fetch the arXiv paper and extract entities, then link "Smith" to entities in the fetched paper.

## Relationship to Existing Features

### Cross-Document Coreference (`anno cross-doc`)

**Connection**: Document reference following would naturally feed into cross-doc coref:

1. Extract URLs from document → `URL` entities
2. Optionally resolve URLs → fetch referenced documents
3. Extract entities from referenced documents
4. Run `anno cross-doc` to link entities across original + referenced docs

**Current gap**: Step 2 (automatic resolution) is missing. Users must manually:
```bash
# Manual workflow
anno extract --url https://arxiv.org/abs/1706.03762 --export ref1.grounded.json
anno extract --text "According to Smith..." --export doc1.grounded.json
anno cross-doc --import ref1.grounded.json --import doc1.grounded.json
```

### Corpus Structure

The `Corpus` type in `anno-core` already supports multiple documents, so the infrastructure exists. What's missing is:
- Automatic URL detection in text
- Optional URL resolution (with caching, rate limiting)
- Integration with `extract`/`pipeline` commands

## Proposed Implementation

### Option 1: Flag-Based (Simple)

Add `--follow-urls` flag to `extract`/`pipeline`:

```bash
anno extract --text "See https://example.com" --follow-urls --model stacked
```

**Behavior**:
- Extract URLs from text
- For each URL, fetch and extract entities
- Return combined results (original + referenced docs)

### Option 2: Separate Command (Explicit)

```bash
anno extract --text "See https://example.com" --extract-urls > urls.jsonl
anno resolve-urls --input urls.jsonl --output resolved.jsonl
anno cross-doc --import resolved.jsonl
```

### Option 3: Pipeline Integration (Most Powerful)

```bash
anno pipeline --text "See https://example.com" \
  --extract \
  --follow-urls \
  --cross-doc \
  --model stacked
```

**Benefits**:
- Single command for full pipeline
- Automatic caching of resolved URLs
- Rate limiting built-in

## Technical Considerations

### Rate Limiting

URL resolution should respect:
- Per-domain rate limits
- Global rate limits
- User-configurable delays

### Caching

Resolved URLs should be cached to avoid re-fetching:
- Cache key: URL + timestamp (for freshness)
- Cache location: `~/.cache/anno/urls/` or configurable
- TTL: Configurable (default: 24 hours)

### Error Handling

- Network failures: Skip URL, log warning
- Invalid URLs: Skip, log warning
- Timeouts: Skip after N seconds, log warning
- Circular references: Detect cycles, skip

### Security

- Validate URLs (whitelist/blacklist domains)
- Sanitize fetched content
- Limit fetch size (prevent DoS)
- Respect robots.txt

## Related Features

### Citation Extraction

Beyond URLs, could extract:
- Academic citations: "Smith (2020)", "Vaswani et al. (2017)"
- DOI references: "doi:10.1234/example"
- ISBN references: "ISBN 978-0-123456-78-9"

These could be resolved to URLs, then fetched.

### Document Graph

With URL following, could build a document graph:
- Nodes: Documents
- Edges: References/citations
- Enables graph-based analysis (PageRank, community detection)

This connects to `anno tier` (hierarchical clustering).

## Implementation Priority

**High**: Basic URL following with `--follow-urls` flag
**Medium**: Caching and rate limiting
**Low**: Citation extraction, document graph construction

