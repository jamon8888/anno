# fastino/gliner2-multi-v1 safetensors map

Ground-truth schema dump of `model.safetensors` from `fastino/gliner2-multi-v1`,
captured to drive the Candle port (Phase 4).

## Snapshot

- HF cache snapshot: `/home/nmarchitecte/.cache/huggingface/hub/models--fastino--gliner2-multi-v1/snapshots/cc151f5b0ce4f7010c3ae8884527dd43dddf9d21/`
- Snapshot revision: `cc151f5b0ce4f7010c3ae8884527dd43dddf9d21`
- File: `model.safetensors`
- File size: 1,228,421,964 bytes (~1171 MiB / ~1.23 GB)
- Total parameter count: **307,098,645** (~307 M)
- Total tensor keys: **227**

## Top-level config (`config.json`)

```json
{
  "_attn_implementation_autoset": true,
  "counting_layer": "count_lstm",
  "max_width": 8,
  "model_name": "microsoft/mdeberta-v3-base",
  "model_type": "extractor",
  "token_pooling": "first",
  "transformers_version": "4.51.0"
}
```

> Note: `counting_layer` is named `"count_lstm"` in config, but the actual
> weights are a **GRU** (`count_embed.gru.*`). The "lstm" label is a misnomer.

## Encoder config (`encoder_config/config.json`)

| Field | Value |
|---|---|
| `model_type` | `deberta-v2` |
| `hidden_size` | 768 |
| `vocab_size` | 250112 |
| `num_hidden_layers` | 12 |
| `num_attention_heads` | 12 |
| `intermediate_size` | 3072 |
| `type_vocab_size` | 0 (no token-type embeddings) |
| `max_position_embeddings` | 512 |
| `position_biased_input` | false |
| `relative_attention` | true |
| `pos_att_type` | `["p2c", "c2p"]` |
| `position_buckets` | 256 |
| `share_att_key` | true |
| `norm_rel_ebd` | `layer_norm` |
| `max_relative_positions` | -1 (defaults to `max_position_embeddings`) |
| `pad_token_id` | 0 |
| `layer_norm_eps` | 1e-7 |
| `hidden_act` | `gelu` |
| `legacy` | true |

### DeBERTa version: v2 vs v3

`encoder.encoder.rel_embeddings.weight` has shape `(512, 768)`. With
`position_buckets = 256`, this matches the DeBERTa-v3 convention of
`[2 * position_buckets, hidden_size]` (i.e. one embedding per signed bucket
in `[-256, +256)`). Combined with `share_att_key=true`, `position_biased_input=false`
and `pos_att_type=[p2c, c2p]`, this is the **DeBERTa-v3 / mDeBERTa-v3-base**
attention scheme — which is consistent with `model_name = "microsoft/mdeberta-v3-base"`.
HuggingFace exposes it under the `model_type: "deberta-v2"` class because the
`DebertaV2Model` implementation handles both v2 and v3 (the v3 differences are
purely in pre-training / vocab; the modeling code is shared).

There is **no** `rel_embeddings.bias`, **no** `LayerNorm` next to
`rel_embeddings` (despite `norm_rel_ebd = "layer_norm"` — see "Concerns" below),
**no** `position_embeddings.weight`, **no** `token_type_embeddings.weight`.

---

## Heads / Modules

Heads are grouped by top-level prefix. Within each group, lines are sorted
alphabetically as `<shape>\t<key>`.

### `encoder` — DeBERTa-v2/v3 (mDeBERTa-v3-base shape)

198 keys, ~278.6 M params. Word embeddings dominate
(250112 * 768 = 192.1 M params).

