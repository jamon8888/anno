#!/usr/bin/env python3
# ruff: noqa: E402, T201
"""
Google Colab notebook — V2: French legal NER LoRA adapter trained on
**real case-law text** with span annotations bootstrapped via weak labeling.

Output: a PEFT-format adapter loadable by anno's
`gliner2_fastino_candle::load_adapter`:

    adapter_fr_legal_v2/
    ├── adapter_config.json
    └── adapter_model.safetensors

## What's different from v1

| Aspect | v1 (`colab_train_gliner2_fr_legal_lora.py`) | v2 (this file) |
|---|---|---|
| Source text | `Jean-Baptiste/wikiner_fr` (Wikipedia FR filtered by legal keywords) | `antoinejeannot/jurisprudence` (real French court decisions) |
| Spans | Gold spans from wikiner | Bootstrapped weak labels from `Jean-Baptiste/camembert-ner` |
| Domain fidelity | Wikipedia-biased, contains legal vocab | Native legal text — verdicts, jugements, articles, ordonnances |
| Label noise | Low (gold) | Higher (weak labels, conf-filtered ≥0.85) |
| Total runtime on T4 | ~30-45 min | ~50-65 min (extra ~15 min for weak labeling) |

The weak-labeling step is the price of admission for real legal text:
**no public French legal NER dataset has gold span annotations**, so we
synthesize them. Camembert-NER is reasonably accurate on French text; we
filter by confidence ≥0.85 to limit noise. The resulting adapter learns
the domain *vocabulary* and *entity surface forms* of real case law,
even if the labels themselves carry some noise.

## How to use this in Colab

1. Open https://colab.research.google.com → New notebook
2. Runtime → Change runtime type → Hardware accelerator: T4 GPU
3. Copy each cell below (delimited by `# %%` markers) into Colab cells, in order
4. Run cells top-to-bottom. Total time on T4 GPU: ~50-65 min.
5. After the last cell, the adapter appears in /content; download via the
   file browser or the auto-triggered download in the last cell.

## Notes

- Weak labels are NOT gold — expect ~5-10% noise vs. a hand-annotated set.
  This is fine for adapting an already-pretrained model to legal text;
  it's NOT fine for absolute eval numbers. If you need gold labels for
  evaluation, hand-annotate a small held-out set.
- The trainer uses the same real `GLiNER2Trainer + TrainingConfig` API
  verified for v1 — explicit `lora_target_modules=["query_proj",
  "key_proj", "value_proj"]` so anno's Phase 4 loader can merge the
  resulting adapter without surprises.
"""

# %% [markdown]
# # 1. Setup
#
# Install gliner2, peft, transformers, datasets, accelerate. ~5 min.

# %%
import subprocess
import sys


def pip_install(*args):
    subprocess.check_call([sys.executable, "-m", "pip", "install", "-q", *args])


pip_install(
    "gliner2",
    "peft>=0.10.0",
    "transformers>=4.40.0",
    "datasets>=2.18.0",
    "accelerate>=0.27.0",
    "safetensors>=0.4.0",
    "sentencepiece",  # camembert tokenizer needs this
    "huggingface_hub>=0.20.0",
)

# ── HuggingFace authentication ───────────────────────────────────────
# Required for stable model/dataset downloads (avoids anonymous rate
# limits; mandatory if you ever pull a gated model).
#
# In Colab: left sidebar → 🔑 Secrets → "Add new secret"
#   Name: HF_TOKEN
#   Value: hf_xxx... (https://huggingface.co/settings/tokens, "read" scope)
#   Notebook access: ON
#
# Locally: export HF_TOKEN=hf_xxx before running.
HF_TOKEN = None
try:
    from google.colab import userdata

    try:
        HF_TOKEN = userdata.get("HF_TOKEN")
    except Exception as e:
        print(f"⚠️  Could not read HF_TOKEN secret: {e}")
        print("    Sidebar → 🔑 Secrets → New secret → Name: HF_TOKEN, access: ON")
except ImportError:
    import os

    HF_TOKEN = os.environ.get("HF_TOKEN")

if HF_TOKEN:
    from huggingface_hub import login

    login(token=HF_TOKEN, add_to_git_credential=False)
    print("✅ Logged in to HuggingFace")
else:
    print("ℹ️  No HF token detected. Public downloads still work but may hit rate limits.")

import torch

