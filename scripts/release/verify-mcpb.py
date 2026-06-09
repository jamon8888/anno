#!/usr/bin/env python3
"""Validate an anno .mcpb package produced by the release workflow."""

from __future__ import annotations

import argparse
import json
import sys
import zipfile
from json import JSONDecodeError
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

    if (
        "/" in args.binary
        or "\\" in args.binary
        or ":" in args.binary
        or args.binary in {".", ".."}
    ):
        fail("--binary must be a filename, not a path")

    try:
        with zipfile.ZipFile(args.package) as archive:
            names = set(archive.namelist())
            if "manifest.json" not in names:
                fail("manifest.json missing")

            manifest = json.loads(archive.read("manifest.json").decode("utf-8"))
            if not isinstance(manifest, dict):
                fail("manifest.json must contain a JSON object")

            binary_path = PurePosixPath("server") / args.binary
            binary_entry = str(binary_path)

            if binary_entry not in names:
                fail(f"{binary_entry} missing")

            if manifest.get("server", {}).get("entry_point") != binary_entry:
                fail("server.entry_point does not point to embedded binary")

            expected_command = "${__dirname}/" + binary_entry
            mcp_config = manifest.get("server", {}).get("mcp_config", {})
            if mcp_config.get("command") != expected_command:
                fail("server.mcp_config.command does not point to embedded binary")

            if mcp_config.get("args") != ["mcp"]:
                fail("server.mcp_config.args must be ['mcp']")

            # Reject legacy top-level mcp_config (invalid per MCPB 0.3 spec)
            if "mcp_config" in manifest:
                fail("mcp_config must be inside server, not at top level")

            platforms = manifest.get("compatibility", {}).get("platforms", [])
            if platforms != [args.platform]:
                fail(f"compatibility.platforms mismatch: {platforms!r}")

            # user_config must be an object keyed by config ID, not an array
            user_config = manifest.get("user_config")
            if user_config is not None and not isinstance(user_config, dict):
                fail("user_config must be an object (keyed by config ID), not an array")
    except FileNotFoundError:
        raise
    except zipfile.BadZipFile as exc:
        fail(f"invalid zip package: {exc}")
    except UnicodeDecodeError as exc:
        fail(f"manifest.json is not valid UTF-8: {exc}")
    except JSONDecodeError as exc:
        fail(f"manifest.json is not valid JSON: {exc}")

    print(f"verify-mcpb: OK {args.package}")


if __name__ == "__main__":
    main()
