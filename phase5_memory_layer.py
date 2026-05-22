#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
Phase 5: Memory Layer Testing
Cowork 3P Gateway — full memory pipeline validation.

Tools under test:
  memory_save, memory_recall, memory_list, memory_forget,
  memory_invalidate, memory_graph_recall, rehydrate, vault_stats, detect

Suites:
  M1  Basic save / recall (4 tests)
  M2  PII anonymization in memory (4 tests)
  M3  Memory lifecycle — list / forget / invalidate (4 tests)
  M4  Vault round-trip and token consistency (3 tests)
  M5  Advanced recall — graph expand, rerank, detect tool (3 tests)
"""
import subprocess, json, threading, time, re, sys, io, queue, uuid, os
sys.stdout = io.TextIOWrapper(sys.stdout.buffer, encoding='utf-8')

BINARY     = r"C:\cargo-target\release\anno-rag.exe"
# Use passphrase-based vault key so the binary starts regardless of OS keyring state
_ENV = dict(os.environ)
_ENV["ANNO_RAG_VAULT_PASSPHRASE"] = "anno-rag-test-passphrase-2026"
SESSION_ID = f"test-phase5-{int(time.time())}"   # isolated from production data
PASS_COUNT = 0
FAIL_COUNT = 0

# ---------------------------------------------------------------------------
# SafeMcpServer (same queue-based dispatcher as Phase 3)
# ---------------------------------------------------------------------------
class SafeMcpServer:
    def __init__(self):
        self._qid   = 0
        self._lock  = threading.Lock()
        self._inbox = queue.Queue()
        self._proc  = subprocess.Popen(
            [BINARY, "mcp"],
            stdin=subprocess.PIPE, stdout=subprocess.PIPE,
            stderr=subprocess.PIPE, bufsize=0, env=_ENV
        )
        self._reader = threading.Thread(target=self._read_loop, daemon=True)
        self._reader.start()
        self._rpc({"method":"initialize","params":{
            "protocolVersion":"2024-11-05","capabilities":{},
            "clientInfo":{"name":"p5","version":"0.1"}}}, timeout=12)
        # warm-up recall: loads NER + embedder (embed_query path)
        self.call("memory_recall", {"query":"warmup","top_k":1}, timeout=20)
        # warm-up save: exercises embed_batch + memory_insert (first-call cold-start)
        self.call("memory_save", {"text":"Warmup memory for pipeline init.",
                                  "kind":"fact","session_id":"warmup-discard"},
                  timeout=30)

    def _read_loop(self):
        while self._proc.poll() is None:
            try:
                line = self._proc.stdout.readline()
                if line:
                    self._inbox.put(line.decode('utf-8', errors='replace').strip())
            except: break

    def _rpc(self, extra, timeout=10):
        with self._lock:
            self._qid += 1; qid = self._qid
            msg = {"jsonrpc":"2.0","id":qid}
            msg.update(extra)
            self._proc.stdin.write((json.dumps(msg)+"\n").encode())
            self._proc.stdin.flush()
            return self._wait_id(qid, timeout)

    def _wait_id(self, target_id, timeout):
        deadline = time.time() + timeout
        while time.time() < deadline:
            try:
                raw = self._inbox.get(timeout=min(1.0, deadline-time.time()))
            except queue.Empty:
                continue
            try:
                r = json.loads(raw)
                if r.get("id") == target_id:
                    return r
            except: pass
        return None

    def call(self, tool, args, timeout=12):
        return self._rpc({"method":"tools/call",
                          "params":{"name":tool,"arguments":args}}, timeout=timeout)

    def text(self, resp):
        """Extract text content from a tools/call response."""
        if not resp: return ""
        try:
            return " ".join(c.get("text","")
                            for c in resp.get("result",{}).get("content",[]))
        except: return ""

    def alive(self):
        return self._proc.poll() is None

    def close(self):
        try: self._proc.stdin.close()
        except: pass
        try: self._proc.wait(timeout=3)
        except: self._proc.kill()


def check(label, passed, detail=""):
    global PASS_COUNT, FAIL_COUNT
    if passed:
        PASS_COUNT += 1
        print(f"  PASS  {label}")
    else:
        FAIL_COUNT += 1
        print(f"  FAIL  {label}" + (f" | {detail}" if detail else ""))

def has_key(resp, key):
    return key in srv.text(resp)

def extract_id(resp):
    """Pull the first UUID-shaped string from the response text."""
    t = srv.text(resp)
    m = re.search(r'[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}', t)
    return m.group(0) if m else None


# ---------------------------------------------------------------------------
print("=" * 62)
print("  Phase 5: Memory Layer Testing")
print(f"  session_id: {SESSION_ID}")
print("=" * 62)
print()

srv = SafeMcpServer()

# ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
# Suite M1: Basic save / recall
# ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
print("Suite M1: Basic Save / Recall (4 tests)")
print("-" * 54)

# M5.1 — save returns a memory_id (use longer timeout: first real save after warmup)
r = srv.call("memory_save", {
    "text": "The quarterly budget review is scheduled for next Friday.",
    "kind": "fact",
    "session_id": SESSION_ID
}, timeout=30)
m1_id = extract_id(r)
check("M5.1  memory_save returns a UUID memory_id", m1_id is not None,
      f"text={srv.text(r)[:80]}" if m1_id is None else f"id={m1_id}")

# M5.2 — recall by keyword
r = srv.call("memory_recall", {
    "query": "budget review schedule",
    "top_k": 5, "session_id": SESSION_ID
})
t = srv.text(r)
check("M5.2  memory_recall finds memory by keyword", "budget" in t.lower() or "quarterly" in t.lower(),
      f"text={t[:100]}" if "budget" not in t.lower() else "")

# M5.3 — recall by semantic similarity (paraphrase)
r = srv.call("memory_recall", {
    "query": "financial planning meeting next week",
    "top_k": 5, "session_id": SESSION_ID
})
t = srv.text(r)
check("M5.3  memory_recall finds memory by semantic similarity",
      "budget" in t.lower() or "quarterly" in t.lower() or "friday" in t.lower(),
      f"text={t[:100]}" if not any(w in t.lower() for w in ["budget","quarterly","friday"]) else "")

# M5.4 — multiple memories, most relevant ranks first
srv.call("memory_save", {"text": "The office printer is out of toner cartridges.", "kind":"fact","session_id":SESSION_ID}, timeout=20)
srv.call("memory_save", {"text": "Project Phoenix deadline is end of Q2.", "kind":"fact","session_id":SESSION_ID}, timeout=20)
srv.call("memory_save", {"text": "Canteen is closed on Mondays for maintenance.", "kind":"fact","session_id":SESSION_ID}, timeout=20)
r = srv.call("memory_recall", {"query": "project deadline milestone", "top_k": 5, "session_id": SESSION_ID})
t = srv.text(r)
check("M5.4  Multiple memories: relevant one found", "phoenix" in t.lower() or "deadline" in t.lower() or "q2" in t.lower(),
      f"text={t[:120]}" if "phoenix" not in t.lower() else "")
print()

# ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
# Suite M2: PII anonymization in memory
# ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
print("Suite M2: PII Anonymization in Memory (4 tests)")
print("-" * 54)

PII_NAME  = "Dr. Cécile Moreau"
PII_EMAIL = "c.moreau@nexacorp.fr"
PII_PHONE = "+33 6 87 65 43 21"

# M5.5 — person name tokenized in stored memory
# memory_recall REHYDRATES pseudonyms back to plaintext by design; check the
# SAVE response's redacted_text to verify anonymization happened at rest.
r_pii = srv.call("memory_save", {
    "text": f"Meeting with {PII_NAME} confirmed for Thursday at 14h.",
    "kind": "fact", "session_id": SESSION_ID
}, timeout=20)
pii_mem_id = extract_id(r_pii)
pii_save_text = srv.text(r_pii)  # JSON: {"id":..., "redacted_text":..., "token_count":...}
raw_in_save = PII_NAME.lower() in pii_save_text.lower() or "cécile" in pii_save_text.lower()
person_in_save = bool(re.search(r'PERSON_\d+', pii_save_text))
check("M5.5a Raw name not stored in redacted_text", not raw_in_save,
      f"found raw name in save response: {pii_save_text[:120]}" if raw_in_save else "")
check("M5.5b PERSON_N token present in redacted_text", person_in_save,
      f"save_text={pii_save_text[:120]}" if not person_in_save else
      f"tokens={re.findall(r'PERSON_\\d+', pii_save_text)}")

# M5.6 — email tokenized (check save response's redacted_text)
r_email = srv.call("memory_save", {
    "text": f"Send the contract draft to {PII_EMAIL} before end of day.",
    "kind": "reference", "session_id": SESSION_ID
}, timeout=20)
email_save_text = srv.text(r_email)
email_in_save   = PII_EMAIL in email_save_text
email_token_save = bool(re.search(r'EMAIL_\d+', email_save_text))
check("M5.6a Raw email not stored in redacted_text", not email_in_save,
      f"found raw email in save response: {email_save_text[:120]}" if email_in_save else "")
check("M5.6b EMAIL_N token present in redacted_text", email_token_save,
      f"save_text={email_save_text[:120]}" if not email_token_save else
      f"tokens={re.findall(r'EMAIL_\\d+', email_save_text)}")
print()

# ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
# Suite M3: Memory lifecycle
# ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
print("Suite M3: Memory Lifecycle (4 tests)")
print("-" * 54)

# M5.7 — memory_list returns saved memories
r = srv.call("memory_list", {"session_id": SESSION_ID, "limit": 20})
t = srv.text(r)
check("M5.7  memory_list returns session memories",
      "budget" in t.lower() or "phoenix" in t.lower() or "thursday" in t.lower(),
      f"text={t[:120]}" if not t else "")

# M5.8 — memory_forget by id, then recall misses it
forget_text = "Temporary note to be deleted: test-forget-marker-xyz."
r_tmp = srv.call("memory_save", {"text": forget_text, "kind":"context","session_id":SESSION_ID})
tmp_id = extract_id(r_tmp)
if tmp_id:
    srv.call("memory_forget", {"id": tmp_id})
    time.sleep(0.3)
r = srv.call("memory_recall", {"query": "test-forget-marker", "top_k": 5, "session_id": SESSION_ID})
t = srv.text(r)
still_present = "test-forget-marker" in t.lower()
check("M5.8  Forgotten memory absent from recall", not still_present,
      "forgotten memory still returned" if still_present else
      (f"(no id to forget)" if tmp_id is None else ""))

# M5.9 — memory_invalidate; invalidated memory absent from default recall
inval_text = "Budget freeze effective immediately — temporary policy."
r_inv = srv.call("memory_save", {"text": inval_text, "kind":"fact","session_id":SESSION_ID})
inv_id = extract_id(r_inv)
if inv_id:
    srv.call("memory_invalidate", {"id": inv_id})
    time.sleep(0.3)
r = srv.call("memory_recall", {"query": "budget freeze policy", "top_k": 5, "session_id": SESSION_ID})
t = srv.text(r)
inval_in_results = "budget freeze" in t.lower()
check("M5.9  Invalidated memory absent from default recall", not inval_in_results,
      "invalidated memory still returned" if inval_in_results else
      (f"(no id to invalidate)" if inv_id is None else ""))

# M5.10 — memory_forget by query (dry_run first, then real)
r_dry = srv.call("memory_forget", {
    "query": f"test phase5 session {SESSION_ID}",
    "dry_run": True, "limit": 50
})
check("M5.10 memory_forget dry_run returns response", r_dry is not None,
      "no response" if r_dry is None else "")
print()

# ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
# Suite M4: Vault round-trip and token consistency
# ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
print("Suite M4: Vault Round-Trip & Token Consistency (3 tests)")
print("-" * 54)

# M5.11 — vault_stats shows non-zero mappings after PII saves
r = srv.call("vault_stats", {})
t = srv.text(r)
# Look for any number > 0 in the stats output
nums = [int(x) for x in re.findall(r'\b(\d+)\b', t) if int(x) > 0]
check("M5.11 vault_stats shows non-zero token mappings", bool(nums),
      f"text={t[:100]}" if not nums else f"counts={nums[:5]}")

# M5.12 — rehydrate round-trip: token → original value
# Use the PERSON_N token from the M5.5 SAVE response (recall rehydrates, not tokenizes)
person_tokens_in_save = re.findall(r'PERSON_\d+', pii_save_text)
if person_tokens_in_save:
    token_to_test = person_tokens_in_save[0]
    r_rh = srv.call("rehydrate", {"text": f"Contact: {token_to_test}"})
    rh_text = srv.text(r_rh)
    # The rehydrated text should contain the original name, not the token
    token_still_there = token_to_test in rh_text
    check("M5.12 rehydrate replaces PERSON_N with original name",
          not token_still_there,
          f"token {token_to_test} not replaced, got: {rh_text[:80]}" if token_still_there else
          f"'{token_to_test}' → '{rh_text.strip()}'")
else:
    check("M5.12 rehydrate round-trip", False, "no PERSON token found in save response")

# M5.13 — same name in two memories → same token (vault determinism)
# Check the SAVE responses' redacted_text; recall rehydrates so tokens aren't visible there.
# Use a plain first+last name (no honorific) so NER boundary is stable across contexts
SAME_NAME = "Alice Fontaine"
rs1 = srv.call("memory_save", {"text": f"Call from {SAME_NAME} re: contract renewal.", "kind":"fact","session_id":SESSION_ID}, timeout=20)
rs2 = srv.call("memory_save", {"text": f"Meeting scheduled with {SAME_NAME} tomorrow.", "kind":"fact","session_id":SESSION_ID}, timeout=20)
t1 = re.findall(r'PERSON_\d+', srv.text(rs1))
t2 = re.findall(r'PERSON_\d+', srv.text(rs2))
# The same name should produce the same token in both save responses
same_token = bool(t1) and bool(t2) and bool(set(t1) & set(t2))
check("M5.13 Same name produces same PERSON_N across memories",
      same_token,
      f"t1={t1} t2={t2}" if not same_token else f"shared token={set(t1)&set(t2)}")
print()

# ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
# Suite M5: Advanced recall
# ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
print("Suite M5: Advanced Recall (3 tests)")
print("-" * 54)

# M5.14 — memory_recall with rerank=True does not crash
r = srv.call("memory_recall", {
    "query": "project deadline budget",
    "top_k": 5, "session_id": SESSION_ID, "rerank": True
}, timeout=20)
check("M5.14 memory_recall with rerank=True returns result",
      r is not None and "result" in r,
      "no response" if r is None else "")

# M5.15 — memory_graph_recall smoke test (entity from vault)
# Use the EMAIL token from the M5.6 SAVE response (recall rehydrates, not tokenizes)
email_tokens_from_save = re.findall(r'EMAIL_(\d+)', email_save_text)
person_tokens_from_save = re.findall(r'PERSON_(\d+)', pii_save_text)
if email_tokens_from_save:
    entity = f"pii:EMAIL:{email_tokens_from_save[0]}"
    r_gr = srv.call("memory_graph_recall", {"entity": entity, "max_hops": 1}, timeout=15)
    check("M5.15 memory_graph_recall does not crash (EMAIL entity)",
          r_gr is not None and "result" in r_gr,
          "no response" if r_gr is None else "")
elif person_tokens_from_save:
    entity = f"pii:PERSON:{person_tokens_from_save[0]}"
    r_gr = srv.call("memory_graph_recall", {"entity": entity, "max_hops": 1}, timeout=15)
    check("M5.15 memory_graph_recall does not crash (PERSON entity)",
          r_gr is not None and "result" in r_gr,
          "no response" if r_gr is None else "")
else:
    check("M5.15 memory_graph_recall smoke test", False, "no entity token found in save responses")

# M5.16 — detect tool: dry-run PII scan (no storage)
r = srv.call("detect", {"text": "Invoice to be sent to paul.dupont@acme.fr, tel 06 11 22 33 44."})
t = srv.text(r)
check("M5.16 detect tool returns PII spans",
      "email" in t.lower() or "EMAIL" in t or "phone" in t.lower() or "PHONE" in t,
      f"text={t[:120]}" if not t else "")
print()

# ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
# Cleanup: forget all test-session memories
# ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
print("Cleanup: forgetting test-session memories...")
r_cleanup = srv.call("memory_forget", {"query": "budget quarterly printer phoenix canteen meeting contract note policy isabelle fontaine cécile moreau", "limit": 100})
print(f"  cleanup response: {srv.text(r_cleanup)[:80]}")

srv.close()

# ---------------------------------------------------------------------------
print()
print("=" * 62)
print("  PHASE 5 RESULTS")
print("=" * 62)
print()
print(f"  Tests Passed:  {PASS_COUNT}")
print(f"  Tests Failed:  {FAIL_COUNT}")
print(f"  Total:         {PASS_COUNT+FAIL_COUNT}")
print()
if FAIL_COUNT == 0:
    print("  PHASE 5 PASSED -- Memory layer fully operational")
else:
    print(f"  PHASE 5 FAILED -- {FAIL_COUNT} issue(s) need attention")
print()