```
(250112, 768)	encoder.embeddings.word_embeddings.weight
(768,)	encoder.embeddings.LayerNorm.bias
(768,)	encoder.embeddings.LayerNorm.weight
(768,)	encoder.encoder.LayerNorm.bias
(768,)	encoder.encoder.LayerNorm.weight
(512, 768)	encoder.encoder.rel_embeddings.weight
(768,)	encoder.encoder.layer.0.attention.output.LayerNorm.bias
(768,)	encoder.encoder.layer.0.attention.output.LayerNorm.weight
(768,)	encoder.encoder.layer.0.attention.output.dense.bias
(768, 768)	encoder.encoder.layer.0.attention.output.dense.weight
(768,)	encoder.encoder.layer.0.attention.self.key_proj.bias
(768, 768)	encoder.encoder.layer.0.attention.self.key_proj.weight
(768,)	encoder.encoder.layer.0.attention.self.query_proj.bias
(768, 768)	encoder.encoder.layer.0.attention.self.query_proj.weight
(768,)	encoder.encoder.layer.0.attention.self.value_proj.bias
(768, 768)	encoder.encoder.layer.0.attention.self.value_proj.weight
(3072,)	encoder.encoder.layer.0.intermediate.dense.bias
(3072, 768)	encoder.encoder.layer.0.intermediate.dense.weight
(768,)	encoder.encoder.layer.0.output.LayerNorm.bias
(768,)	encoder.encoder.layer.0.output.LayerNorm.weight
(768,)	encoder.encoder.layer.0.output.dense.bias
(768, 3072)	encoder.encoder.layer.0.output.dense.weight
(768,)	encoder.encoder.layer.1.attention.output.LayerNorm.bias
(768,)	encoder.encoder.layer.1.attention.output.LayerNorm.weight
(768,)	encoder.encoder.layer.1.attention.output.dense.bias
(768, 768)	encoder.encoder.layer.1.attention.output.dense.weight
(768,)	encoder.encoder.layer.1.attention.self.key_proj.bias
(768, 768)	encoder.encoder.layer.1.attention.self.key_proj.weight
(768,)	encoder.encoder.layer.1.attention.self.query_proj.bias
(768, 768)	encoder.encoder.layer.1.attention.self.query_proj.weight
(768,)	encoder.encoder.layer.1.attention.self.value_proj.bias
(768, 768)	encoder.encoder.layer.1.attention.self.value_proj.weight
(3072,)	encoder.encoder.layer.1.intermediate.dense.bias
(3072, 768)	encoder.encoder.layer.1.intermediate.dense.weight
(768,)	encoder.encoder.layer.1.output.LayerNorm.bias
(768,)	encoder.encoder.layer.1.output.LayerNorm.weight
(768,)	encoder.encoder.layer.1.output.dense.bias
(768, 3072)	encoder.encoder.layer.1.output.dense.weight
(768,)	encoder.encoder.layer.2.attention.output.LayerNorm.bias
(768,)	encoder.encoder.layer.2.attention.output.LayerNorm.weight
(768,)	encoder.encoder.layer.2.attention.output.dense.bias
(768, 768)	encoder.encoder.layer.2.attention.output.dense.weight
(768,)	encoder.encoder.layer.2.attention.self.key_proj.bias
(768, 768)	encoder.encoder.layer.2.attention.self.key_proj.weight
(768,)	encoder.encoder.layer.2.attention.self.query_proj.bias
(768, 768)	encoder.encoder.layer.2.attention.self.query_proj.weight
(768,)	encoder.encoder.layer.2.attention.self.value_proj.bias
(768, 768)	encoder.encoder.layer.2.attention.self.value_proj.weight
(3072,)	encoder.encoder.layer.2.intermediate.dense.bias
(3072, 768)	encoder.encoder.layer.2.intermediate.dense.weight
(768,)	encoder.encoder.layer.2.output.LayerNorm.bias
(768,)	encoder.encoder.layer.2.output.LayerNorm.weight
(768,)	encoder.encoder.layer.2.output.dense.bias
(768, 3072)	encoder.encoder.layer.2.output.dense.weight
(768,)	encoder.encoder.layer.3.attention.output.LayerNorm.bias
(768,)	encoder.encoder.layer.3.attention.output.LayerNorm.weight
(768,)	encoder.encoder.layer.3.attention.output.dense.bias
(768, 768)	encoder.encoder.layer.3.attention.output.dense.weight
(768,)	encoder.encoder.layer.3.attention.self.key_proj.bias
(768, 768)	encoder.encoder.layer.3.attention.self.key_proj.weight
(768,)	encoder.encoder.layer.3.attention.self.query_proj.bias
(768, 768)	encoder.encoder.layer.3.attention.self.query_proj.weight
(768,)	encoder.encoder.layer.3.attention.self.value_proj.bias
(768, 768)	encoder.encoder.layer.3.attention.self.value_proj.weight
(3072,)	encoder.encoder.layer.3.intermediate.dense.bias
(3072, 768)	encoder.encoder.layer.3.intermediate.dense.weight
(768,)	encoder.encoder.layer.3.output.LayerNorm.bias
(768,)	encoder.encoder.layer.3.output.LayerNorm.weight
(768,)	encoder.encoder.layer.3.output.dense.bias
(768, 3072)	encoder.encoder.layer.3.output.dense.weight
(768,)	encoder.encoder.layer.4.attention.output.LayerNorm.bias
(768,)	encoder.encoder.layer.4.attention.output.LayerNorm.weight
(768,)	encoder.encoder.layer.4.attention.output.dense.bias
(768, 768)	encoder.encoder.layer.4.attention.output.dense.weight
(768,)	encoder.encoder.layer.4.attention.self.key_proj.bias
(768, 768)	encoder.encoder.layer.4.attention.self.key_proj.weight
(768,)	encoder.encoder.layer.4.attention.self.query_proj.bias
(768, 768)	encoder.encoder.layer.4.attention.self.query_proj.weight
(768,)	encoder.encoder.layer.4.attention.self.value_proj.bias
(768, 768)	encoder.encoder.layer.4.attention.self.value_proj.weight
(3072,)	encoder.encoder.layer.4.intermediate.dense.bias
(3072, 768)	encoder.encoder.layer.4.intermediate.dense.weight
(768,)	encoder.encoder.layer.4.output.LayerNorm.bias
(768,)	encoder.encoder.layer.4.output.LayerNorm.weight
(768,)	encoder.encoder.layer.4.output.dense.bias
(768, 3072)	encoder.encoder.layer.4.output.dense.weight
(768,)	encoder.encoder.layer.5.attention.output.LayerNorm.bias
(768,)	encoder.encoder.layer.5.attention.output.LayerNorm.weight
(768,)	encoder.encoder.layer.5.attention.output.dense.bias
(768, 768)	encoder.encoder.layer.5.attention.output.dense.weight
(768,)	encoder.encoder.layer.5.attention.self.key_proj.bias
(768, 768)	encoder.encoder.layer.5.attention.self.key_proj.weight
(768,)	encoder.encoder.layer.5.attention.self.query_proj.bias
(768, 768)	encoder.encoder.layer.5.attention.self.query_proj.weight
(768,)	encoder.encoder.layer.5.attention.self.value_proj.bias
(768, 768)	encoder.encoder.layer.5.attention.self.value_proj.weight
(3072,)	encoder.encoder.layer.5.intermediate.dense.bias
(3072, 768)	encoder.encoder.layer.5.intermediate.dense.weight
(768,)	encoder.encoder.layer.5.output.LayerNorm.bias
(768,)	encoder.encoder.layer.5.output.LayerNorm.weight
(768,)	encoder.encoder.layer.5.output.dense.bias
(768, 3072)	encoder.encoder.layer.5.output.dense.weight
(768,)	encoder.encoder.layer.6.attention.output.LayerNorm.bias
(768,)	encoder.encoder.layer.6.attention.output.LayerNorm.weight
(768,)	encoder.encoder.layer.6.attention.output.dense.bias
(768, 768)	encoder.encoder.layer.6.attention.output.dense.weight
(768,)	encoder.encoder.layer.6.attention.self.key_proj.bias
(768, 768)	encoder.encoder.layer.6.attention.self.key_proj.weight
(768,)	encoder.encoder.layer.6.attention.self.query_proj.bias
(768, 768)	encoder.encoder.layer.6.attention.self.query_proj.weight
(768,)	encoder.encoder.layer.6.attention.self.value_proj.bias
(768, 768)	encoder.encoder.layer.6.attention.self.value_proj.weight
(3072,)	encoder.encoder.layer.6.intermediate.dense.bias
(3072, 768)	encoder.encoder.layer.6.intermediate.dense.weight
(768,)	encoder.encoder.layer.6.output.LayerNorm.bias
(768,)	encoder.encoder.layer.6.output.LayerNorm.weight
(768,)	encoder.encoder.layer.6.output.dense.bias
(768, 3072)	encoder.encoder.layer.6.output.dense.weight
(768,)	encoder.encoder.layer.7.attention.output.LayerNorm.bias
(768,)	encoder.encoder.layer.7.attention.output.LayerNorm.weight
(768,)	encoder.encoder.layer.7.attention.output.dense.bias
(768, 768)	encoder.encoder.layer.7.attention.output.dense.weight
(768,)	encoder.encoder.layer.7.attention.self.key_proj.bias
(768, 768)	encoder.encoder.layer.7.attention.self.key_proj.weight
(768,)	encoder.encoder.layer.7.attention.self.query_proj.bias
(768, 768)	encoder.encoder.layer.7.attention.self.query_proj.weight
(768,)	encoder.encoder.layer.7.attention.self.value_proj.bias
(768, 768)	encoder.encoder.layer.7.attention.self.value_proj.weight
(3072,)	encoder.encoder.layer.7.intermediate.dense.bias
(3072, 768)	encoder.encoder.layer.7.intermediate.dense.weight
(768,)	encoder.encoder.layer.7.output.LayerNorm.bias
(768,)	encoder.encoder.layer.7.output.LayerNorm.weight
(768,)	encoder.encoder.layer.7.output.dense.bias
(768, 3072)	encoder.encoder.layer.7.output.dense.weight
(768,)	encoder.encoder.layer.8.attention.output.LayerNorm.bias
(768,)	encoder.encoder.layer.8.attention.output.LayerNorm.weight
(768,)	encoder.encoder.layer.8.attention.output.dense.bias
(768, 768)	encoder.encoder.layer.8.attention.output.dense.weight
(768,)	encoder.encoder.layer.8.attention.self.key_proj.bias
(768, 768)	encoder.encoder.layer.8.attention.self.key_proj.weight
(768,)	encoder.encoder.layer.8.attention.self.query_proj.bias
(768, 768)	encoder.encoder.layer.8.attention.self.query_proj.weight
(768,)	encoder.encoder.layer.8.attention.self.value_proj.bias
(768, 768)	encoder.encoder.layer.8.attention.self.value_proj.weight
(3072,)	encoder.encoder.layer.8.intermediate.dense.bias
(3072, 768)	encoder.encoder.layer.8.intermediate.dense.weight
(768,)	encoder.encoder.layer.8.output.LayerNorm.bias
(768,)	encoder.encoder.layer.8.output.LayerNorm.weight
(768,)	encoder.encoder.layer.8.output.dense.bias
(768, 3072)	encoder.encoder.layer.8.output.dense.weight
(768,)	encoder.encoder.layer.9.attention.output.LayerNorm.bias
(768,)	encoder.encoder.layer.9.attention.output.LayerNorm.weight
(768,)	encoder.encoder.layer.9.attention.output.dense.bias
(768, 768)	encoder.encoder.layer.9.attention.output.dense.weight
(768,)	encoder.encoder.layer.9.attention.self.key_proj.bias
(768, 768)	encoder.encoder.layer.9.attention.self.key_proj.weight
(768,)	encoder.encoder.layer.9.attention.self.query_proj.bias
(768, 768)	encoder.encoder.layer.9.attention.self.query_proj.weight
(768,)	encoder.encoder.layer.9.attention.self.value_proj.bias
(768, 768)	encoder.encoder.layer.9.attention.self.value_proj.weight
(3072,)	encoder.encoder.layer.9.intermediate.dense.bias
(3072, 768)	encoder.encoder.layer.9.intermediate.dense.weight
(768,)	encoder.encoder.layer.9.output.LayerNorm.bias
(768,)	encoder.encoder.layer.9.output.LayerNorm.weight
(768,)	encoder.encoder.layer.9.output.dense.bias
(768, 3072)	encoder.encoder.layer.9.output.dense.weight
(768,)	encoder.encoder.layer.10.attention.output.LayerNorm.bias
(768,)	encoder.encoder.layer.10.attention.output.LayerNorm.weight
(768,)	encoder.encoder.layer.10.attention.output.dense.bias
(768, 768)	encoder.encoder.layer.10.attention.output.dense.weight
(768,)	encoder.encoder.layer.10.attention.self.key_proj.bias
(768, 768)	encoder.encoder.layer.10.attention.self.key_proj.weight
(768,)	encoder.encoder.layer.10.attention.self.query_proj.bias
(768, 768)	encoder.encoder.layer.10.attention.self.query_proj.weight
(768,)	encoder.encoder.layer.10.attention.self.value_proj.bias
(768, 768)	encoder.encoder.layer.10.attention.self.value_proj.weight
(3072,)	encoder.encoder.layer.10.intermediate.dense.bias
(3072, 768)	encoder.encoder.layer.10.intermediate.dense.weight
(768,)	encoder.encoder.layer.10.output.LayerNorm.bias
(768,)	encoder.encoder.layer.10.output.LayerNorm.weight
(768,)	encoder.encoder.layer.10.output.dense.bias
(768, 3072)	encoder.encoder.layer.10.output.dense.weight
(768,)	encoder.encoder.layer.11.attention.output.LayerNorm.bias
(768,)	encoder.encoder.layer.11.attention.output.LayerNorm.weight
(768,)	encoder.encoder.layer.11.attention.output.dense.bias
(768, 768)	encoder.encoder.layer.11.attention.output.dense.weight
(768,)	encoder.encoder.layer.11.attention.self.key_proj.bias
(768, 768)	encoder.encoder.layer.11.attention.self.key_proj.weight
(768,)	encoder.encoder.layer.11.attention.self.query_proj.bias
(768, 768)	encoder.encoder.layer.11.attention.self.query_proj.weight
(768,)	encoder.encoder.layer.11.attention.self.value_proj.bias
(768, 768)	encoder.encoder.layer.11.attention.self.value_proj.weight
(3072,)	encoder.encoder.layer.11.intermediate.dense.bias
(3072, 768)	encoder.encoder.layer.11.intermediate.dense.weight
(768,)	encoder.encoder.layer.11.output.LayerNorm.bias
(768,)	encoder.encoder.layer.11.output.LayerNorm.weight
(768,)	encoder.encoder.layer.11.output.dense.bias
(768, 3072)	encoder.encoder.layer.11.output.dense.weight
```