print(f"CUDA available: {torch.cuda.is_available()}")
if torch.cuda.is_available():
    print(f"GPU: {torch.cuda.get_device_name(0)}")
    print(f"VRAM: {torch.cuda.get_device_properties(0).total_memory / 1e9:.1f} GB")
else:
    print("WARNING: no GPU — V2 weak-labeling step will be VERY slow on CPU.")

# %% [markdown]
# # 2. Stream + sample French jurisprudence
#
# `antoinejeannot/jurisprudence` is ~9.3 GB, 1.13M docs split by juridiction:
#   - `tribunal_judiciaire` (1ère instance)
#   - `cour_d_appel` (cours d'appel)
#   - `cour_de_cassation` (Cour de cassation)
#
# We stream all three (no full download) round-robin so the training set
# spans court levels. ~1000 docs per split → ~3000 total → enough raw text
# for ~5-10k sentence-level training examples after weak labeling.
#
# The dataset's full-text field is auto-detected from common candidates
# (`texte_integral`, `texte`, `text`, `content`, `contenu`).

# %%
from datasets import load_dataset

JURISPRUDENCE_SPLITS = ["tribunal_judiciaire", "cour_d_appel", "cour_de_cassation"]
print(f"Streaming antoinejeannot/jurisprudence splits: {JURISPRUDENCE_SPLITS}")

# Probe the first record of one split to discover the text field
probe = load_dataset(
    "antoinejeannot/jurisprudence", split=JURISPRUDENCE_SPLITS[0], streaming=True
)
first = next(iter(probe))
print(f"\nFirst record fields: {sorted(first.keys())}")
TEXT_FIELD_CANDIDATES = ["texte_integral", "texte", "text", "content", "contenu"]
TEXT_FIELD = next((f for f in TEXT_FIELD_CANDIDATES if f in first), None)
if TEXT_FIELD is None:
    raise RuntimeError(
        f"Could not find a text field in jurisprudence record. "
        f"Available: {sorted(first.keys())}"
    )
print(f"Using text field: '{TEXT_FIELD}'")
print(f"Sample (first 200 chars): {str(first[TEXT_FIELD])[:200]}...")

NUM_DOCS = 3000
PER_SPLIT = NUM_DOCS // len(JURISPRUDENCE_SPLITS)
MIN_LEN = 500       # skip very short docs (probably metadata-only)
MAX_LEN = 50_000    # cap doc length to keep memory bounded

raw_docs: list[str] = []
for split in JURISPRUDENCE_SPLITS:
    print(f"\n  Streaming split '{split}' (target: {PER_SPLIT} docs)...")
    stream = load_dataset(
        "antoinejeannot/jurisprudence", split=split, streaming=True
    )
    kept_in_split = 0
    for row in stream:
        text = row.get(TEXT_FIELD)
        if not text or not isinstance(text, str):
            continue
        if not (MIN_LEN <= len(text) <= MAX_LEN):
            continue
        raw_docs.append(text)
        kept_in_split += 1
        if kept_in_split >= PER_SPLIT:
            break
    print(f"    kept {kept_in_split} docs from '{split}'")

print(f"\nTotal: {len(raw_docs)} jurisprudence documents")
print(f"Mean length: {sum(len(d) for d in raw_docs) / len(raw_docs):.0f} chars")

# %% [markdown]
# # 3. Sentence segmentation
#
# Camembert-NER (and gliner2 itself) have a 512-token limit, so we feed
# them sentence-by-sentence. We use a simple regex splitter — French
# legal text has a lot of "Art." and "M." which can fool naive splitters,
# but for weak labeling perfect sentence boundaries are not critical.

# %%
import re

# Match a sentence terminator followed by whitespace + uppercase or end-of-string.
# Negative lookbehind for common French abbreviations to avoid false splits.
ABBREV = r"(?<!\bM)(?<!\bMme)(?<!\bMlle)(?<!\bDr)(?<!\bMe)(?<!\bSt)(?<!\bArt)(?<!\bart)(?<!\bp)"
SENTENCE_END = re.compile(rf"{ABBREV}([\.\!\?])\s+(?=[A-ZÉÈÀÂÔÎÛÇ])")


