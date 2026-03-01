# /qa -- Real-world quality audit of anno

Evaluate anno's extraction quality by running it on real web content and critiquing the results. This is not a pass/fail test -- it produces a written assessment of what anno does well and where it falls short.

## Report convention

Reports go in `qa/reports/qa-YYYY-MM-DD.md`. Read any existing reports before starting -- step 7 requires comparing against them.

## Procedure

### 1. Build anno (with eval + onnx)

```bash
cd <repo-root>
cargo build --release -p anno-cli --features "eval onnx"
```

If the build fails, stop and report.

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

### 3. Run on diverse real content

Test breadth and depth. Vary content sources, input methods, backends, formats, and languages. Don't just test one URL -- test several, and don't just test `--url` -- test files, stdin, inline text, batch, and directories.

**Note wall-clock time** for each command. A QA run that takes 2 seconds vs 200 seconds per document is a different product.

#### 3a. Stable baseline content (test every run)

These provide a regression baseline. Test all of them, every time, so results are comparable across runs:

```bash
ANNO=./target/release/anno

# Baseline 1: known-hard inline text (lowercase names, ambiguous entities, nested orgs)
$ANNO extract -t "tim cook announced that apple inc. would acquire the german startup deepl, \
  founded by jaroslaw kutylowski in cologne. the deal, worth EUR 3.2 billion, \
  closes on march 15, 2026. cook told reuters he expects 500 new hires." \
  --model stacked --format inline

# Baseline 2: structured data (dates, money, emails, phones, URLs)
$ANNO extract -t "Contact: jane.doe@example.com or +1-555-867-5309. \
  Invoice #2024-0042 dated 2025-12-31 for \$14,999.00 USD. \
  Visit https://example.com/payments or mail 1600 Pennsylvania Ave NW, Washington, DC 20500." \
  --model pattern --format json

# Baseline 3: multilingual (one sentence each)
$ANNO extract -t "Le president Emmanuel Macron a rencontre Angela Merkel a Berlin." --format inline
$ANNO extract -t "Bundeskanzler Olaf Scholz besuchte das Rote Kreuz in Muenchen." --format inline
$ANNO extract -t "Kishida Fumio wa Tokyo de kaigi o hiraita." --format inline
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
--model nuner          # zero-shot; handles lowercase text well
--model gliner --extract-types "person,organization,location,date,money,product"  # zero-shot; use full words, not abbreviations (PER/ORG/LOC produce empty output)
--model gliner2 --extract-types "PER,ORG,LOC"   # multitask (NER + relations)
--model w2ner          # nested entity recognition
--model tplinker       # joint entity-relation extraction (heuristic)
```

GLiNER and NuNER are zero-shot backends: they accept arbitrary type labels via `--extract-types`. GLiNER produces empty output without `--extract-types` (it has no default label set). NuNER has defaults but benefits from explicit types.

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

Check: does `--format json` parse cleanly? Does `--format tsv` have correct column count? Does `--format human` degrade gracefully on non-TTY (no ANSI)?

#### 3f. Subcommand diversity (test beyond just `extract`)

```bash
# Coreference
$ANNO debug --coref -t "Marie Curie discovered radium. She won the Nobel Prize. Curie later moved to Paris." --model stacked

# Relation extraction (only tplinker and gliner2 produce typed relations)
$ANNO extract -t "Tim Cook is the CEO of Apple Inc., headquartered in Cupertino." \
  --model tplinker --extract-relations --format json
$ANNO extract -t "Tim Cook is the CEO of Apple Inc., headquartered in Cupertino." \
  --model gliner2 --extract-types "PER,ORG,LOC" --extract-relations --format json

# Privacy/PII detection and redaction
$ANNO privacy -t "John Smith's SSN is 123-45-6789. Email: john@example.com. Phone: 555-0100."
$ANNO privacy -t "John Smith's SSN is 123-45-6789." --action redact
$ANNO privacy -t "John Smith's SSN is 123-45-6789." --action pseudonymize

# Joint NER + coreference + linking
$ANNO joint -t "Elon Musk founded SpaceX. He also leads Tesla. Musk tweeted about Mars." --model stacked

# Singleton analysis (entities with no coreference links)
$ANNO singleton -t "Apple announced earnings. Google released a new phone. Tim Cook spoke." --model stacked

# Domain shift detection
$ANNO domain -t "The patient presented with acute myocardial infarction." --model stacked

# Explain (feature attribution for a specific entity)
$ANNO explain -t "Barack Obama visited the White House." --model stacked

# Multi-backend analysis (hidden command, still functional)
$ANNO analyze -t "The European Central Bank raised interest rates."

# Compare backends on same input
$ANNO compare -t "Angela Merkel met Emmanuel Macron in Berlin." --models --model-list "stacked,bert-onnx,gliner"

# Pipeline (unified multi-doc processing)
echo "Doc one about Apple." > /tmp/anno-pipe-1.txt
echo "Doc two about Google." > /tmp/anno-pipe-2.txt
$ANNO pipeline --files /tmp/anno-pipe-1.txt,/tmp/anno-pipe-2.txt --model stacked --format json

# Query (filter entities from GroundedDocument)
$ANNO extract -t "Apple CEO Tim Cook met Google CEO Sundar Pichai." --format grounded > /tmp/anno-gd.json
$ANNO query /tmp/anno-gd.json --type PER
$ANNO query /tmp/anno-gd.json --min-confidence 0.8

# Enhance (add coreference to existing GroundedDocument)
$ANNO enhance /tmp/anno-gd.json --coref --format json

# Export to annotation formats
$ANNO export -i /tmp/anno-gd.json -o /tmp/anno-export/ --format brat
$ANNO export -i /tmp/anno-gd.json -o /tmp/anno-export/ --format conll
$ANNO export -i /tmp/anno-gd.json -o /tmp/anno-export/ --format jsonld

# Context (entities with surrounding text)
$ANNO extract -t "Apple CEO Tim Cook met Google CEO Sundar Pichai in Seattle." --format grounded > /tmp/anno-ctx.json
# (context command if available, or use query with context flags)

# Watch (directory watcher -- start, add a file, observe processing, then Ctrl-C)
$ANNO watch /tmp/anno-watch-dir/ --output /tmp/anno-watch-out/ --model stacked --initial &
WATCH_PID=$!
sleep 2
echo "New document about Amazon and Jeff Bezos." > /tmp/anno-watch-dir/new.txt
sleep 5
kill $WATCH_PID
cat /tmp/anno-watch-out/*.json 2>/dev/null
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

# Enormous single line (stress test)
python3 -c "print('Angela Merkel ' * 10000)" | $ANNO extract --model stacked --format inline
```

