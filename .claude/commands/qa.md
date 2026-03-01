# /qa -- Real-world quality audit of anno

Evaluate anno's extraction quality by running it on real web content and critiquing the results. This is not a pass/fail test -- it produces a written assessment of what anno does well and where it falls short.

## Execution strategy

- **Parallelize independent commands**: launch baselines, backend comparisons, format checks, and error-path tests concurrently where they don't share output files. ONNX backends share a model cache and don't conflict.
- **Capture exact output**: save command output to temp files (`> /tmp/anno-qa-baseline1.txt 2>&1`) so diffs against previous runs are reliable. Don't eyeball scrollback.
- **Time everything**: use `time` or note wall-clock for each command. Speed regressions matter as much as correctness regressions. See section 3h for expected ranges.
- **Stop early on build failure**: if `cargo build` fails, the entire QA is blocked. Report the build error and stop.
- **Read previous reports first**: step 7 requires comparison. Read them before running tests so you know what to watch for.

## Report convention

Reports go in `qa/reports/qa-YYYY-MM-DD.md` (gitignored). Append a `-suffix` when multiple reports exist for the same date (e.g., `qa-2026-03-01-post-fixes.md`).

## Procedure

### 1. Build anno (with eval + onnx)

```bash
cd <repo-root>
cargo build --release -p anno-cli --features "eval onnx"
```

If the build fails, stop and report. Expected build time: 20-40s (incremental), 2-5min (clean).

### 2. Check backends and prefetch models

```bash
./target/release/anno info
./target/release/anno models list
```

Note which backends are available. Then prefetch ONNX models so download time doesn't contaminate later timing observations:

```bash
./target/release/anno models download gliner bert-onnx nuner w2ner gliner2 --include-relation
```

At minimum `heuristic`, `pattern`, `crf`, `hmm`, and `stacked` (in non-ML mode) work without any downloads.

**Known issue**: w2ner model may 404 on HuggingFace. If download fails, note it and continue -- w2ner is not required for the core audit.

### 3. Run on diverse real content

Test breadth and depth. Vary content sources, input methods, backends, formats, and languages. Don't just test one URL -- test several, and don't just test `--url` -- test files, stdin, inline text, batch, and directories.

**Note wall-clock time** for each command. A QA run that takes 2 seconds vs 200 seconds per document is a different product.

#### 3a. Stable baseline content (test every run)

These provide a regression baseline. Test all of them, every time, so results are comparable across runs:

```bash
ANNO=./target/release/anno

# Baseline 1: known-hard inline text (lowercase names, ambiguous entities, nested orgs)
# Expected: stacked misses lowercase names (tim cook, apple inc., cologne)
# NuNER is the only backend that handles these -- test both
$ANNO extract -t "tim cook announced that apple inc. would acquire the german startup deepl, \
  founded by jaroslaw kutylowski in cologne. the deal, worth EUR 3.2 billion, \
  closes on march 15, 2026. cook told reuters he expects 500 new hires." \
  --model stacked --format inline

$ANNO extract -t "tim cook announced that apple inc. would acquire the german startup deepl, \
  founded by jaroslaw kutylowski in cologne. the deal, worth EUR 3.2 billion, \
  closes on march 15, 2026. cook told reuters he expects 500 new hires." \
  --model nuner --format inline

# Baseline 2: structured data (dates, money, emails, phones, URLs)
$ANNO extract -t "Contact: jane.doe@example.com or +1-555-867-5309. \
  Invoice #2024-0042 dated 2025-12-31 for \$14,999.00 USD. \
  Visit https://example.com/payments or mail 1600 Pennsylvania Ave NW, Washington, DC 20500." \
  --model pattern --format json

# Baseline 3: multilingual (one sentence each)
# Watch for BERT span truncation: "Angela Merk" (missing "el"), "eskanzler" (missing "Bund")
$ANNO extract -t "Le president Emmanuel Macron a rencontre Angela Merkel a Berlin." --format inline
$ANNO extract -t "Bundeskanzler Olaf Scholz besuchte das Rote Kreuz in Muenchen." --format inline
$ANNO extract -t "Kishida Fumio wa Tokyo de kaigi o hiraita." --format inline

# Baseline 4: privacy/PII detection (SSN, credit card, IBAN, address)
$ANNO privacy -t "Patient SSN: 123-45-6789. Card: 4111-1111-1111-1111. IBAN: DE89370400440532013000."
$ANNO privacy -t "Ship to 1234 Elm Street, Springfield, IL 62704. Email: jane@example.com. Phone: 555-0100."

# Baseline 5: title-word entity detection (CEO, President patterns)
$ANNO extract -t "Apple CEO Tim Cook met Google CEO Sundar Pichai in Seattle." --model stacked --format inline
```

