// Generated loader stubs from datasets_generated.json
// Registry datasets: 451
// JSON datasets: 451
// Missing variants: 0
//
// The registry (anno/src/eval/dataset_registry.rs) and generated JSON
// (datasets_generated.json) are now fully synchronized.
//
// To regenerate:
//   cargo test -p anno --features "eval" generate_datasets_json -- --ignored
//
// To check for mismatches:
//   python3 -c "import json; exec('''
// with open('datasets_generated.json') as f: gen = json.load(f)
// import re
// with open('anno/src/eval/dataset_registry.rs') as f: registry = f.read()
// existing = {m.group(1) for m in re.finditer(r'^    ([A-Z][A-Za-z0-9_]*) \\\\{$', registry, re.MULTILINE)}
// json_ids = {d['id'] for d in gen['datasets']}
// print('JSON not in registry:', json_ids - existing)
// print('Registry not in JSON:', existing - json_ids)
// ''')"
