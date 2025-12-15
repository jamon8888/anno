#!/usr/bin/env python3
"""
Compute SHA256 checksums for cached datasets.

Usage:
    uv run python scripts/compute_checksums.py           # Compute all
    uv run python scripts/compute_checksums.py --update  # Update manifest
    uv run python scripts/compute_checksums.py --verify  # Verify against manifest
"""

import hashlib
import json
import sys
from pathlib import Path

CACHE_DIR = Path.home() / "Library/Caches/anno/datasets"
MANIFEST_PATH = Path.home() / "Library/Caches/anno/manifest.json"
OUTPUT_FILE = Path(__file__).parent / "checksums.json"


def compute_sha256(file_path: Path) -> str:
    """Compute SHA256 hash of a file."""
    sha256 = hashlib.sha256()
    with open(file_path, "rb") as f:
        for chunk in iter(lambda: f.read(8192), b""):
            sha256.update(chunk)
    return sha256.hexdigest()


def load_manifest() -> dict:
    """Load existing manifest."""
    if MANIFEST_PATH.exists():
        with open(MANIFEST_PATH) as f:
            return json.load(f)
    return {"version": 1, "entries": {}, "url_health": {}}


def save_manifest(manifest: dict):
    """Save manifest."""
    with open(MANIFEST_PATH, "w") as f:
        json.dump(manifest, f, indent=2)


def compute_all_checksums() -> dict:
    """Compute checksums for all cached files."""
    if not CACHE_DIR.exists():
        print(f"Cache directory not found: {CACHE_DIR}")
        return {}

    checksums = {}
    files = list(CACHE_DIR.iterdir())
    
    print(f"Computing checksums for {len(files)} files...")
    
    for i, file_path in enumerate(sorted(files)):
        if file_path.is_file() and file_path.name != ".gitkeep":
            checksum = compute_sha256(file_path)
            size = file_path.stat().st_size
            checksums[file_path.name] = {
                "sha256": checksum,
                "size": size,
            }
            if (i + 1) % 20 == 0:
                print(f"  Processed {i + 1}/{len(files)} files...")
    
    print(f"Computed {len(checksums)} checksums.")
    return checksums


def verify_checksums(checksums: dict) -> tuple[int, int, int]:
    """Verify checksums against manifest."""
    manifest = load_manifest()
    entries = manifest.get("entries", {})
    
    matched = 0
    mismatched = 0
    missing = 0
    
    for filename, data in checksums.items():
        entry = entries.get(filename)
        if entry is None:
            missing += 1
        elif entry.get("sha256") == data["sha256"]:
            matched += 1
        else:
            mismatched += 1
            print(f"  MISMATCH: {filename}")
            print(f"    Expected: {entry.get('sha256')}")
            print(f"    Actual:   {data['sha256']}")
    
    return matched, mismatched, missing


def update_manifest(checksums: dict):
    """Update manifest with computed checksums."""
    manifest = load_manifest()
    entries = manifest.get("entries", {})
    
    updated = 0
    added = 0
    
    for filename, data in checksums.items():
        if filename in entries:
            # Update existing entry
            old_sha = entries[filename].get("sha256")
            if old_sha != data["sha256"]:
                entries[filename]["sha256"] = data["sha256"]
                entries[filename]["file_size"] = data["size"]
                updated += 1
        else:
            # Add new entry (minimal)
            entries[filename] = {
                "dataset_id": filename,
                "source_url": "",
                "sha256": data["sha256"],
                "file_size": data["size"],
                "downloaded_at": "",
                "sentence_count": 0,
                "entity_count": 0,
                "anno_version": "unknown",
            }
            added += 1
    
    manifest["entries"] = entries
    save_manifest(manifest)
    
    print(f"Updated: {updated}, Added: {added}")


def generate_rust_snippets(checksums: dict) -> str:
    """Generate Rust code snippets for registry."""
    lines = []
    lines.append("// SHA256 checksums for dataset validation")
    lines.append("// Add these to the corresponding dataset blocks in dataset_registry.rs")
    lines.append("")
    
    for filename, data in sorted(checksums.items()):
        lines.append(f'// {filename}: sha256: "{data["sha256"]}",')
    
    return "\n".join(lines)


def main():
    mode = "compute"
    if "--update" in sys.argv:
        mode = "update"
    elif "--verify" in sys.argv:
        mode = "verify"
    
    print("=" * 60)
    print(f"Dataset Checksum Tool ({mode})")
    print("=" * 60)
    
    checksums = compute_all_checksums()
    
    if mode == "verify":
        matched, mismatched, missing = verify_checksums(checksums)
        print(f"\nVerification results:")
        print(f"  Matched:    {matched}")
        print(f"  Mismatched: {mismatched}")
        print(f"  Missing:    {missing}")
    elif mode == "update":
        update_manifest(checksums)
        print(f"\nManifest updated: {MANIFEST_PATH}")
    else:
        # Default: just save checksums
        with open(OUTPUT_FILE, "w") as f:
            json.dump(checksums, f, indent=2)
        print(f"\nChecksums saved to: {OUTPUT_FILE}")
        
        # Also generate Rust snippets
        rust_snippets = generate_rust_snippets(checksums)
        rust_file = OUTPUT_FILE.with_suffix(".rs")
        with open(rust_file, "w") as f:
            f.write(rust_snippets)
        print(f"Rust snippets saved to: {rust_file}")
    
    print("\n" + "=" * 60)
    print("Done!")


if __name__ == "__main__":
    main()


