Record exact output for each baseline. When comparing against previous runs, diff these first.

#### 3b. Rotating exploration content (pick at least 5, vary each run)

- A live news page (try `--url`, or curl + pipe stdin)
- A Wikipedia article (dense entities, long text)
- A government/legal page (formal language, addresses, monetary terms)
- A Hacker News or Reddit thread (informal, usernames, abbreviations)
- A non-English page (Japanese, German, Arabic, French -- test each separately)
- A technical blog post or changelog (product names, version numbers)
- A financial document (SEC filing, earnings report)

#### 3c. Input method diversity (test all of these)

```bash
# --url (live web fetch; requires eval feature, included in build step)
$ANNO extract --url <url> --model stacked --format inline

# --file (from disk)
curl -sL <url> > /tmp/anno-test.txt
$ANNO extract --file /tmp/anno-test.txt --model stacked --format inline

# -t (inline text)
$ANNO extract -t "Some text here." --format inline

# positional (shorthand for extract -t)
$ANNO "Some text here."

# stdin pipe
echo "Some text here." | $ANNO extract --format inline

# batch --dir (multiple files -- populate the dir first)
mkdir -p /tmp/anno-test-dir/
echo "Apple CEO Tim Cook met with Satya Nadella in Seattle." > /tmp/anno-test-dir/tech.txt
echo "The ECB raised rates by 25 basis points on January 15, 2026." > /tmp/anno-test-dir/finance.txt
echo "Researchers at MIT published findings in Nature on March 1." > /tmp/anno-test-dir/science.txt
$ANNO batch --dir /tmp/anno-test-dir/ --model stacked --format json --output /tmp/anno-out/

# batch --stdin (JSONL)
echo '{"id":"1","text":"Marie Curie won the Nobel Prize in 1903."}
{"id":"2","text":"Contact support@example.com for help."}' \
  | $ANNO batch --stdin --model stacked --format json
```

#### 3d. Backend diversity (test at least 6)

```bash
# Always-available (no downloads)
--model stacked        # default: pattern + heuristic + ML when available
--model pattern        # structured extraction only (dates, money, emails, etc.)
--model heuristic      # capitalization + context heuristics
--model crf            # CRF sequence labeler
--model hmm            # HMM sequence labeler
--model ensemble       # weighted voting across multiple backends
--model bilstm-crf     # BiLSTM + CRF neural baseline
--model minimal        # minimal heuristic (lowest complexity)

# Require ONNX feature + model download
--model bert-onnx      # BERT transformer NER (PER/ORG/LOC/MISC)
--model nuner          # zero-shot; handles lowercase text well (~2s per sentence)
--model gliner --extract-types "person,organization,location,date,money,product"  # zero-shot; full words recommended (abbreviations like PER also work after label expansion)
--model gliner2 --extract-types "PER,ORG,LOC"   # multitask (NER + relations)
--model w2ner          # nested entity recognition (model may not be available)
--model tplinker       # joint entity-relation extraction (heuristic)
```

GLiNER and NuNER are zero-shot backends: they accept arbitrary type labels via `--extract-types`. GLiNER produces empty output without `--extract-types` (it has no default label set). NuNER has defaults but benefits from explicit types.

**Cross-backend comparison**: run the same sentence through multiple backends to check agreement:

```bash
for model in stacked bert-onnx nuner gliner ensemble; do
  echo "=== $model ==="
  $ANNO extract -t "Apple CEO Tim Cook met Google CEO Sundar Pichai in Seattle." \
    --model $model --format inline $([ "$model" = "gliner" ] && echo "--extract-types person,organization,location")
done
```

#### 3e. Output format diversity (test at least 4 formats)