def split_sentences(text: str) -> list[str]:
    # Normalize whitespace
    text = re.sub(r"\s+", " ", text).strip()
    # Split on sentence boundaries
    parts = SENTENCE_END.split(text)
    # SENTENCE_END.split returns [chunk, punct, chunk, punct, ...]; reglue
    sentences = []
    cur = ""
    for p in parts:
        if p in (".", "!", "?"):
            cur += p
            sentences.append(cur.strip())
            cur = ""
        else:
            cur += p
    if cur.strip():
        sentences.append(cur.strip())
    return sentences


# Quick test
sample_sents = split_sentences(raw_docs[0])
print(f"Doc 0 → {len(sample_sents)} sentences. First 3:")
for s in sample_sents[:3]:
    print(f"  • {s[:150]}...")

# %% [markdown]
# # 4. Weak-label with camembert-ner
#
# `Jean-Baptiste/camembert-ner` outputs PER, LOC, ORG, MISC entity groups
# with character offsets and confidence scores. We:
#   - Filter sentences by length (20-400 chars) — gliner2 sweet spot
#   - Run NER on each sentence in batches
#   - Keep only entities with confidence ≥ 0.85
#   - Drop sentences with zero kept entities
#
# Runtime on T4: ~12-15 min for 3000 docs (~30-50k sentences after filter).

# %%
from transformers import pipeline

print("Loading Jean-Baptiste/camembert-ner...")
ner = pipeline(
    "ner",
    model="Jean-Baptiste/camembert-ner",
    aggregation_strategy="simple",
    device=0 if torch.cuda.is_available() else -1,
)

LABEL_MAP = {
    "PER": "person",
    "LOC": "location",
    "ORG": "organization",
    "MISC": "miscellaneous",
}
CONF_THRESHOLD = 0.85
MIN_SENT_LEN = 20
MAX_SENT_LEN = 400
BATCH_SIZE = 32
MAX_EXAMPLES = 6000  # cap to keep training under 60 min

# Collect all candidate sentences across all docs first
all_sentences: list[str] = []
for doc in raw_docs:
    for sent in split_sentences(doc):
        if MIN_SENT_LEN <= len(sent) <= MAX_SENT_LEN:
            all_sentences.append(sent)
print(f"Total candidate sentences: {len(all_sentences):,}")

# Shuffle so we get a varied sample before capping
import random

random.seed(42)
random.shuffle(all_sentences)

# Run NER in batches for throughput
print("\nRunning camembert-ner over sentences (batch_size=32)...")
labeled_examples: list[dict] = []
batch: list[str] = []
processed = 0

for sent in all_sentences:
    batch.append(sent)
    if len(batch) >= BATCH_SIZE:
        results = ner(batch)
        for s, ents in zip(batch, results):
            entities: dict[str, list[str]] = {}
            for e in ents:
                if e["score"] < CONF_THRESHOLD:
                    continue
                label = LABEL_MAP.get(e["entity_group"])
                if label is None:
                    continue
                # The pipeline sometimes leaves leading whitespace
                surface = e["word"].strip()
                if len(surface) < 2:
                    continue
                entities.setdefault(label, []).append(surface)
            if entities:
                labeled_examples.append({"text": s, "entities": entities})
        processed += len(batch)
        batch = []
        if processed % 1000 == 0:
            print(f"  processed {processed:,} sentences, kept {len(labeled_examples):,} with entities")
        if len(labeled_examples) >= MAX_EXAMPLES:
            break

# Flush trailing batch
if batch and len(labeled_examples) < MAX_EXAMPLES:
    results = ner(batch)
    for s, ents in zip(batch, results):
        entities = {}
        for e in ents:
            if e["score"] < CONF_THRESHOLD:
                continue
            label = LABEL_MAP.get(e["entity_group"])
            if label is None:
                continue
            surface = e["word"].strip()
            if len(surface) < 2:
                continue
            entities.setdefault(label, []).append(surface)
        if entities:
            labeled_examples.append({"text": s, "entities": entities})

labeled_examples = labeled_examples[:MAX_EXAMPLES]
print(f"\n✅ Built {len(labeled_examples):,} weakly-labeled examples.")
print(f"\nSample:")
for ex in labeled_examples[:3]:
    print(f"  text: {ex['text'][:150]}...")
    print(f"  entities: {ex['entities']}")
    print()

# Free camembert-ner before training to reclaim VRAM
del ner
if torch.cuda.is_available():
    torch.cuda.empty_cache()

# %% [markdown]
# # 5. Convert to InputExample + train/eval split

