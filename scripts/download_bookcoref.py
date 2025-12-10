#!/usr/bin/env python3
# /// script
# requires-python = ">=3.10"
# dependencies = [
#     "huggingface_hub>=0.20",
#     "rich>=13.0",
# ]
# ///
"""
Download BOOKCOREF dataset annotations for anno.

BOOKCOREF (Martinelli et al. 2025) is the first book-scale coreference benchmark.

IMPORTANT: This script downloads annotations only. The full dataset with text
requires running the official BOOKCOREF preprocessing pipeline (needs spacy, 
nltk, deepdiff, and downloads from Project Gutenberg via Wayback Machine).

For full dataset with text:
    git clone https://github.com/SapienzaNLP/bookcoref
    cd bookcoref && pip install -r requirements.txt
    python download_data.py --format jsonl

Usage:
    uv run scripts/download_bookcoref.py [--config full|split] [--output-dir DIR]
    
Examples:
    # Download full book annotations (recommended for training)
    uv run scripts/download_bookcoref.py --config full
    
    # Download 1500-token windowed version
    uv run scripts/download_bookcoref.py --config split

Output format (JSONL, one JSON per line):
    {
        "doc_key": "pride_and_prejudice_1342",
        "gutenberg_key": "1342",
        "clusters": [[[79,80], [81,82], ...], ...],
        "characters": [{"name": "Mr Bennet", "cluster": [[79,80], ...]}]
    }

Note: The 'sentences' field is only present if you run the official
preprocessing script or download from the full pipeline.
"""

import argparse
import json
import os
import sys
from pathlib import Path

try:
    from huggingface_hub import hf_hub_download, list_repo_files
    from rich.console import Console
    from rich.progress import Progress, SpinnerColumn, TextColumn, BarColumn, TaskProgressColumn
except ImportError as e:
    print(f"Missing dependency: {e}")
    print("Run with: uv run scripts/download_bookcoref.py")
    sys.exit(1)


console = Console()

REPO_ID = "sapienzanlp/bookcoref"
ANNOTATIONS_PATH = "bookcoref_annotations"


def get_cache_dir() -> Path:
    """Get anno cache directory (matches Rust logic)."""
    if custom := os.environ.get("ANNO_CACHE_DIR"):
        return Path(custom)
    
    import platform
    if platform.system() == "Darwin":
        return Path.home() / "Library/Caches/anno"
    else:
        xdg_cache = os.environ.get("XDG_CACHE_HOME", str(Path.home() / ".cache"))
        return Path(xdg_cache) / "anno"


