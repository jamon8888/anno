#!/usr/bin/env python3
"""Generate an asciicast v2 recording for the CloakPipe demo."""

import json
import sys

COLS = 100
ROWS = 35

def make_cast(events):
    """Generate asciicast v2 format."""
    header = {
        "version": 2,
        "width": COLS,
        "height": ROWS,
        "timestamp": 1741416000,
        "title": "CloakPipe Demo — Privacy Middleware for LLM Pipelines",
        "env": {"TERM": "xterm-256color", "SHELL": "/bin/zsh"},
    }
    lines = [json.dumps(header)]
    for ts, typ, data in events:
        lines.append(json.dumps([round(ts, 4), typ, data]))
    return "\n".join(lines) + "\n"


def type_cmd(t, cmd, prompt="$ "):
    """Simulate typing a command character by character."""
    events = []
    events.append((t, "o", f"\x1b[1;32m{prompt}\x1b[0m"))
    t += 0.3
    for ch in cmd:
        events.append((t, "o", ch))
        t += 0.04  # typing speed
    t += 0.5  # pause before enter
    events.append((t, "o", "\r\n"))
    t += 0.1
    return events, t


def output(t, text, delay=0.02):
    """Output text line by line with small delays."""
    events = []
    for line in text.split("\n"):
        events.append((t, "o", line + "\r\n"))
        t += delay
    return events, t


def section_pause(t, secs=1.5):
    return t + secs


