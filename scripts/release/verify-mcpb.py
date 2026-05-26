#!/usr/bin/env python3
"""Validate an anno .mcpb package produced by the release workflow."""

from __future__ import annotations

import argparse
import json
import sys
import zipfile
from pathlib import PurePosixPath


def fail(message: str) -> None:
    print(f"verify-mcpb: {message}", file=sys.stderr)
    raise SystemExit(1)


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("package", help="Path to .mcpb zip package")
    parser.add_argument("--binary", required=True, help="Expected binary filename")
    parser.add_argument("--platform", required=True, help="Expected MCPB platform")
    args = parser.parse_args()

    with zipfile.ZipFile(args.package) as archive:
        names = set(archive.namelist())
        if "manifest.json" not in names:
            fail("manifest.json missing")

        manifest = json.loads(archive.read("manifest.json").decode("utf-8"))
        binary_path = PurePosixPath("server") / args.binary
        binary_entry = str(binary_path)

        if binary_entry not in names:
            fail(f"{binary_entry} missing")

        if manifest.get("server", {}).get("entry_point") != binary_entry:
            fail("server.entry_point does not point to embedded binary")

        expected_command = "${__dirname}/" + binary_entry
        if manifest.get("mcp_config", {}).get("command") != expected_command:
            fail("mcp_config.command does not point to embedded binary")

        if manifest.get("mcp_config", {}).get("args") != ["mcp"]:
            fail("mcp_config.args must be ['mcp']")

        platforms = manifest.get("compatibility", {}).get("platforms", [])
        if platforms != [args.platform]:
            fail(f"compatibility.platforms mismatch: {platforms!r}")

    print(f"verify-mcpb: OK {args.package}")


if __name__ == "__main__":
    main()