def download_bookcoref(
    config: str = "full",
    output_dir: Path | None = None,
) -> Path:
    """Download BOOKCOREF annotations from HuggingFace.
    
    Args:
        config: "full" for full books, "split" for 1500-token windows
        output_dir: Output directory (defaults to anno cache)
        
    Returns:
        Path to output directory
    """
    if output_dir is None:
        output_dir = get_cache_dir() / "datasets" / "bookcoref" / config
    output_dir.mkdir(parents=True, exist_ok=True)
    
    console.print(f"[bold blue]Downloading BOOKCOREF annotations ({config})...[/]")
    console.print("[dim]Note: This downloads annotations only, not full text.[/]")
    console.print("[dim]For full dataset, use: github.com/SapienzaNLP/bookcoref[/]")
    
    # Map config to directory
    config_dir = config if config == "full" else "split"
    
    # List files in the annotations directory
    with Progress(
        SpinnerColumn(),
        TextColumn("[progress.description]{task.description}"),
        console=console,
    ) as progress:
        task = progress.add_task("Listing files...", total=None)
        
        try:
            all_files = list_repo_files(REPO_ID, repo_type="dataset")
            
            # Find JSONL files for the config
            annotation_prefix = f"{ANNOTATIONS_PATH}/{config_dir}/"
            jsonl_files = [f for f in all_files if f.startswith(annotation_prefix) and f.endswith(".jsonl")]
            
            if not jsonl_files:
                console.print(f"[red]No JSONL files found in {annotation_prefix}[/]")
                raise ValueError(f"No JSONL files found for config={config}")
                
        except Exception as e:
            console.print(f"[red]Error listing files: {e}[/]")
            raise
    
    console.print(f"[green]Found {len(jsonl_files)} annotation files[/]")
    
    # Download each annotation file
    stats = {}
    with Progress(
        SpinnerColumn(),
        TextColumn("[progress.description]{task.description}"),
        BarColumn(),
        TaskProgressColumn(),
        console=console,
    ) as progress:
        task = progress.add_task("Downloading...", total=len(jsonl_files))
        
        for file_path in jsonl_files:
            filename = Path(file_path).name
            # Clean up split name
            split_name = filename.replace(".jsonl", "")
            if split_name.endswith("_split"):
                split_name = split_name.replace("_split", "")
            
            progress.update(task, description=f"Downloading {filename}...")
            
            try:
                local_path = hf_hub_download(
                    repo_id=REPO_ID,
                    filename=file_path,
                    repo_type="dataset",
                    local_dir=output_dir / ".hf_cache",
                )
                
                # Process and write output with clean filename
                output_file = output_dir / f"{split_name}.jsonl"
                doc_count = 0
                total_mentions = 0
                total_characters = 0
                
                with open(local_path, "r", encoding="utf-8") as f_in, \
                     open(output_file, "w", encoding="utf-8") as f_out:
                    for line in f_in:
                        if not line.strip():
                            continue
                        
                        doc = json.loads(line)
                        doc_count += 1
                        
                        # Count mentions and characters
                        for cluster in doc.get("clusters", []):
                            total_mentions += len(cluster)
                        total_characters += len(doc.get("characters", []))
                        
                        f_out.write(json.dumps(doc, ensure_ascii=False) + "\n")
                
                stats[split_name] = {
                    "docs": doc_count,
                    "total_mentions": total_mentions,
                    "total_characters": total_characters,
                    "output_file": str(output_file),
                }
                
            except Exception as e:
                console.print(f"[yellow]Warning: Failed to download {filename}: {e}[/]")
            
            progress.advance(task)
    
    # Write metadata
    meta_file = output_dir / "metadata.json"
    metadata = {
        "dataset": "BOOKCOREF",
        "config": config,
        "type": "annotations_only",
        "citation": "Martinelli et al. (2025)",
        "paper": "https://aclanthology.org/2025.acl-long.1197/",
        "github": "https://github.com/SapienzaNLP/bookcoref",
        "hf_repo": REPO_ID,
        "license": "CC-BY-NC-SA-4.0",
        "note": "Annotations only. For full text, use official download_data.py",
        "splits": stats,
    }
    with open(meta_file, "w") as f:
        json.dump(metadata, f, indent=2)
    
    # Clean up HF cache
    hf_cache = output_dir / ".hf_cache"
    if hf_cache.exists():
        import shutil
        shutil.rmtree(hf_cache)
    
    console.print()
    console.print("[bold green]Download complete![/]")
    console.print(f"[dim]Output: {output_dir}[/]")
    
    for split_name, s in stats.items():
        console.print(
            f"  {split_name}: {s['docs']} docs, "
            f"{s['total_mentions']:,} mentions, "
            f"{s['total_characters']:,} characters"
        )
    
    console.print()
    console.print("[yellow]Note: Downloaded annotations only.[/]")
    console.print("[dim]For full dataset with tokenized text:[/]")
    console.print("[dim]  git clone https://github.com/SapienzaNLP/bookcoref[/]")
    console.print("[dim]  cd bookcoref && pip install -r requirements.txt[/]")
    console.print("[dim]  python download_data.py --format jsonl[/]")
    
    return output_dir


def main():
    parser = argparse.ArgumentParser(
        description="Download BOOKCOREF dataset annotations for anno",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog=__doc__.split("Usage:")[1] if "Usage:" in __doc__ else "",
    )
    parser.add_argument(
        "--config",
        choices=["full", "split"],
        default="full",
        help="Dataset configuration: 'full' for complete books, 'split' for 1500-token windows",
    )
    parser.add_argument(
        "--output-dir",
        type=Path,
        help="Output directory (default: anno cache)",
    )
    
    args = parser.parse_args()
    
    try:
        download_bookcoref(
            config=args.config,
            output_dir=args.output_dir,
        )
    except KeyboardInterrupt:
        console.print("\n[yellow]Download cancelled.[/]")
        sys.exit(1)
    except Exception as e:
        console.print(f"\n[red]Error: {e}[/]")
        sys.exit(1)


if __name__ == "__main__":
    main()
