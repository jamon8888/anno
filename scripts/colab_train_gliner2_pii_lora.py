#!/usr/bin/env python3
# ruff: noqa: E402, T201
"""
Google Colab notebook for training PII-domain LoRA adapters on `fastino/gliner2-base-v1`.

The output is two PEFT-format adapter directories that anno's
gliner2_fastino_candle backend can load via `load_adapter`:

    adapter_personal/
    ├── adapter_config.json
    └── adapter_model.safetensors

    adapter_financial/
    ├── adapter_config.json
    └── adapter_model.safetensors

After training, download both directories. From Rust:

    let mut model = GLiNER2FastinoCandle::from_pretrained(
        "fastino/gliner2-base-v1",
    )?;
    model.load_adapter("personal", Path::new("./adapter_personal"))?;
    let names_emails = model.extract_with_types(text, &["name", "email"], 0.5)?;

    model.load_adapter("financial", Path::new("./adapter_financial"))?;
    let cards_ssns = model.extract_with_types(text, &["credit_card", "ssn"], 0.5)?;

## How to use this in Colab

1. Open https://colab.research.google.com → New notebook
2. Runtime → Change runtime type → Hardware accelerator: T4 GPU
3. Copy each cell below (delimited by `# %%` markers) into Colab cells, in order
4. Run cells top-to-bottom. Total time on T4 GPU: ~30-45 minutes for both adapters.
5. After the last cell, the adapters appear in /content; download via the file
   browser (left sidebar) or zip + download programmatically.

## Notes

- Uses `gliner2==1.3.1` (the same version anno's Phase 4 was tested against).
- Training data is synthetic, generated with `Faker`. ~500 examples per adapter.
- LoRA config: r=8, alpha=16, targets Q/K/V projections in encoder layers
  0-11. This matches what anno's `lora.rs` expects (PEFT-format,
  `base_model.model.<path>.lora_{A,B}.weight` keys).
- Saving with `save_adapter_only=True` produces PEFT-format output.
"""

# %% [markdown]
# # Setup
#
# Install gliner2, peft, transformers, faker. ~5 min.

# %%
import subprocess
import sys

def pip_install(*args):
    subprocess.check_call([sys.executable, "-m", "pip", "install", "-q", *args])

pip_install("gliner2==1.3.1", "peft>=0.10.0", "transformers>=4.40.0", "faker>=20.0.0", "safetensors>=0.4.0")

# Sanity-check GPU is visible
import torch
print(f"CUDA available: {torch.cuda.is_available()}")
if torch.cuda.is_available():
    print(f"GPU: {torch.cuda.get_device_name(0)}")
else:
    print("WARNING: no GPU — training will be ~10× slower on CPU.")

# %% [markdown]
# # Synthetic PII data generators
#
# Two domain datasets:
# - **personal**: names, emails, phone numbers, addresses
# - **financial**: credit card numbers, SSNs, IBANs, account numbers
#
# We use Faker to generate realistic surface forms, then wrap them in
# template sentences with annotation spans. Each example becomes a gliner2
# `entities`-task training sample.

# %%
import random
from faker import Faker

fake = Faker()
Faker.seed(42)
random.seed(42)


def make_example(text: str, entities: list[tuple[str, str]]) -> dict:
    """Build a gliner2 training example from text + (entity_text, label) pairs."""
    spans = []
    for entity_text, label in entities:
        idx = text.find(entity_text)
        if idx == -1:
            continue
        spans.append({
            "start": idx,
            "end": idx + len(entity_text),
            "label": label,
        })
    return {"text": text, "entities": spans}


def gen_personal(n: int = 500) -> list[dict]:
    """Names, emails, phones, addresses."""
    examples = []
    templates = [
        "Contact {name} at {email} for details.",
        "Please reach {name}, phone {phone}, regarding the request.",
        "Send the package to {name} at {address}.",
        "{name} can be reached via {email} or {phone}.",
        "The applicant {name} listed {address} as their primary residence.",
        "Confidential: {name}'s personal email is {email}.",
        "Forward correspondence to {name}, {address}, with copy to {email}.",
        "Verified identity: {name} (dob), phone {phone}.",
    ]
    for _ in range(n):
        name = fake.name()
        email = fake.email()
        phone = fake.phone_number()
        address = fake.street_address()
        template = random.choice(templates)
        text = template.format(name=name, email=email, phone=phone, address=address)
        ents = []
        if "{name}" in template:
            ents.append((name, "name"))
        if "{email}" in template:
            ents.append((email, "email"))
        if "{phone}" in template:
            ents.append((phone, "phone"))
        if "{address}" in template:
            ents.append((address, "address"))
        examples.append(make_example(text, ents))
    return examples


