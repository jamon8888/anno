#!/usr/bin/env python3
# ruff: noqa: E402, T201
"""
Google Colab notebook for training a French-legal-NER LoRA adapter on
`fastino/gliner2-base-v1`.

Output: a PEFT-format adapter directory loadable by anno's
`gliner2_fastino_candle::load_adapter`:

    adapter_fr_legal/
    ├── adapter_config.json
    └── adapter_model.safetensors

After training, download the directory. From Rust:

    let mut model = GLiNER2FastinoCandle::from_pretrained(
        "fastino/gliner2-base-v1",
    )?;
    model.load_adapter("fr_legal", Path::new("./adapter_fr_legal"))?;
    let entities = model.extract_with_types(
        "Le Tribunal judiciaire de Paris a rendu son arrêt le 12 mars 2024.",
        &["person", "organization", "location", "miscellaneous"],
        0.5,
    )?;

## How to use this in Colab

1. Open https://colab.research.google.com → New notebook
2. Runtime → Change runtime type → Hardware accelerator: T4 GPU
3. Copy each cell below (delimited by `# %%` markers) into Colab cells, in order
4. Run cells top-to-bottom. Total time on T4 GPU: ~30-45 min.
5. After the last cell, the adapter appears in /content; download via the file
   browser (left sidebar) or the auto-triggered download in the last cell.

## Why French legal + how the data is built

There is no clean public French-legal NER dataset with span annotations on HF
(as of 2026-05). The closest options are:
  - `antoinejeannot/jurisprudence` — 1.1M raw French case-law docs, NO spans
  - `louisbrulenaudet/legalkit` — 53k French law Q/A, NO spans
  - `Jean-Baptiste/wikiner_fr` — 170k French Wikipedia sentences with PER/LOC/ORG/MISC

We use **wikiner_fr filtered to legal-leaning sentences** (containing keywords
like "tribunal", "cour", "arrêt", "code", "loi", "décret", "article",
"jurisprudence"). This gives ~3-5k sentences with the entity types most
relevant in legal text: parties (PER), courts/firms (ORG), jurisdictions
(LOC), laws/decrees (MISC).

This is general-domain French NER, biased toward the legal lexicon — not a
true legal-domain NER. For production legal NER you'd want span-annotated
case law; this script is the public-data approximation.

## Notes

- Uses `gliner2` package's real `GLiNER2Trainer + TrainingConfig` API
  (NOT the older `model.train(...)` guess that didn't exist).
- LoRA config: r=8, alpha=16, targets `query_proj`/`key_proj`/`value_proj`
  across all 12 mDeBERTa encoder layers. Matches what anno's Phase 4
  `lora.rs` knows how to merge.
- `save_adapter_only=True` → native PEFT-format output (adapter_config.json
  + adapter_model.safetensors with `base_model.model.<path>.lora_{A,B}.weight`
  keys).
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

# Sanity-check GPU
import torch

print(f"CUDA available: {torch.cuda.is_available()}")
if torch.cuda.is_available():
    print(f"GPU: {torch.cuda.get_device_name(0)}")
    print(f"VRAM: {torch.cuda.get_device_properties(0).total_memory / 1e9:.1f} GB")
else:
    print("WARNING: no GPU — training will be ~10× slower on CPU.")

# %% [markdown]
# # 2. Load + filter wikiner_fr to legal-leaning sentences
#
# We keep sentences containing at least one French legal keyword. Each
# sentence becomes a gliner2 `InputExample`: text + per-label entity lists.
#
# Label mapping (wikiner_fr → gliner2):
#   - PER  → person
#   - LOC  → location
#   - ORG  → organization
#   - MISC → miscellaneous

# %%
from datasets import load_dataset

print("Loading Jean-Baptiste/wikiner_fr...")
ds = load_dataset("Jean-Baptiste/wikiner_fr")
print(ds)

# wikiner_fr's label scheme: 0=O, 1=LOC, 2=PER, 3=MISC, 4=ORG
# (verify by inspecting features.ner_tags.feature.names if available)
LABEL_NAMES = ["O", "LOC", "PER", "MISC", "ORG"]
LABEL_TO_GLINER2 = {
    "PER": "person",
    "LOC": "location",
    "ORG": "organization",
    "MISC": "miscellaneous",
}

LEGAL_KEYWORDS = {
    "tribunal", "tribunaux", "cour", "cours", "arrêt", "arrêts", "arret", "arrets",
    "jugement", "jugements", "code", "codes", "loi", "lois", "décret", "decret",
    "décrets", "decrets", "article", "articles", "jurisprudence", "magistrat",
    "magistrats", "juge", "juges", "avocat", "avocats", "procureur", "procureurs",
    "audience", "audiences", "appel", "cassation", "constitutionnel", "civil",
    "pénal", "penal", "administratif", "conseil d'état", "conseil constitutionnel",
    "ordonnance", "réquisitoire", "requisitoire", "plaidoirie", "verdict",
    "condamnation", "infraction", "délit", "delit", "crime", "amende",
}


def is_legal_sentence(tokens: list[str]) -> bool:
    """Return True if the tokenized sentence contains any French legal keyword."""
    lower = {t.lower() for t in tokens}
    return any(kw in lower for kw in LEGAL_KEYWORDS)


def tokens_to_input_example(tokens: list[str], tags: list[int]):
    """Convert a (tokens, BIO-tags) wikiner row into a gliner2 InputExample."""
    text = " ".join(tokens)
    # Reconstruct character offsets
    offsets = []
    cursor = 0
    for t in tokens:
        offsets.append((cursor, cursor + len(t)))
        cursor += len(t) + 1  # +1 for the space joiner

    # Walk BIO tags, group consecutive same-label tokens into spans
    entities: dict[str, list[str]] = {}
    i = 0
    while i < len(tags):
        tag = tags[i]
        if tag == 0:  # O
            i += 1
            continue
        label_short = LABEL_NAMES[tag]
        # extend run
        j = i + 1
        while j < len(tags) and tags[j] == tag:
            j += 1
        span_text = " ".join(tokens[i:j])
        gliner_label = LABEL_TO_GLINER2.get(label_short)
        if gliner_label is not None:
            entities.setdefault(gliner_label, []).append(span_text)
        i = j

    return {"text": text, "entities": entities}


train_split = ds["train"]
print(f"\nTotal train sentences: {len(train_split):,}")

legal_examples = []
for row in train_split:
    tokens = row["tokens"]
    tags = row["ner_tags"]
    if not is_legal_sentence(tokens):
        continue
    ex = tokens_to_input_example(tokens, tags)
    if ex["entities"]:  # skip sentences with no entities
        legal_examples.append(ex)

print(f"Legal-leaning sentences with entities: {len(legal_examples):,}")
print(f"\nSample example:")
print(legal_examples[0] if legal_examples else "(none)")

# Cap dataset size for T4 budget (~30 min training)
MAX_TRAIN = 5000
if len(legal_examples) > MAX_TRAIN:
    import random

    random.seed(42)
    random.shuffle(legal_examples)
    legal_examples = legal_examples[:MAX_TRAIN]
    print(f"\nCapped to {MAX_TRAIN} examples for Colab time budget.")

# Train/eval split
random_state = 42
import random

random.seed(random_state)
random.shuffle(legal_examples)
split_at = int(0.9 * len(legal_examples))
train_examples = legal_examples[:split_at]
eval_examples = legal_examples[split_at:]
print(f"\nTrain: {len(train_examples)}  Eval: {len(eval_examples)}")

# %% [markdown]
# # 3. Convert to gliner2 InputExample objects
#
# The trainer accepts either dicts of the right shape or `InputExample`
# instances. Using the typed class makes errors fail loudly.

# %%
from gliner2.training.data import InputExample


def to_input_example(d: dict) -> InputExample:
    return InputExample(
        text=d["text"],
        entities=d["entities"],
    )


train_inputs = [to_input_example(d) for d in train_examples]
eval_inputs = [to_input_example(d) for d in eval_examples]

print(f"Built {len(train_inputs)} train InputExample, {len(eval_inputs)} eval.")
print(f"\nFirst InputExample:\n  text: {train_inputs[0].text[:120]}...")
print(f"  entities: {train_inputs[0].entities}")

# %% [markdown]
# # 4. Train the adapter
#
# LoRA config:
# - r=8, alpha=16 (effective scale = alpha/r = 2.0)
# - target_modules = `query_proj`, `key_proj`, `value_proj` (matches what
#   anno's `lora.rs` knows how to merge into the mDeBERTa-v3 encoder)
# - dropout=0.05
#
# Training:
# - 5 epochs, batch size 8, fp16 on T4
# - Encoder LR 1e-5 (frozen-ish base), task-head LR 5e-4 (faster adaptation)
# - Cosine schedule, 10% warmup
# - Early stopping with patience 2

# %%
from gliner2 import GLiNER2
from gliner2.training.trainer import GLiNER2Trainer, TrainingConfig

print("Loading base model fastino/gliner2-base-v1...")
model = GLiNER2.from_pretrained("fastino/gliner2-base-v1")

config = TrainingConfig(
    output_dir="./adapter_fr_legal",
    experiment_name="fr_legal_ner",
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
    # CRITICAL: explicit target modules so the resulting adapter has
    # only Q/K/V projection deltas — what anno's Phase 4 loader merges.
    # If you omit this, gliner2's default targets symbolic categories
    # (encoder/span_rep/classifier/...) and produces extra LoRA keys
    # that anno currently ignores.
    lora_target_modules=["query_proj", "key_proj", "value_proj"],
)

print("\nStarting training...")
trainer = GLiNER2Trainer(model, config)
trainer.train(train_data=train_inputs, eval_data=eval_inputs)
print("\n✅ Training complete. Adapter saved under ./adapter_fr_legal/")

# %% [markdown]
# # 5. Verify PEFT format
#
# anno's `lora.rs` expects:
#   - `adapter_config.json` with `r`, `lora_alpha`, `target_modules`,
#     `base_model_name_or_path`
#   - `adapter_model.safetensors`
#   - keys matching `base_model.model.<path>.lora_{A,B}.weight`

# %%
import json
import os

from safetensors import safe_open


def find_adapter_dir(root: str) -> str:
    """Locate the actual adapter dir (trainer may write into subdirs like /final)."""
    candidates = []
    for dirpath, _dirnames, filenames in os.walk(root):
        if "adapter_config.json" in filenames:
            candidates.append(dirpath)
    if not candidates:
        raise FileNotFoundError(f"No adapter_config.json under {root}")
    # Prefer 'final' or 'best' subdirs if present
    for c in candidates:
        if c.endswith(("final", "best")):
            return c
    return candidates[0]


def verify_adapter(adapter_dir: str) -> dict:
    summary = {"path": adapter_dir, "files": [], "config": {}, "tensor_keys_sample": []}
    summary["files"] = sorted(os.listdir(adapter_dir))

    config_path = os.path.join(adapter_dir, "adapter_config.json")
    if os.path.exists(config_path):
        with open(config_path) as fh:
            summary["config"] = json.load(fh)

    weights_path = None
    for candidate in ["adapter_model.safetensors", "adapter_weights.safetensors"]:
        p = os.path.join(adapter_dir, candidate)
        if os.path.exists(p):
            weights_path = p
            summary["weights_file"] = candidate
            break

    if weights_path:
        with safe_open(weights_path, framework="pt") as f:
            keys = list(f.keys())
            summary["num_tensors"] = len(keys)
            summary["tensor_keys_sample"] = keys[:3]
            summary["valid_peft_keys"] = sum(
                1
                for k in keys
                if k.startswith("base_model.model.")
                and (k.endswith(".lora_A.weight") or k.endswith(".lora_B.weight"))
            )
    return summary


actual_adapter_dir = find_adapter_dir("./adapter_fr_legal")
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
print(f"  Sample keys: {s['tensor_keys_sample']}")

# Hard assert — fail the cell if format is wrong, before downloading 30+ min of work
assert s.get("valid_peft_keys", 0) > 0, (
    "❌ Adapter has zero PEFT-format keys. "
    "Did `lora_target_modules` match real module names? "
    f"Sample keys found: {s['tensor_keys_sample']}"
)
assert s["config"].get("peft_type") == "LORA", (
    f"❌ adapter_config.json missing peft_type=LORA: {s['config']}"
)
print("\n✅ Adapter passes PEFT-format checks.")

# %% [markdown]
# # 6. Smoke test in Python
#
# Verify the trained adapter changes inference vs. the base model on a
# legal sentence. If both produce the exact same output, training had no
# effect (probably an issue upstream).

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
    "Code du commerce. Maître Jean Dupont a plaidé pour la défense."
)
test_types = ["person", "organization", "location", "miscellaneous"]

print(f"\nText: {test_text}")
print("\n--- Base model (no adapter) ---")
base_out = base.extract_entities(test_text, test_types, threshold=0.5)
print(f"  {base_out}")

print(f"\n--- After load_adapter('{actual_adapter_dir}') ---")
base.load_adapter(actual_adapter_dir)
adapt_out = base.extract_entities(test_text, test_types, threshold=0.5)
print(f"  {adapt_out}")

# Sanity: outputs should differ at least somewhere
if base_out == adapt_out:
    print("\n⚠️  Base and adapter produced identical output. Training may have")
    print("    had no effect, OR the test sentence is too easy for both.")
else:
    print("\n✅ Adapter changed inference behavior.")

# %% [markdown]
# # 7. Package + download
#
# Zip the adapter and trigger a browser download in Colab.

# %%
import shutil

# Copy adapter to a clean top-level dir for zipping (strips any /final or /best suffix)
EXPORT_DIR = "./adapter_fr_legal_export"
if os.path.exists(EXPORT_DIR):
    shutil.rmtree(EXPORT_DIR)
shutil.copytree(actual_adapter_dir, EXPORT_DIR)

zip_path = shutil.make_archive("adapter_fr_legal", "zip", ".", EXPORT_DIR)
print(f"Created {zip_path} ({os.path.getsize(zip_path):,} bytes)")

try:
    from google.colab import files

    files.download("adapter_fr_legal.zip")
except ImportError:
    print("Not in Colab — find adapter_fr_legal.zip in your working directory.")

# %% [markdown]
# # 8. After downloading: testing in Rust (anno)
#
# 1. Unzip next to your Rust project:
#
#    ```bash
#    unzip adapter_fr_legal.zip
#    mv adapter_fr_legal_export adapter_fr_legal
#    ls adapter_fr_legal/
#    # adapter_config.json   adapter_model.safetensors
#    ```
#
# 2. Run anno's `real_adapter_changes_inference` test against it:
#
#    ```bash
#    GLINER2_TEST_ADAPTER_DIR=./adapter_fr_legal \
#      cargo test -p anno --features gliner2-fastino-candle \
#        --test gliner2_fastino_candle_lora -- --ignored real_adapter \
#        --nocapture --test-threads=1
#    ```
#
# 3. Or write a custom example reusing the demo skeleton:
#
#    ```rust
#    let mut model = GLiNER2FastinoCandle::from_pretrained(
#        "fastino/gliner2-base-v1",
#    )?;
#
#    let text = "Le Tribunal judiciaire de Paris a rendu son arrêt le 12 mars 2024.";
#    let types = &["person", "organization", "location", "miscellaneous"];
#
#    let base = ZeroShotNER::extract_with_types(&model, text, types, 0.5)?;
#    println!("base: {base:?}");
#
#    model.load_adapter("fr_legal", Path::new("./adapter_fr_legal"))?;
#    let after = ZeroShotNER::extract_with_types(&model, text, types, 0.5)?;
#    println!("after: {after:?}");
#    ```
#
# 4. Optional: train a SECOND complementary adapter (e.g. cap `LEGAL_KEYWORDS`
#    to only `code`/`loi`/`décret`/`article` for a "law-text" focus instead
#    of `tribunal`/`cour`/`juge` for a "court-procedure" focus) and exercise
#    the runtime swap in `gliner2_candle_lora_demo.rs`.