# %%
from gliner2.training.data import InputExample


def to_input_example(d: dict) -> InputExample:
    return InputExample(text=d["text"], entities=d["entities"])


random.seed(42)
random.shuffle(labeled_examples)
split_at = int(0.9 * len(labeled_examples))
train_inputs = [to_input_example(d) for d in labeled_examples[:split_at]]
eval_inputs = [to_input_example(d) for d in labeled_examples[split_at:]]

print(f"Train: {len(train_inputs)}  Eval: {len(eval_inputs)}")

# Label distribution sanity check
from collections import Counter

label_counter: Counter = Counter()
for ex in labeled_examples:
    for label, spans in ex["entities"].items():
        label_counter[label] += len(spans)
print(f"\nLabel distribution across all spans:")
for label, count in label_counter.most_common():
    print(f"  {label}: {count:,}")

# %% [markdown]
# # 6. Train the adapter
#
# Same hyperparameters as v1. The trainer uses the real
# `GLiNER2Trainer + TrainingConfig` API; `save_adapter_only=True`
# produces native PEFT-format output that anno's Phase 4 loader consumes.

# %%
from gliner2 import GLiNER2
from gliner2.training.trainer import GLiNER2Trainer, TrainingConfig

print("Loading base model fastino/gliner2-base-v1...")
model = GLiNER2.from_pretrained("fastino/gliner2-base-v1")

config = TrainingConfig(
    output_dir="./adapter_fr_legal_v2",
    experiment_name="fr_legal_ner_v2_spans",
    num_epochs=5,
    batch_size=8,
    encoder_lr=1e-5,
    task_lr=5e-4,
    warmup_ratio=0.1,
    scheduler_type="cosine",
    fp16=torch.cuda.is_available(),
    eval_strategy="epoch",
    save_best=True,
    early_stopping=True,
    early_stopping_patience=2,
    # ── LoRA ──────────────────────────────────────────────────────
    use_lora=True,
    lora_r=8,
    lora_alpha=16.0,
    lora_dropout=0.05,
    lora_use_dora=False,
    save_adapter_only=True,
    # Explicit Q/K/V targets so anno's lora.rs can merge the result.
    lora_target_modules=["query_proj", "key_proj", "value_proj"],
)

print("\nStarting training...")
trainer = GLiNER2Trainer(model, config)
trainer.train(train_data=train_inputs, eval_data=eval_inputs)
print("\n✅ Training complete. Adapter saved under ./adapter_fr_legal_v2/")

# %% [markdown]
# # 7. Verify PEFT format (hard-asserts before download)

# %%
import json
import os

from safetensors import safe_open


def find_adapter_dir(root: str) -> str:
    candidates = []
    for dirpath, _dirnames, filenames in os.walk(root):
        if "adapter_config.json" in filenames:
            candidates.append(dirpath)
    if not candidates:
        raise FileNotFoundError(f"No adapter_config.json under {root}")
    for c in candidates:
        if c.endswith(("final", "best")):
            return c
    return candidates[0]


def verify_adapter(adapter_dir: str) -> dict:
    s = {"path": adapter_dir, "files": sorted(os.listdir(adapter_dir)), "config": {}}
    cfg_p = os.path.join(adapter_dir, "adapter_config.json")
    if os.path.exists(cfg_p):
        with open(cfg_p) as fh:
            s["config"] = json.load(fh)
    weights = None
    for name in ["adapter_model.safetensors", "adapter_weights.safetensors"]:
        p = os.path.join(adapter_dir, name)
        if os.path.exists(p):
            weights = p
            s["weights_file"] = name
            break
    if weights:
        with safe_open(weights, framework="pt") as f:
            keys = list(f.keys())
            s["num_tensors"] = len(keys)
            s["tensor_keys_sample"] = keys[:3]
            s["valid_peft_keys"] = sum(
                1
                for k in keys
                if k.startswith("base_model.model.")
                and (k.endswith(".lora_A.weight") or k.endswith(".lora_B.weight"))
            )
    return s


actual_adapter_dir = find_adapter_dir("./adapter_fr_legal_v2")
print(f"Located adapter at: {actual_adapter_dir}\n")

