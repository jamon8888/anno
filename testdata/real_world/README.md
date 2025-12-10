# Real-World Test Data

This directory contains real-world HTML content fetched from various URLs for regression testing.

## Files

- `eu_climate_law.html` - European Commission Climate Law page (116KB, 1411 lines)
- `arxiv_ner_paper.html` - ArXiv paper on Named Entity Recognition (45KB, 699 lines)
- `eu_council_climate.html` - EU Council climate change policy page
- `openai_usage_policies.html` - OpenAI usage policies page
- `reuters_openai_article.html` - Reuters article about OpenAI
- `un_ai_regulation.html` - UN article on AI regulation

## Usage

These files are used for:
- Regression testing on real-world HTML content
- Testing HTML text extraction
- Validating entity extraction on diverse, messy documents
- Testing crossdoc coreference on real content

## Fetching New Data

To add new test data:

```bash
curl -sL "https://example.com/article" > testdata/real_world/example.html
```

Or use MCP tools to search for relevant URLs, then fetch with curl.

## Notes

- Files are stored as raw HTML (not markdown) to test real-world HTML parsing
- Some files may be empty due to redirects or JavaScript rendering
- Files are gitignored by default - add to `.gitignore` if needed