def main():
    events = []
    t = 0.5

    # -- Title card --
    title_text = (
        "\x1b[1;36m"
        "╔══════════════════════════════════════════════════════════════╗\r\n"
        "║              CloakPipe v0.4 — Live Demo                    ║\r\n"
        "║     Privacy middleware for LLM & RAG pipelines             ║\r\n"
        "╚══════════════════════════════════════════════════════════════╝"
        "\x1b[0m\r\n"
    )
    events.append((t, "o", title_text))
    t = section_pause(t, 2.5)
    events.append((t, "o", "\x1b[2J\x1b[H"))  # clear screen
    t += 0.3

    # -- Step 1: Show help --
    e, t = type_cmd(t, "cloakpipe --help")
    events.extend(e)

    help_out = (
        "Privacy middleware for LLM & RAG pipelines\r\n"
        "\r\n"
        "\x1b[1;33mUsage:\x1b[0m cloakpipe [OPTIONS] <COMMAND>\r\n"
        "\r\n"
        "\x1b[1;33mCommands:\x1b[0m\r\n"
        "  \x1b[1;32mstart\x1b[0m   Start the CloakPipe proxy server\r\n"
        "  \x1b[1;32mtest\x1b[0m    Test detection on sample text\r\n"
        "  \x1b[1;32mstats\x1b[0m   Show vault statistics\r\n"
        "  \x1b[1;32minit\x1b[0m    Initialize a new cloakpipe.toml config file\r\n"
        "  \x1b[1;32mtree\x1b[0m    CloakTree: vectorless document retrieval\r\n"
        "  \x1b[1;32mvector\x1b[0m  ADCPE: encrypt/decrypt embedding vectors\r\n"
        "  help    Print this message or the help of the given subcommand(s)\r\n"
        "\r\n"
        "\x1b[1;33mOptions:\x1b[0m\r\n"
        "  -c, --config <CONFIG>  Path to configuration file [default: cloakpipe.toml]\r\n"
        "  -h, --help             Print help\r\n"
        "  -V, --version          Print version\r\n"
    )
    e, t = output(t, help_out.replace("\r\n", "\n").rstrip("\n"), delay=0.03)
    events.extend(e)
    t = section_pause(t, 2.0)

    # -- Step 2: Test detection --
    events.append((t, "o", "\r\n"))
    t += 0.2

    test_cmd = (
        'cloakpipe test --text "Quarterly Report: Tata Motors reported '
        'Rs 3.4L Cr revenue in Q3 2025. Send wire transfer of \\$2.8M to '
        'alice.chen@tatagroup.com. AWS credentials: AKIAIOSFODNN7EXAMPLE"'
    )
    e, t = type_cmd(t, test_cmd)
    events.extend(e)
    t += 0.3

    test_out = (
        "\r\n"
        "\x1b[1;36m--- Input ---\x1b[0m\r\n"
        "Quarterly Report: Tata Motors reported Rs 3.4L Cr revenue in Q3 2025.\r\n"
        "Send wire transfer of $2.8M to alice.chen@tatagroup.com.\r\n"
        "AWS credentials: AKIAIOSFODNN7EXAMPLE\r\n"
        "\r\n"
        "\x1b[1;33m--- Detected Entities (4) ---\x1b[0m\r\n"
        "  \x1b[1;31m[Date]\x1b[0m     \"Q3 2025\"                  (confidence: 100%, source: Financial)\r\n"
        "  \x1b[1;31m[Amount]\x1b[0m   \"$2.8M\"                    (confidence: 100%, source: Financial)\r\n"
        "  \x1b[1;31m[Email]\x1b[0m    \"alice.chen@tatagroup.com\"  (confidence: 100%, source: Pattern)\r\n"
        "  \x1b[1;31m[Secret]\x1b[0m   \"AKIAIOSFODNN7EXAMPLE\"     (confidence: 100%, source: Pattern)\r\n"
        "\r\n"
        "\x1b[1;32m--- Pseudonymized ---\x1b[0m\r\n"
        "Quarterly Report: Tata Motors reported Rs 3.4L Cr revenue in \x1b[1;33mDATE_1\x1b[0m.\r\n"
        "Send wire transfer of \x1b[1;33mAMOUNT_1\x1b[0m to \x1b[1;33mEMAIL_1\x1b[0m.\r\n"
        "AWS credentials: \x1b[1;33mSECRET_1\x1b[0m\r\n"
        "\r\n"
        "\x1b[1;32m--- Rehydrated ---\x1b[0m\r\n"
        "Quarterly Report: Tata Motors reported Rs 3.4L Cr revenue in Q3 2025.\r\n"
        "Send wire transfer of $2.8M to alice.chen@tatagroup.com.\r\n"
        "AWS credentials: AKIAIOSFODNN7EXAMPLE\r\n"
        "\r\n"
        "  Tokens rehydrated: \x1b[1;32m4\x1b[0m\r\n"
        "  Roundtrip match: \x1b[1;32mYES ✓\x1b[0m\r\n"
    )
    for line in test_out.split("\r\n"):
        events.append((t, "o", line + "\r\n"))
        t += 0.08  # slower for readability
    t = section_pause(t, 3.0)

    # -- Step 3: Vector encryption --
    events.append((t, "o", "\r\n"))
    t += 0.2

    e, t = type_cmd(t, "cloakpipe vector test")
    events.extend(e)
    t += 0.3

    vec_out = (
        "\x1b[1;36mADCPE Test (dim=8)\x1b[0m\r\n"
        "  Cosine similarity (original):  \x1b[1;37m0.437748\x1b[0m\r\n"
        "  Cosine similarity (encrypted): \x1b[1;37m0.437748\x1b[0m\r\n"
        "  Distance preserved: \x1b[1;32mYES ✓\x1b[0m\r\n"
        "  Roundtrip max error: \x1b[1;37m3.33e-16\x1b[0m\r\n"
        "  Roundtrip exact: \x1b[1;32mYES ✓\x1b[0m\r\n"
    )
    for line in vec_out.split("\r\n"):
        events.append((t, "o", line + "\r\n"))
        t += 0.1
    t = section_pause(t, 2.5)

    # -- Step 4: Init config --
    events.append((t, "o", "\r\n"))
    t += 0.2

    e, t = type_cmd(t, "cloakpipe init")
    events.extend(e)
    t += 0.2

    init_out = (
        "Created cloakpipe.toml\r\n"
        "\r\n"
        "Next steps:\r\n"
        "  1. Set OPENAI_API_KEY (or your upstream API key)\r\n"
        "  2. Set CLOAKPIPE_VAULT_KEY=$(openssl rand -hex 32)\r\n"
        "  3. Run: cloakpipe start\r\n"
    )
    for line in init_out.split("\r\n"):
        events.append((t, "o", line + "\r\n"))
        t += 0.06
    t = section_pause(t, 2.0)

    # -- Step 5: Show proxy start --
    events.append((t, "o", "\r\n"))
    t += 0.2

    e, t = type_cmd(t, "cloakpipe start")
    events.extend(e)
    t += 0.5

    start_out = (
        "\x1b[2m2026-03-08T00:00:01Z\x1b[0m \x1b[32m INFO\x1b[0m cloakpipe_proxy: Loaded config from cloakpipe.toml\r\n"
        "\x1b[2m2026-03-08T00:00:01Z\x1b[0m \x1b[32m INFO\x1b[0m cloakpipe_proxy: Vault initialized (AES-256-GCM, file backend)\r\n"
        "\x1b[2m2026-03-08T00:00:01Z\x1b[0m \x1b[32m INFO\x1b[0m cloakpipe_proxy: Detection engine ready (4 layers)\r\n"
        "\x1b[2m2026-03-08T00:00:01Z\x1b[0m \x1b[1;32m INFO\x1b[0m cloakpipe_proxy: \x1b[1mListening on 127.0.0.1:8900\x1b[0m\r\n"
        "\x1b[2m2026-03-08T00:00:01Z\x1b[0m \x1b[32m INFO\x1b[0m cloakpipe_proxy: Upstream: https://api.openai.com\r\n"
    )
    for line in start_out.split("\r\n"):
        events.append((t, "o", line + "\r\n"))
        t += 0.15
    t = section_pause(t, 1.5)

    # Simulate Ctrl+C
    events.append((t, "o", "^C\r\n"))
    t += 0.5

    # -- Step 6: Show Python usage --
    events.append((t, "o", "\r\n"))
    t += 0.2

    comment = "\x1b[2m# That's it. Point any OpenAI client at CloakPipe:\x1b[0m\r\n"
    events.append((t, "o", comment))
    t += 0.8

    python_code = (
        "\x1b[1;36m"
        '  client = OpenAI(base_url="http://127.0.0.1:8900/v1")\r\n'
        "\x1b[0m"
        "\x1b[2m# Your data is pseudonymized before it leaves your machine.\x1b[0m\r\n"
        "\x1b[2m# Responses are rehydrated automatically. Zero code changes.\x1b[0m\r\n"
    )
    for line in python_code.split("\r\n"):
        events.append((t, "o", line + "\r\n"))
        t += 0.12
    t = section_pause(t, 2.0)

    # -- Outro --
    events.append((t, "o", "\r\n"))
    outro = (
        "\x1b[1;36m"
        "  ┌─────────────────────────────────────────────────────────┐\r\n"
        "  │  github.com/rohansx/cloakpipe                          │\r\n"
        "  │  cargo install cloakpipe-cli                            │\r\n"
        "  │  MIT License • 45 tests • <5ms overhead                 │\r\n"
        "  └─────────────────────────────────────────────────────────┘"
        "\x1b[0m\r\n"
    )
    events.append((t, "o", outro))
    t = section_pause(t, 3.0)

    # Write output
    cast = make_cast(events)
    outpath = "demo/cloakpipe-demo.cast"
    with open(outpath, "w") as f:
        f.write(cast)
    print(f"Written to {outpath} ({len(events)} events, {t:.1f}s duration)")


if __name__ == "__main__":
    main()