def gen_financial(n: int = 500) -> list[dict]:
    """Credit cards, SSNs, IBANs, account numbers."""
    examples = []
    templates = [
        "Charge {credit_card} for the order; SSN on file {ssn}.",
        "Wire transfer from IBAN {iban} to account {account_number}.",
        "Customer SSN: {ssn}, payment method ending in {credit_card_last_4}.",
        "Credit card {credit_card} declined; please verify SSN {ssn}.",
        "International transfer: source IBAN {iban}, destination account {account_number}.",
        "Tax ID {ssn} associated with credit card {credit_card}.",
        "Account holder: SSN {ssn}, primary card {credit_card}, IBAN {iban}.",
        "Payment processed: card {credit_card}, account {account_number}.",
    ]
    for _ in range(n):
        credit_card = fake.credit_card_number()
        ssn = fake.ssn()
        iban = fake.iban()
        account_number = fake.bban()
        template = random.choice(templates)
        try:
            text = template.format(
                credit_card=credit_card,
                credit_card_last_4=credit_card[-4:],
                ssn=ssn,
                iban=iban,
                account_number=account_number,
            )
        except KeyError:
            continue
        ents = []
        if "{credit_card}" in template:
            ents.append((credit_card, "credit_card"))
        if "{ssn}" in template:
            ents.append((ssn, "ssn"))
        if "{iban}" in template:
            ents.append((iban, "iban"))
        if "{account_number}" in template:
            ents.append((account_number, "account_number"))
        examples.append(make_example(text, ents))
    return examples


personal_data = gen_personal(500)
financial_data = gen_financial(500)
print(f"personal: {len(personal_data)} examples")
print(f"financial: {len(financial_data)} examples")
print("\nSample personal example:")
print(personal_data[0])
print("\nSample financial example:")
print(financial_data[0])

# %% [markdown]
# # Train adapter A: personal-info domain
#
# LoRA config: r=8, alpha=16, target_modules = Q/K/V projections in all 12
# encoder layers. ~10-15 min on T4.

# %%
from gliner2 import GLiNER2

print("Loading base model fastino/gliner2-base-v1...")
model = GLiNER2.from_pretrained("fastino/gliner2-base-v1")

# Move model to GPU if available
if torch.cuda.is_available():
    model = model.to("cuda")

print("\nStarting training adapter_personal...")
model.train(
    data=personal_data,
    use_lora=True,
    lora_r=8,
    lora_alpha=16.0,
    lora_dropout=0.05,
    # target_modules: explicit list matching encoder Q/K/V across all 12 layers
    # Format must match the actual gliner2 parameter naming; if `target_modules`
    # is omitted, peft uses regex-matching against module names.
    lora_target_modules=["query_proj", "key_proj", "value_proj"],
    save_adapter_only=True,
    output_dir="./adapter_personal",
    num_epochs=3,
    batch_size=8,
    learning_rate=1e-4,
)
print("Saved adapter to ./adapter_personal/")

# %% [markdown]
# # Train adapter B: financial-PII domain
#
# Same training config, different domain data. ~10-15 min on T4.

# %%
print("Reloading base model (fresh weights, no carry-over from adapter A)...")
del model
torch.cuda.empty_cache() if torch.cuda.is_available() else None

model = GLiNER2.from_pretrained("fastino/gliner2-base-v1")
if torch.cuda.is_available():
    model = model.to("cuda")

print("\nStarting training adapter_financial...")
model.train(
    data=financial_data,
    use_lora=True,
    lora_r=8,
    lora_alpha=16.0,
    lora_dropout=0.05,
    lora_target_modules=["query_proj", "key_proj", "value_proj"],
    save_adapter_only=True,
    output_dir="./adapter_financial",
    num_epochs=3,
    batch_size=8,
    learning_rate=1e-4,
)
print("Saved adapter to ./adapter_financial/")

# %% [markdown]
# # Verify both adapters are PEFT-format
#
# anno's `lora.rs` expects:
# - `adapter_config.json` with fields: `r`, `lora_alpha`, `target_modules`, `base_model_name_or_path`
# - `adapter_model.safetensors` (or `adapter_weights.safetensors` fallback)
# - safetensors keys following the pattern `base_model.model.<path>.lora_{A,B}.weight`

# %%
import json
import os
from safetensors import safe_open


def verify_adapter(adapter_dir: str) -> dict:
    """Return a summary of what's in the adapter directory."""
    summary = {"path": adapter_dir, "files": [], "config": {}, "tensor_keys_sample": []}
    for f in os.listdir(adapter_dir):
        summary["files"].append(f)

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
            # Show first 3 keys to verify the PEFT pattern
            summary["tensor_keys_sample"] = keys[:3]
            # Validate the key pattern anno expects
            valid_pattern_count = sum(
                1
                for k in keys
                if k.startswith("base_model.model.") and (k.endswith(".lora_A.weight") or k.endswith(".lora_B.weight"))
            )
            summary["valid_peft_keys"] = valid_pattern_count
    return summary