Per-layer key inventory (16 keys × 12 layers = 192):

- `attention.self.{query_proj,key_proj,value_proj}.{weight,bias}` — separate Q/K/V projections (DeBERTa-v2/v3 convention; not a fused QKV)
- `attention.output.dense.{weight,bias}` + `attention.output.LayerNorm.{weight,bias}` — post-attention projection + LayerNorm
- `intermediate.dense.{weight,bias}` — FFN up-projection (768 → 3072)
- `output.dense.{weight,bias}` + `output.LayerNorm.{weight,bias}` — FFN down-projection (3072 → 768) + post-FFN LayerNorm

No biases / projections specific to relative attention beyond `rel_embeddings.weight` are present, which is consistent with `share_att_key=true` (the same Q/K projections are reused for content↔position attention).

### `span_rep` — span representation head

12 keys, ~14.2 M params. Builds a per-span representation from start/end token
hidden states.

```
(3072,)	span_rep.span_rep_layer.out_project.0.bias
(3072, 1536)	span_rep.span_rep_layer.out_project.0.weight
(768,)	span_rep.span_rep_layer.out_project.3.bias
(768, 3072)	span_rep.span_rep_layer.out_project.3.weight
(3072,)	span_rep.span_rep_layer.project_end.0.bias
(3072, 768)	span_rep.span_rep_layer.project_end.0.weight
(768,)	span_rep.span_rep_layer.project_end.3.bias
(768, 3072)	span_rep.span_rep_layer.project_end.3.weight
(3072,)	span_rep.span_rep_layer.project_start.0.bias
(3072, 768)	span_rep.span_rep_layer.project_start.0.weight
(768,)	span_rep.span_rep_layer.project_start.3.bias
(768, 3072)	span_rep.span_rep_layer.project_start.3.weight
```

