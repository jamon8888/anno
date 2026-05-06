# fastino/gliner2-multi-v1 — PyTorch forward-pass trace

Trace of the actual PyTorch inference forward pass, captured to drive the
Candle port (Phase 4). Pairs with
`gliner2-multi-v1-safetensors-keys.md` (which describes the parameter layout).

## Sources

All paths below are inside the `gliner2==1.3.1` wheel (sdist on PyPI), unpacked
to `/tmp/gliner2-src/gliner2/` in WSL Ubuntu-C. The `SpanMarkerV0` class is
pulled in from the upstream `gliner==0.2.26` wheel (gliner2 imports it
verbatim).

| Symbol | File / line range |
|---|---|
| `Extractor.__init__` | `gliner2/model.py` L78–L146 |
| `Extractor._extract_from_batch` (inference entry) | `gliner2/inference/engine.py` L221–L279 |
| `Extractor._extract_sample` | `gliner2/inference/engine.py` L281–L337 |
| `Extractor._extract_classification_result` | `gliner2/inference/engine.py` L339–L377 |
| `Extractor._extract_span_result` (count + span scoring) | `gliner2/inference/engine.py` L379–L446 |
| `Extractor.compute_span_rep_batched` | `gliner2/model.py` L499–L553 |
| `Extractor._compute_span_rep_core` | `gliner2/model.py` L555–L595 |
| `SchemaTransformer._extract_embeddings_fast` (the "schema_gather" op) | `gliner2/processor.py` L1113–L1144 |
| `SchemaTransformer._extract_embeddings_loop` (slow fallback) | `gliner2/processor.py` L1146–L1194 |
| Special-token vocabulary (`[P] [E] [C] [R] [L] [SEP_STRUCT] [SEP_TEXT] [EXAMPLE] [OUTPUT] [DESCRIPTION]`) | `gliner2/processor.py` L205–L219 |
| `CountLSTM` (the `count_embed` module) | `gliner2/layers.py` L137–L179 |
| `CompileSafeGRU` (the actual recurrent step) | `gliner2/layers.py` L6–L58 |
| `create_mlp` (factory used for `classifier`, `count_pred`) | `gliner2/layers.py` L61–L83 |
| `SpanMarkerV0` (the `span_rep` module) | upstream `gliner/modeling/span_rep.py` L463–L510 |
| `create_projection_layer` (used inside `SpanMarkerV0`) | upstream `gliner/modeling/layers.py` L66–L88 |
| `extract_elements` (the gather inside `SpanMarkerV0`) | upstream `gliner/modeling/span_rep.py` L367–L385 |

The Phase-3 ONNX export script (`scripts/gliner2_export_onnx.py`) just calls
`GLiNER2.from_pretrained` and runs `torch.onnx.export` on the whole module —
it does **not** describe the forward pass; the source above does.

---

## Big picture

