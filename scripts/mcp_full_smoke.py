import json
import os
import pathlib
import queue
import subprocess
import sys
import tempfile
import threading
import time


REQUEST_TIMEOUT_SECONDS = 180


CRITICAL_CONTEXT_TOOLS = {
    "index",
    "status",
    "search",
    "legal_search",
    "knowledge_search",
    "corpus_list",
    "forget",
}


def enqueue_stdout(proc, out_queue):
    for line in proc.stdout:
        out_queue.put(line)


def write_message(proc, payload):
    line = json.dumps(payload, separators=(",", ":")) + "\n"
    proc.stdin.write(line)
    proc.stdin.flush()


def send(proc, out_queue, msg_id, method, params=None, timeout=REQUEST_TIMEOUT_SECONDS):
    payload = {"jsonrpc": "2.0", "id": msg_id, "method": method}
    if params is not None:
        payload["params"] = params
    write_message(proc, payload)

    deadline = time.monotonic() + timeout
    while True:
        remaining = deadline - time.monotonic()
        if remaining <= 0:
            raise TimeoutError(f"timed out waiting for {method}")
        if proc.poll() is not None and out_queue.empty():
            raise RuntimeError(f"process exited while waiting for {method}")
        try:
            raw = out_queue.get(timeout=min(remaining, 1.0))
        except queue.Empty:
            continue
        raw = raw.strip()
        if not raw:
            continue
        try:
            data = json.loads(raw)
        except json.JSONDecodeError:
            continue
        if data.get("id") == msg_id:
            return data


def notify(proc, method, params=None):
    payload = {"jsonrpc": "2.0", "method": method}
    if params is not None:
        payload["params"] = params
    write_message(proc, payload)


def call_tool(proc, out_queue, msg_id, name, arguments=None):
    return send(
        proc,
        out_queue,
        msg_id,
        "tools/call",
        {
            "name": name,
            "arguments": arguments or {},
        },
    )


def tool_text(response):
    result = response.get("result", {})
    content = result.get("content", [])
    if not content:
        return ""
    first = content[0]
    if isinstance(first, dict):
        return first.get("text", "")
    return ""


def parse_json_text(response):
    text = tool_text(response)
    if not text:
        return {}
    try:
        return json.loads(text)
    except json.JSONDecodeError:
        return {"raw": text}


def write_fixture(root):
    root.mkdir(parents=True, exist_ok=True)
    (root / "client-note.txt").write_text(
        "Client ACME. Donnees brutes client. Contact jean@example.com.",
        encoding="utf-8",
    )
    (root / "contract.txt").write_text(
        "Contrat de prestation. Paiement sous 30 jours. Clause de confidentialite.",
        encoding="utf-8",
    )
    anon = root / "anon"
    anon.mkdir()
    (anon / "generated.anon.md").write_text(
        "generated output must be ignored",
        encoding="utf-8",
    )


def first_uuid_like(value, keys):
    if isinstance(value, dict):
        for key in keys:
            item = value.get(key)
            if isinstance(item, str) and len(item) >= 32:
                return item
        for item in value.values():
            found = first_uuid_like(item, keys)
            if found:
                return found
    if isinstance(value, list):
        for item in value:
            found = first_uuid_like(item, keys)
            if found:
                return found
    return None


def first_corpus_id(value):
    if not isinstance(value, dict):
        return None
    for key in ("corpus_id", "selected_corpus_id"):
        item = value.get(key)
        if isinstance(item, str):
            return item
    for list_key in ("corpora", "items", "sources"):
        rows = value.get(list_key)
        if isinstance(rows, list):
            for row in rows:
                found = first_corpus_id(row)
                if found:
                    return found
    return None


def first_hit_id(value, key):
    if isinstance(value, dict):
        if isinstance(value.get(key), str):
            return value[key]
        for child in value.values():
            found = first_hit_id(child, key)
            if found:
                return found
    if isinstance(value, list):
        for child in value:
            found = first_hit_id(child, key)
            if found:
                return found
    return None