s = verify_adapter(actual_adapter_dir)
print(f"=== {s['path']} ===")
print(f"  Files: {s['files']}")
if s["config"]:
    cfg = s["config"]
    print(f"  peft_type: {cfg.get('peft_type')}")
    print(f"  r: {cfg.get('r')}, alpha: {cfg.get('lora_alpha')}")
    print(f"  target_modules: {cfg.get('target_modules')}")
    print(f"  base_model_name_or_path: {cfg.get('base_model_name_or_path')}")
print(f"  Total tensors: {s.get('num_tensors', 'n/a')}")
print(f"  Valid PEFT-pattern keys: {s.get('valid_peft_keys', 0)} / {s.get('num_tensors', 0)}")
print(f"  Sample keys: {s.get('tensor_keys_sample')}")

assert s.get("valid_peft_keys", 0) > 0, (
    f"❌ Adapter has zero PEFT-format keys. "
    f"Sample keys: {s.get('tensor_keys_sample')}"
)
assert s["config"].get("peft_type") == "LORA", (
    f"❌ adapter_config.json missing peft_type=LORA: {s['config']}"
)
print("\n✅ Adapter passes PEFT-format checks.")

# %% [markdown]
# # 8. Smoke test on a real legal sentence

# %%
print("Reloading base model + applying adapter for smoke test...")
del model
if torch.cuda.is_available():
    torch.cuda.empty_cache()

base = GLiNER2.from_pretrained("fastino/gliner2-base-v1")
if torch.cuda.is_available():
    base = base.to("cuda")

test_text = (
    "Le Tribunal judiciaire de Paris a rendu son arrêt le 12 mars 2024, "
    "condamnant la société Acme SA pour violation de l'article L.123-4 du "
    "Code du commerce. Maître Jean Dupont a plaidé pour la défense devant "
    "la Cour d'appel de Versailles."
)
test_types = ["person", "organization", "location", "miscellaneous"]

print(f"\nText: {test_text}")
print("\n--- Base model (no adapter) ---")
print(f"  {base.extract_entities(test_text, test_types, threshold=0.5)}")

print(f"\n--- After load_adapter('{actual_adapter_dir}') ---")
base.load_adapter(actual_adapter_dir)
adapt_out = base.extract_entities(test_text, test_types, threshold=0.5)
print(f"  {adapt_out}")

# %% [markdown]
# # 9. Package + download

# %%
import shutil

EXPORT_DIR = "./adapter_fr_legal_v2_export"
if os.path.exists(EXPORT_DIR):
    shutil.rmtree(EXPORT_DIR)
shutil.copytree(actual_adapter_dir, EXPORT_DIR)

zip_path = shutil.make_archive("adapter_fr_legal_v2", "zip", ".", EXPORT_DIR)
print(f"Created {zip_path} ({os.path.getsize(zip_path):,} bytes)")

try:
    from google.colab import files

    files.download("adapter_fr_legal_v2.zip")
except ImportError:
    print("Not in Colab — find adapter_fr_legal_v2.zip in your working directory.")

# %% [markdown]
# # 10. After downloading: testing in Rust (anno)
#
# 1. Unzip and rename:
#
#    ```bash
#    unzip adapter_fr_legal_v2.zip
#    mv adapter_fr_legal_v2_export adapter_fr_legal_v2
#    ls adapter_fr_legal_v2/
#    # adapter_config.json   adapter_model.safetensors
#    ```
#
# 2. Quick sanity test using anno's existing test harness:
#
#    ```bash
#    GLINER2_TEST_ADAPTER_DIR=./adapter_fr_legal_v2 \
#      cargo test -p anno --features gliner2-fastino-candle \
#        --test gliner2_fastino_candle_lora -- --ignored real_adapter \
#        --nocapture --test-threads=1
#    ```
#
# 3. Side-by-side with v1 — exercise runtime adapter swap with two real
#    adapters (v1 = wikiner-filtered, v2 = jurisprudence-bootstrapped).
#    Reuse `examples/gliner2_candle_lora_demo.rs`:
#
#    ```bash
#    GLINER2_ADAPTER_A=./adapter_fr_legal     \
#    GLINER2_ADAPTER_B=./adapter_fr_legal_v2  \
#    cargo run --release -p anno --features gliner2-fastino-candle \
#        --example gliner2_candle_lora_demo
#    ```
#
#    The demo prints per-step entity output and per-(step, step) confidence
#    drift — you should see non-zero deltas between A↔B since the two
#    adapters were trained on different distributions of legal text.