Each of `project_start`, `project_end`, `out_project` is a 2-layer MLP using
`nn.Sequential` with indices `0` and `3` (i.e. `Linear → GELU/activation →
Dropout → Linear`, where indices 1 and 2 are activation + dropout with no
parameters):

- `project_start`: 768 → 3072 → 768  (per-token start-state projection)
- `project_end`:   768 → 3072 → 768  (per-token end-state projection)
- `out_project`:  1536 → 3072 → 768  (concat[start,end]: 2*768 → 768)

### `count_embed` — count-prefix embedding (GRU-based, despite "count_lstm" config)

9 keys, ~6.8 M params. Builds a small recurrent embedding over the
(start, end) token states for the counting head.

```
(2304,)	count_embed.gru.bias_hh_l0
(2304,)	count_embed.gru.bias_ih_l0
(2304, 768)	count_embed.gru.weight_hh_l0
(2304, 768)	count_embed.gru.weight_ih_l0
(20, 768)	count_embed.pos_embedding.weight
(3072,)	count_embed.projector.0.bias
(3072, 1536)	count_embed.projector.0.weight
(768,)	count_embed.projector.2.bias
(768, 3072)	count_embed.projector.2.weight
```

- `gru.*_l0` — single-layer GRU. Gate factor of 3 (reset, update, new) gives
  hidden_size = 2304 / 3 = **768**. So this is a `nn.GRU(input_size=768,
  hidden_size=768, num_layers=1, bidirectional=False)`. Note: PyTorch's
  `nn.GRU` exposes both `bias_ih_l0` and `bias_hh_l0` (CuDNN convention),
  even though only one bias is mathematically needed.