for adapter in ["adapter_personal", "adapter_financial"]:
    print(f"\n=== {adapter} ===")
    summary = verify_adapter(adapter)
    print(f"  Files: {summary['files']}")
    if summary["config"]:
        cfg = summary["config"]
        print(f"  r: {cfg.get('r')}, alpha: {cfg.get('lora_alpha')}, fan_in_fan_out: {cfg.get('fan_in_fan_out', False)}")
        print(f"  target_modules: {cfg.get('target_modules')}")
        print(f"  base_model_name_or_path: {cfg.get('base_model_name_or_path')}")
    print(f"  Total tensors: {summary.get('num_tensors', 'n/a')}")
    print(f"  Valid PEFT-pattern keys: {summary.get('valid_peft_keys', 0)} / {summary.get('num_tensors', 0)}")
    print(f"  Sample keys: {summary['tensor_keys_sample']}")

# %% [markdown]
# # Optional: Quick smoke test in Python
#
# Verify the adapters actually change inference behavior in the Python
# baseline before downloading them for the Rust test.

# %%
print("Loading base model for smoke test...")
del model
torch.cuda.empty_cache() if torch.cuda.is_available() else None
model = GLiNER2.from_pretrained("fastino/gliner2-base-v1")
if torch.cuda.is_available():
    model = model.to("cuda")

test_text = (
    "Contact John Smith at john.smith@example.com or call +1-555-867-5309. "
    "His credit card 4532-1234-5678-9010 has SSN 123-45-6789 on file."
)
test_types_personal = ["name", "email", "phone"]
test_types_financial = ["credit_card", "ssn"]

print(f"\nText: {test_text}")
print("\n--- Base model (no adapter) ---")
base_personal = model.extract_entities(test_text, test_types_personal, threshold=0.5)
base_financial = model.extract_entities(test_text, test_types_financial, threshold=0.5)
print(f"  personal entities: {base_personal}")
print(f"  financial entities: {base_financial}")

print("\n--- After load_adapter('adapter_personal') ---")
model.load_adapter("./adapter_personal")
adapt_p_personal = model.extract_entities(test_text, test_types_personal, threshold=0.5)
adapt_p_financial = model.extract_entities(test_text, test_types_financial, threshold=0.5)
print(f"  personal entities: {adapt_p_personal}")
print(f"  financial entities: {adapt_p_financial}")

print("\n--- After unload + load_adapter('adapter_financial') ---")
if hasattr(model, "unload_adapter"):
    model.unload_adapter()
else:
    # Fallback if API name differs
    print("  (model has no unload_adapter; reloading base)")
    del model
    torch.cuda.empty_cache() if torch.cuda.is_available() else None
    model = GLiNER2.from_pretrained("fastino/gliner2-base-v1")
    if torch.cuda.is_available():
        model = model.to("cuda")

model.load_adapter("./adapter_financial")
adapt_f_personal = model.extract_entities(test_text, test_types_personal, threshold=0.5)
adapt_f_financial = model.extract_entities(test_text, test_types_financial, threshold=0.5)
print(f"  personal entities: {adapt_f_personal}")
print(f"  financial entities: {adapt_f_financial}")

# %% [markdown]
# # Package + download
#
# Zip both adapters and trigger download in Colab.

# %%
import shutil

shutil.make_archive("adapter_personal", "zip", ".", "adapter_personal")
shutil.make_archive("adapter_financial", "zip", ".", "adapter_financial")
print("Created:")
print(f"  adapter_personal.zip ({os.path.getsize('adapter_personal.zip')} bytes)")
print(f"  adapter_financial.zip ({os.path.getsize('adapter_financial.zip')} bytes)")

# In Colab: trigger browser download
try:
    from google.colab import files

    files.download("adapter_personal.zip")
    files.download("adapter_financial.zip")
except ImportError:
    print("Not in Colab — find the .zip files in your working directory.")

# %% [markdown]
# # After downloading: testing in Rust
#
# 1. Unzip both adapters next to each other:
#
#    ```bash
#    unzip adapter_personal.zip
#    unzip adapter_financial.zip
#    ls adapter_personal/  # adapter_config.json, adapter_model.safetensors
#    ls adapter_financial/
#    ```
#
# 2. Run anno's `real_adapter_changes_inference` test against either:
#
#    ```bash
#    GLINER2_TEST_ADAPTER_DIR=./adapter_personal \
#      cargo test -p anno --features gliner2-fastino-candle \
#        --test gliner2_fastino_candle_lora -- --ignored real_adapter \
#        --nocapture --test-threads=1
#    ```
#
# 3. Or write a custom test that loads both and asserts:
#
#    ```rust
#    let mut model = GLiNER2FastinoCandle::from_pretrained("fastino/gliner2-base-v1")?;
#    model.load_adapter("personal", Path::new("./adapter_personal"))?;
#    let names = model.extract_with_types(text, &["name", "email"], 0.5)?;
#
#    model.load_adapter("financial", Path::new("./adapter_financial"))?;
#    let financial = model.extract_with_types(text, &["credit_card", "ssn"], 0.5)?;
#    ```