Read the full output of each command. Do not truncate or pipe through head/tail.

### 4. Critique the output

For each piece of content, check:

**Span correctness** (use `--format inline` -- truncated spans are immediately visible)
- Are entity boundaries correct? Look for names cut mid-word ("Lagard" instead of "Lagarde").
- Are multi-word names captured fully? ("Dr. Jennifer Doudna" not just "Jennifer")
- Known recurring bug: BPE-to-char alignment can truncate 1-3 trailing characters.

**Type accuracy**
- PER vs ORG vs LOC assignments correct?
- Month names tagged as LOC? Weekdays as PER? Common misclassification patterns?

**Recall**
- What obvious entities were missed entirely?
- Two-word person names? Organization acronyms? Monetary amounts?
- Lowercase names (test with NuNER -- stacked often misses these)?

**Precision**
- False positives? Navigation text tagged as entities? Common nouns tagged as proper?

**Structured extraction**
- Dates, monetary amounts, emails, URLs -- correctly parsed?
- European decimal commas (EUR 3,2 Mrd.) handled?
- Trailing whitespace in spans?

**Coreference** (debug --coref and joint)
- Pronoun chains correct? "She" -> right antecedent?
- Name variants grouped? "Curie" + "Marie Curie" = same cluster?
- Spurious merges? Distinct people collapsed into one cluster?

**Relations** (tplinker and gliner2 only)
- Are extracted relations semantically correct?
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
- Do brat `.ann` files validate against brat conventions?
- Does CoNLL output have correct IOB tags?
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

1. **Test conditions**: date, URLs tested, backends used, anno version (`anno info`), wall-clock timings
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

Check whether these previously-identified issues are still present (update this list as bugs are fixed or new ones found):

- [ ] Span truncation: BPE-to-char alignment cutting 1-3 trailing chars ("Furukawa" -> "Furuk", "Merkel" -> "Merk", "Charpentier" -> "Charpent")
- [ ] Stacked drops PER after title words: "CEO Tim Cook" -> Tim Cook missed; bert-onnx and NuNER find it
- [ ] GLiNER zero output with short type labels: `--extract-types "PER,ORG,LOC"` -> 0 entities; "PERSON,ORGANIZATION,LOCATION" works
- [ ] Export command broken on GroundedDocument: `anno export -i grounded.json` re-runs extraction instead of reading signals
- [ ] URL ingestion noise: `--url` on news sites returns >90% nav/chrome junk
- [ ] Coreference spurious merges: distinct people collapsed into one cluster
- [ ] Enhance coref creates nonsensical clusters: merges unrelated proper nouns into one track
- [ ] Stacked misses lowercase names: "tim cook" unrecognized without NuNER
- [ ] bert-onnx fragmentation on long text: produces nonsense spans ("Lin", "Tor")
- [ ] European decimal comma parsing: "EUR 3,2 Mrd." not recognized as MONEY
- [ ] German word-boundary slicing: "Bundeskanzler" -> "eskanzler" (first chars dropped)
- [ ] Hashtag false positive on invoice numbers: "#2024-0042" triggers "#2024" as Hashtag
- [ ] Joint produces 0 coref chains despite clear pronouns

### 7. Compare against previous runs

Read previous reports from `qa/reports/`. For each:
- Are baseline results (section 3a) identical, improved, or regressed?
- Are bugs from section 6 still present?
- Any new bugs not seen before?
- Any previous bugs now fixed (update the checklist)?

## What this is NOT

- Not a benchmark against gold labels (that's `just ci-eval` / `just eval-sanity`)
- Not a unit test suite (that's `just test`)
- Not a property test (that's `just proptest`)

This answers: "if someone ran anno on content they care about, would the output be useful?"