def smoke_call_plan(fixture, state):
    fake_uuid = "00000000-0000-0000-0000-000000000000"
    corpus_id = state.get("corpus_id")
    source_id = state.get("source_id") or fake_uuid
    review_id = state.get("review_id") or fake_uuid
    doc_id = state.get("doc_id") or fake_uuid
    chunk_id = state.get("chunk_id") or fake_uuid
    row_id = state.get("row_id") or fake_uuid
    col_id = state.get("col_id") or fake_uuid

    corpus_args = {"corpus_id": corpus_id or fake_uuid}
    return [
        ("anno_health", {}),
        ("status", {}),
        ("anno_init_vault", {"passphrase": "anno-local-smoke-passphrase"}),
        ("download_models", {}),
        ("index", {"path": str(fixture), "profile": "all"}),
        ("corpus_list", {}),
        ("corpus_get", corpus_args),
        ("corpus_health", corpus_args),
        ("sources", {}),
        ("knowledge_status", {}),
        ("knowledge_sources", {}),
        ("knowledge_add_local_folder", {"path": str(fixture)}),
        ("knowledge_sync", {}),
        (
            "knowledge_search",
            {
                "query": "donnees brutes client",
                "mode": "fast",
                "allow_cross_corpus": True,
            },
        ),
        ("knowledge_forget", {"source_id": source_id}),
        ("legal_ingest", {"folder": str(fixture), "recursive": True}),
        (
            "legal_search",
            {
                "query": "paiement confidentialite",
                "allow_cross_corpus": True,
            },
        ),
        (
            "legal_graph_query",
            {"intent": "party_dossier", "party": "org:acme"},
        ),
        (
            "legal_rehydrate_citation",
            {"chunk_id": chunk_id, "byte_start": 0, "byte_end": 8},
        ),
        ("legal_extract_contract", {"doc_id": doc_id}),
        ("legal_extract_case_file", {"dossier_id": "smoke-dossier"}),
        ("legal_timeline", {"dossier_id": "smoke-dossier"}),
        ("legal_risk_review", {"scope_id": doc_id, "is_dossier": False}),
        (
            "legal_mandatory_clause_audit",
            {"doc_id": doc_id, "doc_type": "b2b_contract"},
        ),
        (
            "legal_prescription_check",
            {
                "category": "contractuel",
                "event_date": "2020-01-15T00:00:00Z",
                "interrupting_events": [],
            },
        ),
        (
            "legal_validate_field",
            {
                "chunk_id": chunk_id,
                "field_name": "obligation:paiement",
                "action": "confirm",
                "corrected_value": None,
                "note": "smoke",
                "actor": "anno-smoke",
            },
        ),
        (
            "search",
            {
                "query": "paiement confidentialite",
                "scope": "legal",
                "allow_cross_corpus": True,
            },
        ),
        (
            "search",
            {
                "query": "client contrat",
                "scope": "all",
                "allow_cross_corpus": True,
            },
        ),
        ("legacy_search", {"query": "client contrat", "top_k": 3}),
        ("detect", {"text": "Contact jean@example.com pour ACME."}),
        ("rehydrate", {"text": "EMAIL_1 PERSON_1"}),
        ("memory_save", {"text": "Preference client: reponse concise", "kind": "preference"}),
        ("memory_list", {}),
        ("memory_recall", {"query": "preference client"}),
        ("memory_graph_recall", {"entity": "client"}),
        ("memory_invalidate", {"id": fake_uuid}),
        ("memory_forget", {"query": "preference client", "limit": 1}),
        ("review_create", {"name": "Smoke review", "corpus_id": corpus_id}),
        ("review_add_rows", {"review_id": review_id, "doc_ids": [], "force_reextract": False}),
        ("review_extract", {"review_id": review_id, "force_reextract": False}),
        (
            "review_refine_cell",
            {
                "review_id": review_id,
                "row_id": row_id,
                "col_id": col_id,
                "instruction": "smoke",
            },
        ),
        (
            "review_set_cell",
            {
                "review_id": review_id,
                "row_id": row_id,
                "col_id": col_id,
                "value": "smoke",
                "lock": False,
            },
        ),
        ("review_lock_cell", {"review_id": review_id, "row_id": row_id, "col_id": col_id}),
        ("review_unlock_cell", {"review_id": review_id, "row_id": row_id, "col_id": col_id}),
        ("review_export", {"review_id": review_id, "format": "csv"}),
        ("review_get", {"review_id": review_id}),
        ("vault_stats", {}),
        ("forget", {"target": str(fixture)}),
        ("status", {}),
    ]


