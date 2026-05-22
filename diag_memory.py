import subprocess, json, threading, time

BINARY = r"C:\cargo-target\release\anno-rag.exe"
proc = subprocess.Popen([BINARY, "mcp"], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, bufsize=0)

def send(msg):
    proc.stdin.write((json.dumps(msg) + "\n").encode())
    proc.stdin.flush()

def recv(t=12):
    r=[None]
    def _r():
        try: r[0]=proc.stdout.readline().decode('utf-8',errors='replace').strip()
        except: pass
    th=threading.Thread(target=_r,daemon=True); th.start(); th.join(t)
    return r[0]

send({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"dbg","version":"0.1"}}})
recv(12)

send({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"memory_save","arguments":{"text":"Test memory diagnostics 12345.","kind":"fact","session_id":"diag"}}})
r = recv(12)
print("=== memory_save ===")
if r:
    p = json.loads(r)
    print(json.dumps(p, indent=2)[:1000])

send({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"memory_recall","arguments":{"query":"test memory diagnostics","top_k":3,"session_id":"diag"}}})
r = recv(12)
print("\n=== memory_recall ===")
if r:
    p = json.loads(r)
    print(json.dumps(p, indent=2)[:800])

send({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"memory_list","arguments":{"session_id":"diag","limit":5}}})
r = recv(12)
print("\n=== memory_list ===")
if r:
    p = json.loads(r)
    print(json.dumps(p, indent=2)[:800])

proc.stdin.close()
try: proc.wait(timeout=3)
except: proc.kill()