- `pos_embedding.weight` — `(20, 768)`: 20 position slots × 768 dims.
  Aligns with the `count_pred` output dim of 20 (one slot per discrete count
  bucket; max_count = 20, matching `count_pred.2` below).
- `projector` — 2-layer MLP `nn.Sequential` with indices `0` and `2`
  (`Linear → activation → Linear`): `1536 → 3072 → 768`. Input is likely
  `concat[span_rep_start, span_rep_end]` or `concat[span_rep, schema_rep]`
  (both are 1536-d).

### `count_pred` — count-prediction head

4 keys, ~1.2 M params. 2-layer MLP that predicts discrete counts in `[0, 19]`.

```
(1536,)	count_pred.0.bias
(1536, 768)	count_pred.0.weight
(20,)	count_pred.2.bias
(20, 1536)	count_pred.2.weight
```

`nn.Sequential` with indices `0` and `2` (`Linear → activation → Linear`):
`768 → 1536 → 20`. Output dim 20 is the max-count bucket count and matches
`count_embed.pos_embedding.weight`'s first dim.

### `classifier` — final span-class scorer

4 keys, ~1.18 M params. 2-layer MLP that produces a single logit per span.

```
(1536,)	classifier.0.bias
(1536, 768)	classifier.0.weight
(1,)	classifier.2.bias
(1, 1536)	classifier.2.weight
```