```bash
# All formats for extract:
--format human     # colored hierarchical text (the default -- test this!)
--format json      # single JSON object with provenance + entities
--format jsonl     # streaming: provenance line, then one entity per line
--format tsv       # tab-separated (entity, type, start, end, confidence, surface)
--format inline    # [TYPE: entity_text] inline in source text
--format grounded  # full GroundedDocument JSON (pipeline integration format)
```

Check: does `--format json` parse cleanly (`| python3 -m json.tool`)? Does `--format tsv` have correct column count? Does `--format human` suppress ANSI when piped to a file (`| cat -v` -- should show no `^[` escapes)?

#### 3f. Subcommand diversity (test beyond just `extract`)

Test high-value subcommands first, then the rest:

**Tier 1 (always test)**:

```bash
# Privacy/PII detection and redaction
$ANNO privacy -t "John Smith's SSN is 123-45-6789. Email: john@example.com. Phone: 555-0100."
$ANNO privacy -t "John Smith's SSN is 123-45-6789." --action redact
$ANNO privacy -t "John Smith's SSN is 123-45-6789." --action pseudonymize

# Coreference
$ANNO debug --coref -t "Marie Curie discovered radium. She won the Nobel Prize. Curie later moved to Paris." --model stacked

# Relation extraction (only tplinker and gliner2 produce typed relations)
$ANNO extract -t "Tim Cook is the CEO of Apple Inc., headquartered in Cupertino." \
  --model tplinker --extract-relations --format json

# Export to annotation formats
$ANNO extract -t "Apple CEO Tim Cook met Google CEO Sundar Pichai." --format grounded > /tmp/anno-gd.json
$ANNO export -i /tmp/anno-gd.json -o /tmp/anno-export/ --format brat
$ANNO export -i /tmp/anno-gd.json -o /tmp/anno-export/ --format conll --overwrite
```

**Tier 2 (test when time allows)**:

```bash
# Joint NER + coreference + linking
$ANNO joint -t "Elon Musk founded SpaceX. He also leads Tesla. Musk tweeted about Mars." --model stacked

# Domain shift detection
$ANNO domain -t "The patient presented with acute myocardial infarction." --model stacked

# Explain (feature attribution for a specific entity)
$ANNO explain -t "Barack Obama visited the White House." --model stacked

# Query (filter entities from GroundedDocument)
$ANNO query /tmp/anno-gd.json --type PER
$ANNO query /tmp/anno-gd.json --min-confidence 0.8

# Enhance (add coreference to existing GroundedDocument)
$ANNO enhance /tmp/anno-gd.json --coref --format json

# Batch processing
$ANNO batch --dir /tmp/anno-test-dir/ --model stacked --format json --output /tmp/anno-out/

# Compare backends on same input
$ANNO compare -t "Angela Merkel met Emmanuel Macron in Berlin." --models --model-list "stacked,bert-onnx,gliner"

# Singleton, analyze, pipeline, watch (if time permits)
```

#### 3g. Error-path testing

Test what happens when things go wrong. These should produce clear errors, not silent failures or panics:

```bash
# Empty input
$ANNO extract -t ""
echo "" | $ANNO extract

# Binary / non-text input
$ANNO extract --file /bin/ls

# Nonexistent file
$ANNO extract --file /tmp/does-not-exist.txt

# Invalid backend
$ANNO extract -t "test" --model nonexistent-backend

# Invalid extract-types (empty, malformed)
$ANNO extract -t "test" --model gliner --extract-types ""

# URL that 404s or times out
$ANNO extract --url "https://httpstat.us/404"

# Malformed JSONL for batch
echo '{"bad json' | $ANNO batch --stdin --model stacked --format json

# Enormous single line (stress test -- expect 5-30s for stacked)
python3 -c "print('Angela Merkel ' * 10000)" | $ANNO extract --model stacked --format inline
```

Read the full output of each command. Do not truncate or pipe through head/tail.

#### 3h. Timing reference (expected ranges)

Compare wall-clock times against these baselines (single sentence, macOS, M-series):