The model is a single `nn.Module` whose Python-level forward is a hand-rolled
mix of tensor ops + Python control flow that is **not** wrappable as one
`forward(input_ids, attention_mask) -> (scores, spans)` call (despite what
the export script's fallback `torch.onnx.export` stub pretends). The eight
ONNX sessions in Phase 3 are just the chunks of this Python pass that **are**
expressible as static graphs; the Python glue (per-sample loops, threshold
filtering, `_find_spans`, span overlap removal) was kept out of ONNX and
re-implemented host-side.

The 5 learned modules and their roles:

| Module | Role | Output shape |
|---|---|---|
| `encoder` | mDeBERTa-v3 backbone over `[schemas] [SEP_TEXT] [text]` | `(B, S, 768)` |
| `span_rep.span_rep_layer` | per-(start, end) span MLP from token states | `(B, L, max_width, 768)` |
| `count_embed` (= `CountLSTM`) | unrolls a per-instance × per-field "structure" embedding | `(count, num_fields, 768)` |
| `count_pred` | predicts how many instances to extract from the `[P]` token | `(20,)` logits |
| `classifier` | scores classification labels (entities don't use it) | `(num_labels,)` logits |

There is **no** dedicated `schema_gather` module and **no** dedicated
`scorer` module. Both are non-parametric:

- "schema_gather" = an `index_select` from `last_hidden_state` at the seq
  positions of the special tokens `[P] [E] [C] [R] [L]`
  (`processor.py` L1140–L1141).
- "scorer" = a single `torch.einsum('lkd,bpd->bplk', span_rep, struct_proj)`
  followed by `sigmoid` (`engine.py` L424–L426).

---

## Input layout (what the encoder actually sees)

The processor builds **one packed sequence per (text, schema) pair**:

```
[CLS]
   for each schema task k:
       "(" [P] schema_name [SEP_STRUCT]                 # P-token = task prompt
            ([E] field_name)*  or  ([L] label_name)*    # one per field/label
       ")"
   [SEP_TEXT]
   word_1 word_2 ... word_T
[SEP]
```

Where the special-token alphabet is:

| Token | Meaning |
|---|---|
| `[P]` | task prompt — one per schema; its hidden state feeds `count_pred` and is `embs[0]` |
| `[E]` | entity / structure-field marker — one per field; its hidden state is a per-field row of `embs[1:]` |
| `[L]` | label marker (classification) — one per label; its hidden state feeds `classifier` |
| `[C]` | classification field marker (used inside JSON structures) |
| `[R]` | relation field marker |
| `[SEP_STRUCT]`, `[SEP_TEXT]` | structural separators (not gathered) |

After encoding, `_extract_embeddings_fast` does two distinct gathers per
sample:

1. **Text gather** — `token_embeddings[i, text_word_indices[i]]` →
   `(n_words, 768)`. Picks the *first* sub-word of every input word
   (`token_pooling = "first"` in this checkpoint).
2. **Schema gather** — for every schema `j` and every special-token position
   `pos` recorded during preprocessing:
   `token_embeddings[i, pos]` → `embs_per_schema[i][j]` is a list of 768-d
   vectors. The first vector is always the `[P]` row; the rest are one row
   per `[E]` / `[L]` / `[C]` / `[R]` in order.

This is the "schema_gather" the Phase-3 ONNX export factored out. It's a
plain `index_select`, not a learned op.

---

## Pseudocode trace (single sample, inference)

```python
# ── 0. Pack: text + every schema head into one sequence ──────────────────
input_ids, attention_mask = processor.collate(text, schemas)
# input_ids: (1, S)

# ── 1. Encode ────────────────────────────────────────────────────────────
hidden = encoder(input_ids, attention_mask).last_hidden_state
# hidden: (1, S, 768)

# ── 2. Schema gather (non-parametric) ────────────────────────────────────
text_emb  = hidden[0, text_word_indices]                # (T, 768)
embs_per_schema = []
for j in range(num_schemas):
    schema_token_positions = special_indices[j]         # list of seq positions
    embs_per_schema.append(hidden[0, schema_token_positions])
    # shape: (1 + num_fields_j, 768), row 0 = [P], rows 1: = [E]/[L]/[C]/[R]

# ── 3. Span representation (only if any schema is span-based) ────────────
# SpanMarkerV0:  start_proj, end_proj are per-token MLPs.
start_rep = span_rep.project_start(text_emb.unsqueeze(0))   # (1, T, 768)
end_rep   = span_rep.project_end  (text_emb.unsqueeze(0))   # (1, T, 768)

# Build all (start, start+w) pairs for w in [0, max_width):
starts = torch.arange(T).unsqueeze(1).expand(-1, max_width)            # (T, W)
ends   = starts + torch.arange(max_width).unsqueeze(0)                 # (T, W)
valid  = ends < T                                                      # (T, W)
safe_spans = torch.stack([
    torch.where(valid, starts, 0).reshape(-1),
    torch.where(valid, ends,   0).reshape(-1),
], dim=-1).unsqueeze(0)                                                # (1, T*W, 2)

start_at = extract_elements(start_rep, safe_spans[..., 0])             # (1, T*W, 768)
end_at   = extract_elements(end_rep,   safe_spans[..., 1])             # (1, T*W, 768)
cat = torch.cat([start_at, end_at], dim=-1).relu()                     # (1, T*W, 1536)
span_rep_out = span_rep.out_project(cat).view(1, T, max_width, 768)    # (1, T, W, 768)
# Per-sample slice: (T, W, 768)

# ── 4. Per-schema decode ─────────────────────────────────────────────────
for j, schema in enumerate(schemas):
    embs = embs_per_schema[j]                # (1 + F, 768)
    p_emb       = embs[0]                    # (768,)        -- [P]
    field_embs  = embs[1:]                   # (F, 768)      -- [E] or [L]

    if task == "classifications":
        # ─── classifier head ──────────────────────────────────────────
        # classifier: 768 → 1536 → 1
        logits = classifier(field_embs).squeeze(-1)        # (F,)
        # softmax / sigmoid + argmax → label
        continue

    # ─── span tasks: predict count, then unroll structure_proj, then score
    # count_pred: 768 → 1536 → 20
    pred_count = int(count_pred(p_emb.unsqueeze(0)).argmax(-1))   # ∈ [0, 19]

    if pred_count == 0 or T == 0:
        continue

    # count_embed = CountLSTM:
    #   pos_seq[t] = pos_embedding[t] for t in 0..pred_count
    #   h_0 = field_embs                                    # (F, 768)
    #   h_t = GRU_step(pos_seq[t], h_{t-1})                 # (pred_count, F, 768)
    #   struct_proj = projector(cat[h_t, field_embs])       # (pred_count, F, 768)
    struct_proj = count_embed(field_embs, pred_count)
    # struct_proj: (pred_count, F, 768)   -- "instance × field" representation

    # ─── span scoring (the einsum) ────────────────────────────────────
    # span_rep_out_per_sample: (T, W, 768)   indices (l, k, d)
    # struct_proj            : (count, F, 768)  indices (b, p, d)
    scores = torch.sigmoid(
        torch.einsum('lkd,bpd->bplk', span_rep_out_per_sample, struct_proj)
    )
    # scores: (count, F, T, W)  →  for each instance b, field p: a (T, W) heatmap

    # ─── Python-side decode (kept out of ONNX) ────────────────────────
    for inst in range(pred_count):
        for field in range(F):
            heatmap = scores[inst, field]                   # (T, W)
            picks = (heatmap >= threshold).nonzero()         # (start, width) pairs
            # → emit (text[start_map[s] : end_map[s+w]], confidence)
```

---

## Per-module I/O contract

### 1. `encoder` (mDeBERTa-v3-base)

- **In:** `input_ids: (B, S) int64`, `attention_mask: (B, S) int64`.
- **Out:** `last_hidden_state: (B, S, 768) float`.

The packed sequence interleaves schema tokens and text words; positions of
`[P]/[E]/[L]/[C]/[R]` are tracked by the processor in
`batch.schema_special_indices` so they can be gathered out without
re-tokenizing.

### 2. `span_rep.span_rep_layer` (`SpanMarkerV0`)

- **In:** `h: (B, T, 768)` (the *text-only* token states after pooling),
  `span_idx: (B, T*max_width, 2)` of (start, end) pairs.
- **Out:** `(B, T, max_width, 768)`.

Internals (`gliner/modeling/span_rep.py:490–510`):

```
start_rep = project_start(h)           # 768 →[Linear→ReLU→Dropout→Linear]→ 768
end_rep   = project_end(h)             # same shape
start_at  = gather(start_rep, span_idx[..., 0])
end_at    = gather(end_rep,   span_idx[..., 1])
out       = out_project(relu(cat[start_at, end_at])).view(B, T, max_width, 768)
```

The 2-layer MLPs match the safetensors `(3072, 768) / (768, 3072)` shapes
because `create_projection_layer` is `Linear(D, 4·D) → ReLU → Dropout →
Linear(4·D, D)` with `D=768` (so the inner dim is 3072, not 1536 — the 1536
in `out_project` is the input dim from `cat[start, end]`, distinct).

### 3. `count_embed` (`CountLSTM`, despite the `count_lstm` config name)

- **In:** `pc_emb: (F, 768)` = the per-field schema embeddings (the `embs[1:]`
  rows), `gold_count_val: int` = predicted `pred_count` from `count_pred`.
- **Out:** `(min(pred_count, 20), F, 768)`.

Internals (`layers.py:159–179`):

```
L = min(gold_count_val, 20)
pos_seq = pos_embedding(arange(L))                  # (L, 768)
pos_seq = pos_seq.unsqueeze(1).expand(L, F, 768)    # broadcast across fields
h       = GRU(pos_seq, h0=pc_emb)                   # (L, F, 768)
out     = projector(cat[h, pc_emb_broadcast])       # (L, F, 768)
```

Where `projector` is `Linear(1536, 3072) → ReLU → Linear(3072, 768)` (the
indices-`0`-and-`2` MLP from the safetensors map; ReLU has no params).

The GRU uses PyTorch (reset, update, new) gate ordering, hidden = input =
768. The implementation is `CompileSafeGRU` (a hand-rolled loop that mimics
`nn.GRU` so `torch.compile` can trace it), but the parameter names match
`nn.GRU` for checkpoint compat.

**Conditioning meaning:** the GRU's *initial hidden state* is the field's
schema embedding, and the *input* at every step is just the position
embedding of the instance index. So the model is conditioning each
"instance × field" slot on (a) which field it is (via `h_0`) and (b) which
instance number within that field (via the position embedding). That's
exactly how the count head conditions span scoring: a different
`struct_proj[b, p, :]` row for every (instance, field) pair, scored against
the same shared `span_rep` tensor.

### 4. `count_pred`

- **In:** `p_emb: (1, 768)` — the `[P]` row of `embs_per_schema[j]` only.
- **Out:** `(1, 20)` logits over the discrete count buckets `[0, 19]`.

Internals: `Linear(768, 1536) → ReLU → Linear(1536, 20)` (`create_mlp`
with `intermediate_dims=[1536]`, `activation="relu"`, `dropout=0` — no
`LayerNorm`).

**Used at inference as** `pred_count = argmax(count_pred(p_emb))`. There is
no soft / multi-instance fallback at inference; a hard count is picked.

### 5. `classifier`

- **In:** `field_embs: (num_labels, 768)` — the `[L]`-token rows of a
  classifications schema (NOT spans). Note: for span heads this module is
  **not invoked** — span scoring is done entirely by the einsum above.
- **Out:** `(num_labels,)` logits.

Internals: `Linear(768, 1536) → ReLU → Linear(1536, 1)` (`create_mlp` with
`intermediate_dims=[1536]`). The output dim is **1** because each label has
its own 768-d input row — the model produces one scalar per label by
stacking. So the safetensors `(1, 1536)` final layer is correct.

The `class_act` field on each classification config decides whether the
output is `sigmoid` (multi-label) or `softmax` (single-label); the forward
method itself only emits raw logits.

---

## The "scorer" — what the einsum actually computes

```
scores[b, p, l, k] = Σ_d span_rep[l, k, d] · struct_proj[b, p, d]
```

i.e. an inner product between (per-span, per-(instance, field)) 768-d
vectors. In NLP terms it's "score of span (l, k) being assigned to field
`p` of instance `b`", and the model uses the same span_rep tensor across
all `(b, p)` pairs — only `struct_proj` changes. After `sigmoid` it's
treated as an independent Bernoulli per (b, p, l, k) cell.

For **entities** specifically (which have no count concept), the model
still calls `count_embed(...)` with the predicted count; entities just
read the `inst=0` slice (`scores[0, p, l, k]`) and union over instances —
see `_extract_entities` (`engine.py:448–507`), which slices
`span_scores[0, :, -text_len:]` and picks per-name spans via threshold +
`_find_spans`.

The `-text_len:` slice in `_extract_entities` / `_extract_relations` /
`_extract_structures` is interesting: span_scores is computed over the
full **packed** sequence's worth of span slots in the structure case
(because some classification *fields* embedded inside a JSON structure
look up scores at positions before the text), but entity / relation /
structure decode then selects only the trailing text portion. For the
zero-shot entity case this is just `span_scores[0, :, :, :]` reshaped.

---

## Non-obvious tensor manipulations

1. **Non-batched span_rep slicing.** `compute_span_rep_batched`
   pads `token_embs_list` into `(B, max_T, 768)`, runs SpanMarkerV0 once,
   then slices `span_rep[i, :tl, :, :]` per sample to get
   `(tl, max_width, 768)`. This works only because SpanMarkerV0 is purely
   pointwise (per-token MLP + gather), no cross-position mixing.

2. **Invalid-span masking.** `safe_spans` replaces all `(start, end)` pairs
   where `end >= text_len` with `(0, 0)` so the `gather` is always safe;
   the corresponding `span_mask` (= "True for invalid") is later used to
   zero out their scores during loss. **At inference**, the threshold step
   plus the `0 <= start < text_len and end <= text_len` check inside
   `_find_spans` (engine.py:702–730) handles it instead.

3. **`count_embed` sequence layout.** Pos embedding is shape `(20, 768)`
   and `gold_count_val` is capped at 20 (matches `count_pred` output dim
   of 20). The GRU runs **across instances**, not across fields — fields
   are just the batch-like axis of the GRU.

4. **No softmax over fields.** The einsum scoring + sigmoid is
   per-(b, p, l, k) Bernoulli. The model never normalises across the
   `p` (field) axis, which is why span_scores can fire for multiple
   labels on the same span.

5. **`text_len` is word-count, not subword-count.** After
   `_extract_embeddings_fast`, `T` is the number of *words* (one row per
   first-subword). The `(start_map, end_map)` arrays inside
   `PreprocessedBatch` are what convert (start_word, end_word) back to
   character offsets in the original text.

6. **Special tokens are picked up by string identity.** The processor adds
   `[P] [E] [L] [C] [R] [SEP_STRUCT] [SEP_TEXT] [EXAMPLE] [OUTPUT]
   [DESCRIPTION]` as `additional_special_tokens` then resizes the encoder's
   word embedding (`encoder.resize_token_embeddings`). That's why the dumped
   `word_embeddings.weight` is `(250112, 768)` and not the stock
   `(250101, 768)` of mdeberta-v3-base — the +11 rows are these markers.

---

## What the Phase-3 ONNX 8-session split corresponds to

| ONNX session | PyTorch op | Source |
|---|---|---|
| `encoder.onnx` | `encoder(input_ids, attention_mask).last_hidden_state` | `engine.py:232–235` |
| `token_gather.onnx` | `hidden[batch, text_word_indices]` | `processor.py:1130` |
| `schema_gather.onnx` | `hidden[batch, schema_special_indices]` | `processor.py:1141` (per schema) |
| `span_rep.onnx` | `SpanMarkerV0.forward(text_emb, safe_spans)` | `model.py:_compute_span_rep_core` |
| `count_pred_argmax.onnx` | `count_pred(p_emb).argmax(-1)` | `engine.py:410–411` |
| `count_lstm_fixed.onnx` | `count_embed(field_embs, pred_count)` | `engine.py:423` |
| `scorer.onnx` | `sigmoid(einsum('lkd,bpd->bplk', span_rep, struct_proj))` | `engine.py:424–426` |
| `classifier.onnx` | `classifier(field_embs).squeeze(-1)` | `engine.py:354` |

The Python loops, `argmax`-into-Python-int, threshold comparisons, span
overlap removal, validators, and char-offset mapping are all kept out of
ONNX — they live host-side in `_extract_sample` and `_find_spans`.

---

## Implications for the Candle port

- We need **two** different decode paths: classification and span. They
  share the encoder + the gather but never share the head.
- The "scorer" is just one `einsum + sigmoid`, easy to express.
- The "schema_gather" is just `index_select` on dim 1 of the encoder
  output. `batch.schema_special_indices` (already computed by the Rust
  processor in Phase 2) gives the positions directly.
- `count_embed` requires a custom step-loop GRU (or the
  `CompileSafeGRU` impl rewritten in Candle); the official
  `candle_nn::rnn::GRU` should work since the gate layout matches PyTorch
  exactly (verified in the safetensors notes).
- `count_pred.argmax` is host-side: keep it that way (one scalar per
  schema), then unroll `count_embed` for that many steps. Don't try to
  make `pred_count` a tensor through the rest of the graph.
- For entities, set `pred_count = 1` is **not** correct — the model
  predicts an actual count and the entity decode path just always reads
  `inst=0`. So we can't shortcut the count_embed call away for the
  entity-only case unless we accept that single iteration.