`nn.Sequential` with indices `0` and `2` (`Linear → activation → Linear`):
`768 → 1536 → 1`. Output dim 1 is consistent with a per-span / per-(span, label)
score (the "label" dimension comes from the schema/prompt side of the model
and is fed in as part of the 768-d input rather than as an extra output dim).

---

## Heads NOT present in the safetensors

The Phase 4 plan referenced these prefixes; for record, they are **absent**
from the checkpoint:

- `schema_gather.*` — no parametric schema/prompt encoder. Schema
  representations are presumably re-used from the same encoder applied to
  the prompt prefix (zero-shot GLiNER-style), then fed into `classifier`
  and `count_pred` without a dedicated module.
- `scorer.*` — the scoring head is named `classifier.*` here.

There are also **no unclassified keys** — every key sits cleanly under one of
the five top-level prefixes above (`encoder`, `span_rep`, `count_embed`,
`count_pred`, `classifier`).

---

## LSTM / GRU gate ordering

The recurrent module is `count_embed.gru.*`, **not an LSTM**. PyTorch's
`nn.GRU` packs gates in this order along axis 0 of every weight/bias tensor
(this matches the [official docs](https://docs.pytorch.org/docs/stable/generated/torch.nn.GRU.html)):

```
[ W_ir ;  W_iz ;  W_in ]   for weight_ih_l0
[ W_hr ;  W_hz ;  W_hn ]   for weight_hh_l0
[ b_ir ;  b_iz ;  b_in ]   for bias_ih_l0
[ b_hr ;  b_hz ;  b_hn ]   for bias_hh_l0
```

i.e. **(reset, update, new)** — *not* the cuDNN/Keras (z, r, n) ordering.
Gate width is hidden_size = 768, so each block is 768 rows and the total
first-axis dim is 3 × 768 = 2304, which matches the dumped shapes.

When porting to Candle: `candle_nn::rnn::GRU` uses the same PyTorch layout,
so the weights can be loaded directly with no permutation. The two biases
`bias_ih_l0` and `bias_hh_l0` should both be loaded — PyTorch applies them
as `b_ih + b_hh` inside each gate.

(For reference, had this been an LSTM, the PyTorch `nn.LSTM` ordering would
be **(input, forget, cell, output)** with first-axis dim 4 × hidden_size.)

---

## Concerns / things to double-check during the port

1. **`norm_rel_ebd = "layer_norm"` but no rel-embedding LayerNorm weights
   exist.** In stock HF DeBERTa-v2/v3, this option creates a `LayerNorm`
   applied to `rel_embeddings` before use; its weights would live under
   `encoder.encoder.LayerNorm.*`. We *do* have `encoder.encoder.LayerNorm.{weight,bias}`
   here — those are likely re-used as the rel-embeddings LayerNorm (HF's
   implementation aliases them). Verify this when porting: do not allocate a
   separate LayerNorm.

2. **`type_vocab_size = 0`** — no `token_type_embeddings`, so the embedding
   forward pass should skip token-type addition.

3. **`position_biased_input = false`** — no absolute position embeddings;
   only relative attention contributes positional information.

4. **`share_att_key = true`** — the Q and K projections are shared between
   content and relative-position attention; no separate `pos_q_proj` /
   `pos_k_proj` weights are needed (and indeed none are present).

5. **`counting_layer: "count_lstm"` in top-level config is misleading** —
   the actual module is a single-layer **GRU** (`count_embed.gru.*`).