| Backend | Expected | Red flag |
|---------|----------|----------|
| pattern | <20ms | >50ms |
| heuristic | <20ms | >50ms |
| crf | <20ms | >50ms |
| hmm | <10ms | >50ms |
| stacked | 300-500ms | >800ms |
| bert-onnx | 300-400ms | >800ms |
| gliner | 350-450ms | >1s |
| tplinker | 400-500ms | >1s |
| nuner | 1.5-2.5s | >5s |
| ensemble | 350-450ms | >1s |

Significant deviations suggest model loading issues, ONNX session creation overhead, or a regression.

### 4. Critique the output

For each piece of content, check:

**Span correctness** (use `--format inline` -- truncated spans are immediately visible)
- Are entity boundaries correct? Look for names cut mid-word ("Lagard" instead of "Lagarde").
- Are multi-word names captured fully? ("Dr. Jennifer Doudna" not just "Jennifer")
- Known recurring bug: BPE-to-char alignment can truncate 1-3 trailing characters on ONNX backends.

**Type accuracy**
- PER vs ORG vs LOC assignments correct?
- Month names tagged as LOC? Weekdays as PER? Common misclassification patterns?
- Capitalized common words falsely tagged? ("Phone" as PER, "Chemistry" as LOC)

**Recall**
- What obvious entities were missed entirely?
- Two-word person names? Organization acronyms? Monetary amounts?
- Lowercase names (test with NuNER -- stacked systematically misses these)?

