# Human-Voice Agent Interaction Dataset

## Why This Dataset Exists

Most NLP evaluation corpora come from **written text**: news articles, Wikipedia,
literary fiction. But conversational AI systems operate in **spoken dialogue**,
where entirely different phenomena emerge:

1. **Response tokens** like "uh huh", "oui", "d'accord" aren't questions—they're
   conversational grease that keeps dialogue flowing. But VAD-based systems treat
   any detected speech as requiring a response, causing interruptions.

2. **Aside sequences** where humans whisper or gesture to exclude the agent get
   picked up anyway, because "everything counts" to an always-listening system.

3. **Discourse deixis** where "this" and "that" refer to prior utterances, events,
   or propositions—not to noun phrases that standard coreference handles.

This dataset captures these phenomena from **real human-agent interactions**,
providing test cases that written-text corpora simply don't contain.

## Source

From Rudaz, Broth & Mlynář (2025) "Everything counts: the managed omnirelevance
of speech in human-voice agent interaction" (submitted to ACM TOCHI).

Ethnomethodological conversation analysis of:

1. **Pepper robot (2022)**: Rule-based chatbot in a museum setting
2. **ChatGPT "advanced voice mode" (2025)**: LLM-based voice agent on smartphone

Both systems use **silence-based turn-taking** (Voice Activity Detection):
any detected speech triggers a response, which is the root cause of the problems
documented in this data.

## Relevance to Anno

While the paper's core contribution (audio-based turn-taking analysis) is outside
anno's scope, the transcripts provide test cases for:

- **Abstract anaphora**: "This" referring to events/propositions, not entities
- **Discourse deixis**: Demonstratives pointing to prior discourse segments
- **Response tokens**: Classifying "oui", "d'accord" as continuers vs. answers
- **Multiparty coreference**: Speaker attribution and reference tracking in dialogue

## Annotations

Each example includes:

| Field | Description |
|-------|-------------|
| `id` | Unique identifier (excerpt_turn format) |
| `text` | The utterance text |
| `speaker` | Speaker ID (TOM, ANA, ROB, CED, GUI, ADR, VOA, EMM, GPT) |
| `speaker_type` | "human" or "agent" |
| `language` | "fr" (French) or "en" (English) |
| `is_aside` | Whether utterance was designed to exclude agent (whispered/gestured) |
| `is_response_token` | Whether this is a continuer like "uh huh", "okay" |
| `triggered_cutoff` | Whether this response token triggered agent interruption |
| `discourse_deixis` | List of demonstrative references with antecedents |
| `shell_nouns` | List of shell noun instances |
| `notes` | Relevant observations from the paper |

## Key Phenomena

### 1. Response Token Trouble (Excerpt 3)

The paper documents how normal conversational response tokens ("oui", "d'accord")
trigger unwanted interruptions from the voice agent:

```
EMM: → "oui"     [response token]
GPT: "je vois que le texte-"  [CUT OFF due to detecting EMM's speech]
GPT: "exactement!"  [assessment of EMM's "oui" instead of continuing]
```

### 2. Aside Sequences (Excerpts 1 & 2)

Humans produce whispered/gestured contributions to exclude the agent:

```
ANA: °tu n'es pas très intelligent°  [whispered critique of robot]
GUI: [gestures "trois"]              [mouthed number to avoid agent hearing]
```

### 3. Agent Misalignment (Excerpt 1)

After farewell, robot initiates new interaction:

```
TOM: "salut"      [farewell]
ROB: "bonjour"    [hello - wrong response, starts new interaction]
```

## Citation

```bibtex
@unpublished{rudaz2025omnirelevance,
  title={Everything counts: the managed omnirelevance of speech in 
         human-voice agent interaction},
  author={Rudaz, Damien and Broth, Mathias and Mlyn{\'a}{\v{r}}, Jakub},
  year={2025},
  note={Submitted to ACM TOCHI}
}
```

## Integration Status

This dataset is registered in `anno/src/eval/dataset_registry.rs` as `HumanVoiceAgentInteraction`
for metadata/discovery purposes, but is **NOT** in `anno/src/eval/loader.rs` for automatic
downloading because:

1. It's a local dataset (no public URL)
2. It's small enough to include in the repo
3. It requires specialized loading (not standard CoNLL/BIO format)

**To use this data:**

```rust
// Direct file loading (see anno/tests/human_voice_agent_dataset.rs)
let transcripts: Vec<TranscriptTurn> = load_jsonl("testdata/human_voice_agent/transcripts.jsonl")?;
let deixis: Vec<DiscourseDeixisExample> = load_jsonl("testdata/human_voice_agent/discourse_deixis.jsonl")?;
let tokens: Vec<ResponseToken> = load_jsonl("testdata/human_voice_agent/response_tokens.jsonl")?;
```

**Files:**

| File | Records | Purpose |
|------|---------|---------|
| `transcripts.jsonl` | 70 | Raw dialogue turns with metadata |
| `discourse_deixis.jsonl` | 10 | Abstract anaphora with character offsets |
| `response_tokens.jsonl` | 11 | Response token classification examples |

## License

This dataset is derived from published academic work for research purposes.
The transcription conventions follow Jefferson (2004) and Mondada (2018).
