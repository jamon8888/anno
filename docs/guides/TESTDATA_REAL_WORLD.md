# Real-World Test Data

This directory contains real-world content fetched from various URLs for regression testing and validation.

## Fetching Test Data

Use the batch fetch script to download diverse content types:

```bash
./scripts/fetch_testdata.sh
```

This script fetches:
- HTML pages (news articles, policy pages, academic abstracts)
- PDF documents (research papers)
- JSON datasets (if available)
- Plain text files

## Current Test Data

### HTML Documents
- `eu_climate_law.html` - European Commission Climate Law page (116KB, 1411 lines)
- `arxiv_ner_paper.html` - ArXiv paper on Named Entity Recognition (45KB, 699 lines)
- `un_ai_regulation.html` - UN article on AI regulation (89KB, 1361 lines)
- `https:__arxiv_org_abs_2101_00884.html` - ArXiv abstract on coreference resolution (45KB, 707 lines)
- `https:__arxiv_org_abs_2309_14084.html` - ArXiv abstract on NER (45KB, 699 lines)

### PDF Documents (fetched as HTML)
- `https:__arxiv_org_pdf_2309_14084.html` - ArXiv PDF on NER (210KB, 2622 lines)
- `https:__arxiv_org_pdf_2101_00884.html` - ArXiv PDF on coreference (190KB, 4018 lines)

## Usage

These files are used for:
- Regression testing on real-world HTML/PDF content
- Testing HTML text extraction
- Validating entity extraction on diverse, messy documents
- Testing crossdoc coreference on real content
- Validating clustering quality on heterogeneous documents

## Testing Extraction

```bash
# Test extraction on a single file
cargo run --features eval-advanced -- extract --file testdata/real_world/eu_climate_law.html --model stacked

# Test crossdoc on directory
cargo run --features eval-advanced -- cross-doc testdata/real_world --extensions html --threshold 0.6
```

## Notes

- Files are stored as raw HTML/PDF (not markdown) to test real-world parsing
- Some files may be empty due to redirects or JavaScript rendering
- Files are gitignored by default - add to `.gitignore` if needed
- PDFs are fetched as HTML (ArXiv serves PDFs as HTML when accessed via curl)