**Precision**
- False positives? Navigation text tagged as entities? Common nouns tagged as proper?
- Fiscal quarters (Q1-Q4) should NOT be entities.
- Invoice numbers (#2024-0042) should NOT be hashtags.

**Structured extraction (pattern backend)**
- Dates: ISO (2025-12-31), US (March 15, 2026), EU (15. Januar 2024), month-year (April 2018)?
- Money: USD ($14,999.00), EUR (EUR 3.2 billion)?
- Contact: emails, phone numbers (+1-555-867-5309)?
- URLs correctly extracted?
- European decimal commas (EUR 3,2 Mrd.) handled?
- Trailing whitespace in spans?

**Privacy/PII detection**
- SSN (123-45-6789) detected as ID_NUMBER?
- Credit card (4111-1111-1111-1111) detected?
- IBAN (DE89370400440532013000) detected?
- Addresses with ZIP + state abbreviation detected?
- Redact and pseudonymize produce clean output?
- Watch for false positives: "Phone", "Chemistry" etc.

**Coreference** (debug --coref and joint)
- Pronoun chains correct? "She" -> right antecedent?
- Name variants grouped? "Curie" + "Marie Curie" = same cluster?
- Spurious merges? Distinct people collapsed into one cluster?

**Relations** (tplinker and gliner2 only)
- Are extracted relations semantically correct?
- Bare "in" / "on" should NOT trigger LOCATED_IN / OCCURRED_ON with semantically incompatible types.
- Does `--model stacked --extract-relations` silently produce zero relations? (Expected -- only tplinker/gliner2 emit typed relations.)

**URL ingestion quality**
- Did `--url` fetch clean text or include nav/sidebar/footer junk?
- How much noise vs signal in the extracted text?

**Output format correctness**
- Does `--format json` produce valid, parseable JSON?
- Does `--format tsv` have consistent column counts across rows?
- Does `--format human` suppress ANSI when piped to a file?
- Does `--format grounded` round-trip through `query` and `enhance`?

**Export format correctness**
- Do brat `.ann` files have valid T-annotation format?
- Does CoNLL output have correct BIO tags, one token per line?
- Does JSON-LD have valid `@context`?

**Error behavior**
- Do bad inputs produce clear error messages on stderr?
- Do any inputs cause panics or hangs?
- Are exit codes nonzero for failures?

**Cross-backend comparison**
- Where do backends agree/disagree?
- Which finds more entities? Which has fewer false positives?
- Does NuNER outperform stacked on lowercase text?
- Does w2ner find nested entities that others miss?

### 5. Write the report

Save to `qa/reports/qa-YYYY-MM-DD.md`. Produce a structured critique covering:

1. **Test conditions**: date, commit SHA, URLs tested, backends used, anno version (`anno info`), wall-clock timings
2. **Baseline results**: exact output for each stable baseline (section 3a), diffed against previous run if available
3. **Per-source findings**: what worked, what failed, with specific entity spans quoted
4. **Bug table**: concrete bugs found (span truncation, misclassification, etc.) with reproduction commands
5. **Subcommand coverage**: which subcommands worked, which errored, which produced surprising output
6. **Format audit**: any format that produced invalid/malformed output
7. **Error handling**: which error-path tests produced good messages vs bad ones
8. **Overall assessment**: strengths, weaknesses, surprises
9. **Actionable issues**: specific things worth fixing, ordered by impact

Be concrete. Quote entity spans, show expected vs actual types, include the command that reproduces each issue.

### 6. Regression check on known bugs

Check whether these previously-identified issues are still present. Update this checklist in the QA command itself (not just the report) when bugs are confirmed fixed or new ones found.

#### Fixed (verified 2026-03-01)

- [x] ~~Stacked drops PER after title words~~: "CEO Tim Cook" now detected correctly
- [x] ~~GLiNER zero output with short type labels~~: "PER,ORG,LOC" now works via label expansion
- [x] ~~Export command broken on GroundedDocument~~: brat and conll produce valid output
- [x] ~~Hashtag false positive on invoice numbers~~: "#2024-0042" no longer triggers Hashtag
- [x] ~~SSN/credit card/IBAN not detected by privacy~~: pre-NER scan catches structured PII
- [x] ~~Q1-Q4 tagged as PER~~: fiscal quarter filter in heuristic backend
- [x] ~~CRF spans cross sentence boundaries~~: sentence boundary enforcement added
- [x] ~~HTML hex entities (&#x27;) not decoded~~: hex path added to entity decoder
- [x] ~~Bare "in"/"on" triggers false LOCATED_IN relations~~: entity-type compatibility guard added
- [x] ~~Month-year dates not recognized~~: "April 2018" and "Oktober 2024" now extracted as DATE
- [x] ~~"Chemistry" flagged as ID number~~: MRN heuristic now requires at least one digit
- [x] ~~BERT span truncation~~: word-boundary healing in onnx.rs finalize_entity extends entities to enclosing word boundaries. "Angela Merkel" and "Bundeskanzler" now correct.
- [x] ~~Coreference spurious merges~~: raised link_threshold to 0.45, halved substring weight to 0.15, added pronoun-specific threshold (0.5x), required multi-word or >4 char proper nouns for global merge.
- [x] ~~Joint produces 0 coref chains~~: added exact-string-match + last-name fallback when BP produces only singletons. "Elon Musk" + "Musk" now cluster.
- [x] ~~European decimal comma~~: MONEY patterns accept comma decimal (EUR 3,2 Mrd, €3,50). Added Mrd/Mio/Bn/Mn magnitude abbreviations.
- [x] ~~German word-boundary slicing~~: same fix as BERT span truncation (word-boundary healing in onnx.rs).

#### Open (as of 2026-03-01)

- [ ] Stacked misses lowercase names: "tim cook", "apple inc." unrecognized without NuNER. NuNER is the only backend that handles these but is ~5x slower.
- [ ] Enhance coref nonfunctional: creates 1 track but `signal_to_track` is empty.
- [ ] URL ingestion noise: `--url` on news sites returns >90% nav/chrome junk as entity candidates.
- [ ] bert-onnx fragmentation on long text: produces nonsense spans ("Lin", "Tor") on multi-paragraph input.
- [ ] "Phone" tagged as PERSON by privacy: capitalized common word caught by heuristic backend.
- [ ] IBAN double detection: pre-NER scan and NER both fire, producing redundant entries.
- [ ] info display: ONNX backends show as available without "(requires model download)" note.

### 7. Compare against previous runs

Read previous reports from `qa/reports/`. For each:
- Are baseline results (section 3a) identical, improved, or regressed?
- Are bugs from section 6 still present?
- Any new bugs not seen before?
- Any previous bugs now fixed (update the checklist in THIS file, section 6)?
- Any timing regressions?

## What this is NOT

- Not a benchmark against gold labels (that's `just ci-eval` / `just eval-sanity`)
- Not a unit test suite (that's `just test`)
- Not a property test (that's `just proptest`)

This answers: "if someone ran anno on content they care about, would the output be useful?"