def update_state(name, parsed, state):
    if name in ("index", "corpus_list", "sources"):
        state["corpus_id"] = state.get("corpus_id") or first_corpus_id(parsed)
    if name in ("knowledge_add_local_folder", "knowledge_sources", "sources"):
        state["source_id"] = state.get("source_id") or first_hit_id(parsed, "source_id")
    if name in ("legal_search", "search"):
        state["doc_id"] = state.get("doc_id") or first_hit_id(parsed, "doc_id")
        state["chunk_id"] = state.get("chunk_id") or first_hit_id(parsed, "chunk_id")
    if name == "review_create":
        state["review_id"] = state.get("review_id") or first_hit_id(parsed, "review_id")
    if name == "review_get":
        state["row_id"] = state.get("row_id") or first_hit_id(parsed, "row_id")
        state["col_id"] = state.get("col_id") or first_hit_id(parsed, "col_id")
    if not state.get("doc_id"):
        state["doc_id"] = first_uuid_like(parsed, ("doc_id", "document_id")) or state.get("doc_id")


def require_env_path(name):
    value = os.environ.get(name)
    if not value:
        raise RuntimeError(f"{name} is required")
    path = pathlib.Path(value)
    if not path.exists():
        raise RuntimeError(f"{name} path does not exist: {path}")
    return path


def main():
    exe = require_env_path("ANNO_RAG_EXE")
    models = require_env_path("ANNO_MODELS_DIR")
    data_dir = pathlib.Path(tempfile.mkdtemp(prefix="anno-mcp-smoke-data-"))
    fixture = pathlib.Path(tempfile.mkdtemp(prefix="anno-mcp-smoke-fixture-"))
    write_fixture(fixture)

    env = os.environ.copy()
    env["ANNO_RAG_DATA_DIR"] = str(data_dir)
    env["ANNO_MODELS_DIR"] = str(models)
    env.setdefault("ANNO_RAG_VAULT_PASSPHRASE", "anno-local-smoke-passphrase")

    proc = subprocess.Popen(
        [str(exe), "mcp"],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        encoding="utf-8",
        env=env,
    )
    out_queue = queue.Queue()
    reader = threading.Thread(target=enqueue_stdout, args=(proc, out_queue), daemon=True)
    reader.start()

    msg = 1
    results = []
    state = {}
    tools = []
    try:
        init = send(
            proc,
            out_queue,
            msg,
            "initialize",
            {
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {"name": "anno-smoke", "version": "1.0"},
            },
        )
        msg += 1
        results.append({"tool": "initialize", "status": "pass", "result": init})
        notify(proc, "notifications/initialized")

        listed = send(proc, out_queue, msg, "tools/list", {})
        msg += 1
        tools = [tool["name"] for tool in listed.get("result", {}).get("tools", [])]
        results.append(
            {"tool": "tools/list", "status": "pass", "result": {"count": len(tools), "tools": tools}}
        )

        called = set()
        planned_steps = smoke_call_plan(fixture, {})
        for step_index, (name, _) in enumerate(planned_steps):
            args = smoke_call_plan(fixture, state)[step_index][1]
            if name not in tools:
                results.append(
                    {"tool": name, "status": "not_advertised", "result": {"arguments": args}}
                )
                continue
            response = call_tool(proc, out_queue, msg, name, args)
            msg += 1
            called.add(name)
            parsed = parse_json_text(response)
            update_state(name, parsed, state)
            status = "pass"
            if "error" in response:
                status = "jsonrpc_error"
            elif isinstance(parsed, dict) and parsed.get("ok") is False:
                status = "contextual_error"
            results.append({"tool": name, "status": status, "result": parsed})

        missing_tool_calls = sorted(set(tools) - called)
        failures = [row for row in results if row["status"] == "jsonrpc_error"]
        contextual_errors = [row for row in results if row["status"] == "contextual_error"]
        critical_contextual_errors = [
            row for row in contextual_errors if row["tool"] in CRITICAL_CONTEXT_TOOLS
        ]
        summary = {
            "exe": str(exe),
            "data_dir": str(data_dir),
            "fixture": str(fixture),
            "models": str(models),
            "tool_count": len(tools),
            "calls": len(results),
            "missing_tool_calls": missing_tool_calls,
            "failures": failures,
            "contextual_errors": contextual_errors,
            "critical_contextual_errors": critical_contextual_errors,
        }
        print(json.dumps(summary, indent=2, ensure_ascii=False))
        return 1 if failures or critical_contextual_errors or missing_tool_calls else 0
    finally:
        if proc.poll() is None:
            proc.terminate()
            try:
                proc.wait(timeout=5)
            except subprocess.TimeoutExpired:
                proc.kill()


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except Exception as exc:
        print(json.dumps({"error": str(exc)}, indent=2), file=sys.stderr)
        raise
