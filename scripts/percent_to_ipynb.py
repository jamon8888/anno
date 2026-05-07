#!/usr/bin/env python3
"""Convert a `# %%`-percent-format Python script into a Jupyter `.ipynb`.

Cells delimited by `# %%` become code cells; cells delimited by
`# %% [markdown]` become markdown cells (each subsequent `# `-prefixed
line is stripped down to its content).

Usage:
    python scripts/percent_to_ipynb.py input1.py input2.py [...]

Each output sits next to its input, with `.ipynb` extension. Suitable
for direct upload to Google Colab (File → Upload notebook).
"""

from __future__ import annotations

import json
import sys
from pathlib import Path


def split_percent_cells(source: str) -> list[tuple[str, list[str]]]:
    """Split source into [(kind, [lines]), ...]. kind is 'code' or 'markdown'."""
    cells: list[tuple[str, list[str]]] = []
    current_kind = "code"
    current_lines: list[str] = []
    header_consumed = False

    for raw in source.splitlines():
        stripped = raw.rstrip("\n")
        if stripped.startswith("# %% [markdown]"):
            if current_lines and header_consumed:
                cells.append((current_kind, current_lines))
            current_kind = "markdown"
            current_lines = []
            header_consumed = True
        elif stripped.startswith("# %%"):
            if current_lines and header_consumed:
                cells.append((current_kind, current_lines))
            current_kind = "code"
            current_lines = []
            header_consumed = True
        else:
            if not header_consumed:
                # Pre-cell header (shebang, module docstring) — skip.
                continue
            current_lines.append(raw)

    if current_lines:
        cells.append((current_kind, current_lines))

    # Strip leading/trailing empty lines per cell + clean markdown prefix
    cleaned: list[tuple[str, list[str]]] = []
    for kind, lines in cells:
        # trim
        while lines and not lines[0].strip():
            lines.pop(0)
        while lines and not lines[-1].strip():
            lines.pop()
        if not lines:
            continue
        if kind == "markdown":
            # Strip the leading "# " or "#" from markdown comment lines
            md = []
            for line in lines:
                if line.startswith("# "):
                    md.append(line[2:])
                elif line == "#":
                    md.append("")
                else:
                    md.append(line)
            cleaned.append(("markdown", md))
        else:
            cleaned.append(("code", lines))
    return cleaned


def cells_to_notebook(cells: list[tuple[str, list[str]]]) -> dict:
    """Build a minimal nbformat-4 notebook from cell list."""
    nb_cells = []
    for kind, lines in cells:
        # nbformat expects each line to end with \n except the last.
        source = [line + "\n" for line in lines[:-1]] + [lines[-1]] if lines else []
        if kind == "markdown":
            nb_cells.append(
                {"cell_type": "markdown", "metadata": {}, "source": source}
            )
        else:
            nb_cells.append(
                {
                    "cell_type": "code",
                    "metadata": {},
                    "execution_count": None,
                    "outputs": [],
                    "source": source,
                }
            )

    return {
        "cells": nb_cells,
        "metadata": {
            "accelerator": "GPU",
            "colab": {"provenance": []},
            "kernelspec": {
                "display_name": "Python 3",
                "language": "python",
                "name": "python3",
            },
            "language_info": {"name": "python"},
        },
        "nbformat": 4,
        "nbformat_minor": 5,
    }


def convert(path: Path) -> Path:
    src = path.read_text(encoding="utf-8")
    cells = split_percent_cells(src)
    nb = cells_to_notebook(cells)
    out = path.with_suffix(".ipynb")
    out.write_text(json.dumps(nb, indent=1, ensure_ascii=False), encoding="utf-8")
    return out


def main(argv: list[str]) -> int:
    if len(argv) < 2:
        print(__doc__)
        return 1
    for arg in argv[1:]:
        p = Path(arg)
        if not p.exists():
            print(f"[FAIL] not found: {p}")
            return 2
        out = convert(p)
        print(f"[OK] {p}  ->  {out}  ({out.stat().st_size:,} bytes)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
