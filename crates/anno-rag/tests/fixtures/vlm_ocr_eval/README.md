# VLM-OCR Evaluation Fixtures

Synthetic French legal page samples for evaluating Vision-Language Model OCR
candidates.  **All content is entirely fictional** — no real client PII,
no real case numbers, no real company or person names.

## Fixture Classes

| Class | Count | Description |
|-------|-------|-------------|
| `printed/` | 3 | Clean typeset contract clauses (Article 1–9 style text) |
| `handwritten/` | 2 | Simulated handwritten annotations on lined paper |
| `tables/` | 2 | Structured tables with party names, dates, amounts |
| `stamps/` | 2 | Body text with a rotated CACHET / REÇU overlay |

## Regenerating PNGs

PNGs are **not committed** (see `.gitignore`).  Regenerate them with:

```bash
cd crates/anno-rag/tests/fixtures/vlm_ocr_eval
python generate_fixtures.py
```

Requirements: `Pillow >= 9.0` (`pip install Pillow`).

## Reference texts

Each PNG has a `.ref.txt` sidecar (committed) containing the ground-truth
transcription used for CER computation by `scripts/eval_vlm_ocr.py`.

## VLM-OCR Eval Results

Run `scripts/eval_vlm_ocr.py` after starting the three vLLM servers to populate
this table.

| Model | printed CER | handwritten CER | tables CER | stamps CER | Overall |
|-------|------------|----------------|-----------|-----------|---------|
| LightOnOCR-2-1B | — | — | — | — | — |
| olmOCR-7B | — | — | — | — | — |
| PaddleOCR-VL-1.6 | — | — | — | — | — |
