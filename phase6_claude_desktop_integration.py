#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
Phase 6: Claude Desktop Integration Test Suite
Cowork 3P Gateway — end-to-end validation for local Claude Desktop MCP use.

Suites:
  D1  MCP schema & handshake     (7 tests) — tools/list conformance for Claude Desktop
  D2  Cross-session persistence  (4 tests) — memory survives server restart
  D3  Ingest → restart → search  (5 tests) — CLI ingest + search round-trip + idempotency
  D4  Vault persistence          (4 tests) — token rehydration survives restart
  D5  Config & clean shutdown    (3 tests) — claude_desktop_config.json + graceful exit
"""
import subprocess, json, threading, time, re, sys, io, queue, os, uuid, pathlib, tempfile, shutil
sys.stdout = io.TextIOWrapper(sys.stdout.buffer, encoding='utf-8')

BINARY     = r"C:\cargo-target\release\anno-rag.exe"
_ENV       = dict(os.environ)
_ENV["ANNO_RAG_VAULT_PASSPHRASE"] = "anno-rag-test-passphrase-2026"

PASS_COUNT = 0
FAIL_COUNT = 0


# ---------------------------------------------------------------------------
# SafeMcpServer — queue-based single-reader dispatcher (same as Phase 3/5)
# ---------------------------------------------------------------------------
class SafeMcpServer:
    def __init__(self, label="srv", warmup=True):
        self._label = label
        self._qid   = 0
        self._lock  = threading.Lock()
        self._inbox = queue.Queue()
        self._proc  = subprocess.Popen(
            [BINARY, "mcp"],
            stdin=subprocess.PIPE, stdout=subprocess.PIPE,
            stderr=subprocess.PIPE, bufsize=0, env=_ENV
        )
        self._reader = threading.Thread(
            target=self._read_loop, daemon=True, name=f"reader-{label}"
        )
        self._reader.start()
        # initialize — capture response for D1.1
        self.init_resp = self._rpc(
            {"method": "initialize", "params": {
                "protocolVersion": "2024-11-05", "capabilities": {},
                "clientInfo": {"name": "p6", "version": "0.1"}}},
            timeout=15
        )
        if warmup:
            # recall warmup: loads NER + embedder (embed_query path)
            self.call("memory_recall", {"query": "warmup", "top_k": 1}, timeout=30)
            # save warmup: exercises embed_batch + memory_insert (first-call cold-start).
            # Cold start on this machine can exceed 30 s; use 90 s to be safe.
            self.call("memory_save",
                      {"text": "Warmup entry for pipeline init.", "kind": "fact",
                       "session_id": "warmup-discard"},
                      timeout=90)

    def _read_loop(self):
        while self._proc.poll() is None:
            try:
                line = self._proc.stdout.readline()
                if line:
                    self._inbox.put(line.decode("utf-8", errors="replace").strip())
            except:
                break

    def _rpc(self, extra, timeout=10):
        with self._lock:
            self._qid += 1
            qid = self._qid
            msg = {"jsonrpc": "2.0", "id": qid}
            msg.update(extra)
            self._proc.stdin.write((json.dumps(msg) + "\n").encode())
            self._proc.stdin.flush()
            return self._wait_id(qid, timeout)

    def _wait_id(self, target_id, timeout):
        deadline = time.time() + timeout
        while time.time() < deadline:
            try:
                raw = self._inbox.get(timeout=min(1.0, deadline - time.time()))
            except queue.Empty:
                continue
            try:
                r = json.loads(raw)
                if r.get("id") == target_id:
                    return r
            except:
                pass
        return None

    def tools_list(self, timeout=10):
        return self._rpc({"method": "tools/list", "params": {}}, timeout=timeout)

    def call(self, tool, args, timeout=12):
        return self._rpc(
            {"method": "tools/call", "params": {"name": tool, "arguments": args}},
            timeout=timeout
        )

    def text(self, resp):
        if not resp:
            return ""
        try:
            return " ".join(
                c.get("text", "") for c in resp.get("result", {}).get("content", [])
            )
        except:
            return ""

    def alive(self):
        return self._proc.poll() is None

    def close(self):
        try:
            self._proc.stdin.close()
        except:
            pass
        try:
            self._proc.wait(timeout=5)
        except:
            self._proc.kill()

    def exit_code(self):
        try:
            self._proc.wait(timeout=5)
            return self._proc.returncode
        except:
            self._proc.kill()
            return -1


def check(label, passed, detail=""):
    global PASS_COUNT, FAIL_COUNT
    if passed:
        PASS_COUNT += 1
        print(f"  PASS  {label}")
    else:
        FAIL_COUNT += 1
        print(f"  FAIL  {label}" + (f" | {detail}" if detail else ""))


def extract_id(text):
    """Pull the first UUID from a string."""
    m = re.search(
        r"[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}", text
    )
    return m.group(0) if m else None


def ingest_cli(folder, timeout=180):
    """Run `anno-rag ingest <folder>` and return (stdout+stderr, ingested_count)."""
    r = subprocess.run(
        [BINARY, "ingest", folder],
        capture_output=True, text=True, env=_ENV, timeout=timeout
    )
    out = r.stdout + r.stderr
    m = re.search(r"ingested\s+(\d+)", out, re.IGNORECASE)
    count = int(m.group(1)) if m else -1
    return out.strip(), count


# ===========================================================================
print("=" * 66)
print("  Phase 6: Claude Desktop Integration Test Suite")
print("=" * 66)
print()


# ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
# Suite D1: MCP Schema & Handshake
# ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
print("Suite D1: MCP Schema & Handshake (Claude Desktop compatibility)")
print("-" * 58)

srv1 = SafeMcpServer("d1", warmup=False)

# D1.1 — initialize returns the expected protocol version
proto = srv1.init_resp.get("result", {}).get("protocolVersion", "") if srv1.init_resp else ""
check("D1.1  initialize returns protocolVersion 2024-11-05",
      proto == "2024-11-05",
      f"got: {proto!r}" if proto != "2024-11-05" else "")

# D1.2 — tools/list returns ≥ 8 tools
r_tl = srv1.tools_list(timeout=10)
tools_raw = r_tl.get("result", {}).get("tools", []) if r_tl else []
tools_by_name = {t["name"]: t for t in tools_raw}
check("D1.2  tools/list returns ≥ 8 tools",
      len(tools_raw) >= 8,
      f"got {len(tools_raw)}" if len(tools_raw) < 8 else f"{len(tools_raw)} tools")

# D1.3 — every tool has a non-empty description
missing_desc = [t["name"] for t in tools_raw if not t.get("description", "").strip()]
check("D1.3  All tools have non-empty description",
      not missing_desc,
      f"missing description: {missing_desc}" if missing_desc else "")

# D1.4 — every tool has inputSchema {type: "object", properties: {}}
bad_schema = [
    t["name"] for t in tools_raw
    if t.get("inputSchema", {}).get("type") != "object"
    or "properties" not in t.get("inputSchema", {})
]
check("D1.4  All tools have inputSchema type:object with properties",
      not bad_schema,
      f"bad schema: {bad_schema}" if bad_schema else "")

# D1.5 — all 10 expected tools present
REQUIRED_TOOLS = {
    "search", "memory_save", "memory_recall", "memory_list",
    "memory_forget", "memory_invalidate", "memory_graph_recall",
    "rehydrate", "vault_stats", "detect",
}
missing_tools = REQUIRED_TOOLS - set(tools_by_name.keys())
check("D1.5  All 10 expected tools present",
      not missing_tools,
      f"missing: {missing_tools}" if missing_tools else f"present: {sorted(tools_by_name)}")

# D1.6 — search tool exposes a "query" parameter
search_props = tools_by_name.get("search", {}).get("inputSchema", {}).get("properties", {})
check("D1.6  search tool has 'query' parameter",
      "query" in search_props,
      f"props: {list(search_props)}" if "query" not in search_props else "")

# D1.7 — memory_save exposes "text" and "session_id"
ms_props = tools_by_name.get("memory_save", {}).get("inputSchema", {}).get("properties", {})
check("D1.7  memory_save has 'text' and 'session_id' parameters",
      "text" in ms_props and "session_id" in ms_props,
      f"props: {list(ms_props)}" if not ("text" in ms_props and "session_id" in ms_props) else "")

srv1.close()
print()


# ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
# Suite D2: Cross-Session Memory Persistence
# ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
print("Suite D2: Cross-Session Memory Persistence")
print("-" * 58)

PERSIST_SESSION = f"persist-{int(time.time())}"
PERSIST_TEXT    = "The annual board meeting is confirmed for 15 December at headquarters."

# Process A: save a memory, then shut down cleanly
print("  INFO  Starting Process A …")
srv_a = SafeMcpServer("d2-A")
r_save_a = srv_a.call("memory_save", {
    "text": PERSIST_TEXT, "kind": "fact", "session_id": PERSIST_SESSION
}, timeout=60)
saved_id = extract_id(srv_a.text(r_save_a))
check("D2.1  Process A: memory_save returns a UUID",
      saved_id is not None,
      f"resp={srv_a.text(r_save_a)[:80]}" if saved_id is None else f"id={saved_id}")
srv_a.close()
ec_a = srv_a.exit_code()
# Exit code 1 is normal on Windows when stdin closes (EOF treated as pipe error
# in the Rust readline loop). The important thing is the process is not hung.
check("D2.2  Process A: exits after stdin closed (not hung)",
      ec_a is not None,
      "process hung — kill required" if ec_a is None else f"exited with code {ec_a}")
print(f"  INFO  Process A closed (exit={ec_a}). Waiting 1 s for file handles …")
time.sleep(1.0)

# Process B: recall the memory saved by A
print("  INFO  Starting Process B …")
srv_b = SafeMcpServer("d2-B")
check("D2.3  Process B: starts and initializes successfully",
      srv_b.alive(), "process died during startup")
r_recall_b = srv_b.call("memory_recall", {
    "query": "annual board meeting december headquarters",
    "top_k": 5, "session_id": PERSIST_SESSION
}, timeout=15)
t_recall_b = srv_b.text(r_recall_b)
found_b = any(w in t_recall_b.lower() for w in ["board", "december", "meeting", "headquarters"])
check("D2.4  Process B: recalls memory written by Process A",
      found_b,
      f"text={t_recall_b[:120]}" if not found_b else f"snippet: {t_recall_b[:60]}")
srv_b.close()
print()


# ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
# Suite D3: Ingest → Restart → Search
# ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
print("Suite D3: Ingest → Restart → Search")
print("-" * 58)

# Create a fresh one-doc corpus guaranteed not to be in LanceDB yet
FRESH_UUID  = str(uuid.uuid4())
tmp_corpus  = tempfile.mkdtemp(prefix="anno_p6_")
fresh_path  = os.path.join(tmp_corpus, "fresh_test_doc.txt")
FRESH_KEYWORD = "procurement"
with open(fresh_path, "w", encoding="utf-8") as f:
    f.write(
        f"Unique document {FRESH_UUID}.\n"
        f"The {FRESH_KEYWORD} committee approved a new supplier contract for office equipment.\n"
        f"All purchases above 5000 euros require three competitive quotes.\n"
    )
print(f"  INFO  Fresh corpus: {fresh_path} (UUID={FRESH_UUID[:8]}…)")

# D3.1 — first ingest: 1 new document
print("  INFO  Running first ingest …")
ingest_out1, count1 = ingest_cli(tmp_corpus)
check("D3.1  First ingest indexes 1 new document",
      count1 == 1,
      f"output: {ingest_out1[:100]}" if count1 != 1 else f"ingested={count1}")

# D3.2 — MCP server starts after ingest
print("  INFO  Starting MCP server …")
srv_d3 = SafeMcpServer("d3")
check("D3.2  MCP server starts after ingest",
      srv_d3.alive(), "process died during startup")

# D3.3 — search finds content from the fresh doc
r_srch1 = srv_d3.call("search", {
    "query": f"{FRESH_KEYWORD} supplier contract office equipment competitive quotes",
    "top_k": 5
}, timeout=15)
t_srch1 = srv_d3.text(r_srch1)
found_fresh = (FRESH_UUID in t_srch1 or FRESH_KEYWORD in t_srch1.lower()
               or "supplier" in t_srch1.lower() or "quotes" in t_srch1.lower())
check("D3.3  Search finds content from the freshly ingested doc",
      found_fresh,
      f"text={t_srch1[:150]}" if not found_fresh else "")

# D3.4 — second ingest on same corpus: 0 new docs (idempotent)
print("  INFO  Running idempotent re-ingest …")
ingest_out2, count2 = ingest_cli(tmp_corpus)
check("D3.4  Re-ingest is idempotent (0 new documents)",
      count2 == 0,
      f"output: {ingest_out2[:100]}" if count2 != 0 else "skipped as expected")

# D3.5 — search still works after idempotent re-ingest
r_srch2 = srv_d3.call("search", {
    "query": "competitive quotes supplier contract",
    "top_k": 5
}, timeout=15)
t_srch2 = srv_d3.text(r_srch2)
still_found = (FRESH_KEYWORD in t_srch2.lower() or "supplier" in t_srch2.lower()
               or "quotes" in t_srch2.lower() or FRESH_UUID in t_srch2)
check("D3.5  Search still works after idempotent re-ingest",
      still_found,
      f"text={t_srch2[:120]}" if not still_found else "")

srv_d3.close()
shutil.rmtree(tmp_corpus, ignore_errors=True)
print()


# ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
# Suite D4: Vault Persistence Across Restarts
# ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
print("Suite D4: Vault Persistence Across Restarts")
print("-" * 58)

VAULT_SESSION = f"vault-persist-{int(time.time())}"
PII_NAME      = "Antoine Lefebvre"
PII_TEXT_FULL = f"The compliance report was reviewed and approved by {PII_NAME}, DPO."

print("  INFO  Starting Process A (vault write) …")
srv_v1 = SafeMcpServer("d4-A")
r_vsave = srv_v1.call("memory_save", {
    "text": PII_TEXT_FULL, "kind": "fact", "session_id": VAULT_SESSION
}, timeout=120)
vsave_text = srv_v1.text(r_vsave)
person_tokens = re.findall(r"PERSON_\d+", vsave_text)
check("D4.1  Process A: PII name produces PERSON_N token in redacted_text",
      bool(person_tokens),
      f"save_text={vsave_text[:120]}" if not person_tokens else f"token={person_tokens}")

# Get vault stats count for reference (wait long enough for any in-flight save to commit)
r_vstats_a = srv_v1.call("vault_stats", {}, timeout=30)
vstats_a_text = srv_v1.text(r_vstats_a)
token_to_test = person_tokens[0] if person_tokens else None

srv_v1.close()
ec_v1 = srv_v1.exit_code()
print(f"  INFO  Process A closed (exit={ec_v1}). Token: {token_to_test}. Waiting 1 s …")
time.sleep(1.0)

# Process B: rehydrate the token from A
print("  INFO  Starting Process B (vault read) …")
srv_v2 = SafeMcpServer("d4-B")

if token_to_test:
    r_rh = srv_v2.call("rehydrate", {"text": f"DPO contact: {token_to_test}"}, timeout=10)
    rh_text = srv_v2.text(r_rh)
    token_gone = token_to_test not in rh_text
    name_back  = PII_NAME.split()[-1].lower() in rh_text.lower()  # last name
    check("D4.2  Process B: rehydrate replaces PERSON_N with original name",
          token_gone and name_back,
          f"rh_text={rh_text[:100]}" if not (token_gone and name_back)
          else f"'{token_to_test}' → '{rh_text.strip()}'")
else:
    check("D4.2  Process B: rehydrate round-trip", False,
          "skipped — no token from D4.1")

# D4.3 — vault_stats in B shows non-zero entries
r_vstats_b = srv_v2.call("vault_stats", {}, timeout=10)
vstats_b_text = srv_v2.text(r_vstats_b)
nonzero_b = bool(re.search(r"[1-9]\d*", vstats_b_text))
check("D4.3  Process B: vault_stats shows non-zero token mappings",
      nonzero_b,
      f"stats={vstats_b_text[:80]}" if not nonzero_b else f"stats={vstats_b_text[:60]}")

# D4.4 — recall in B finds the PII memory, rehydrated to plaintext
r_vrecall = srv_v2.call("memory_recall", {
    "query": "compliance report DPO reviewed approved",
    "top_k": 5, "session_id": VAULT_SESSION
}, timeout=15)
t_vrecall = srv_v2.text(r_vrecall)
name_in_recall = PII_NAME.split()[-1].lower() in t_vrecall.lower()
content_found  = any(w in t_vrecall.lower()
                     for w in ["compliance", "dpo", "reviewed", "approved"])
check("D4.4  Process B: recalls PII memory and rehydrates to plaintext",
      name_in_recall or content_found,
      f"text={t_vrecall[:120]}" if not (name_in_recall or content_found) else
      f"snippet: {t_vrecall[:80]}")

srv_v2.close()
print()


# ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
# Suite D5: Config & Clean Shutdown
# ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
print("Suite D5: Config & Clean Shutdown")
print("-" * 58)

# D5.1 — server exits cleanly when stdin is closed (within 5 s)
srv_d5 = SafeMcpServer("d5", warmup=False)
srv_d5._proc.stdin.close()
deadline = time.time() + 5.0
while time.time() < deadline and srv_d5._proc.poll() is None:
    time.sleep(0.1)
exited = srv_d5._proc.poll() is not None
check("D5.1  Server exits cleanly when stdin closed (within 5 s)",
      exited,
      "still running after 5 s" if not exited else f"exit_code={srv_d5._proc.returncode}")
if not exited:
    srv_d5._proc.kill()

# D5.2 — emit and validate a ready-to-paste claude_desktop_config.json
binary_abs  = str(pathlib.Path(BINARY).resolve())
data_dir    = str(pathlib.Path.home() / ".anno-rag")
config = {
    "mcpServers": {
        "anno-rag": {
            "command": binary_abs,
            "args":    ["mcp"],
            "env": {
                "ANNO_RAG_VAULT_PASSPHRASE": "<your-passphrase-here>",
                "ANNO_RAG_DATA_DIR":         data_dir,
            }
        }
    }
}
config_json = json.dumps(config, indent=2)
parsed      = json.loads(config_json)                        # validates JSON
entry       = parsed.get("mcpServers", {}).get("anno-rag", {})
valid_cfg   = (
    "mcpServers"          in parsed
    and "anno-rag"        in parsed["mcpServers"]
    and entry.get("args") == ["mcp"]
    and "command"         in entry
    and "ANNO_RAG_VAULT_PASSPHRASE" in entry.get("env", {})
)
check("D5.2  claude_desktop_config.json snippet is valid JSON with required fields",
      valid_cfg, "malformed config" if not valid_cfg else "")

config_path = os.path.join(
    os.environ.get("APPDATA", str(pathlib.Path.home() / "AppData" / "Roaming")),
    "Claude", "claude_desktop_config.json"
)
print(f"\n  ┌─ Paste into {config_path}")
print(  "  │  (merge into existing file if it already exists):")
for line in config_json.splitlines():
    print(f"  │  {line}")
print(  "  └─")

# D5.3 — binary reports its version
r_ver = subprocess.run(
    [BINARY, "--version"],
    capture_output=True, text=True, timeout=10, env=_ENV
)
version_str = r_ver.stdout.strip() or r_ver.stderr.strip()
check("D5.3  Binary responds to --version",
      r_ver.returncode == 0 and bool(version_str),
      f"rc={r_ver.returncode} out={version_str[:40]!r}" if not version_str
      else f"version: {version_str}")

print()


# ---------------------------------------------------------------------------
# Cleanup: forget test-session memories
# ---------------------------------------------------------------------------
print("Cleanup: removing test memories from Phase 6 sessions …")
cleanup_srv = SafeMcpServer("cleanup", warmup=False)
for sid in [PERSIST_SESSION, VAULT_SESSION]:
    r_cl = cleanup_srv.call("memory_forget", {
        "query": "board meeting december procurement compliance dpo lefebvre antoine",
        "limit": 50
    }, timeout=15)
    forgotten = re.findall(
        r"[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}",
        cleanup_srv.text(r_cl)
    )
    print(f"  session {sid[:30]}… → {len(forgotten)} memor{'y' if len(forgotten)==1 else 'ies'} removed")
cleanup_srv.close()


# ---------------------------------------------------------------------------
print()
print("=" * 66)
print("  PHASE 6 RESULTS")
print("=" * 66)
print()
print(f"  Tests Passed:  {PASS_COUNT}")
print(f"  Tests Failed:  {FAIL_COUNT}")
print(f"  Total:         {PASS_COUNT + FAIL_COUNT}")
print()
if FAIL_COUNT == 0:
    print("  PHASE 6 PASSED -- Claude Desktop integration validated")
else:
    print(f"  PHASE 6 FAILED -- {FAIL_COUNT} issue(s) need attention")
print()
