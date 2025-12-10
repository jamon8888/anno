#!/usr/bin/env python3
# /// script
# requires-python = ">=3.10"
# dependencies = [
#     "huggingface_hub>=0.20",
#     "torch>=2.0",
#     "safetensors>=0.4",
#     "rich>=13.0",
# ]
# ///
"""
Download Maverick coreference model weights and convert to safetensors.

Maverick (Martinelli et al. 2024) is a state-of-the-art coreference resolution
model that achieves 83.6 CoNLL-F1 on OntoNotes using only 500M parameters.

Available models:
    - maverick-mes-ontonotes: General coreference (news, web, etc.)
    - maverick-mes-litbank: Literary texts (novels, fiction)  
    - maverick-mes-preco: Large-scale coreference with singletons

Usage:
    uv run scripts/download_maverick_weights.py [--model MODEL] [--output-dir DIR]
    
Examples:
    # Download LitBank model (best for book-scale)
    uv run scripts/download_maverick_weights.py --model litbank
    
    # Download all models
    uv run scripts/download_maverick_weights.py --all

Output:
    Safetensors weights compatible with candle/burn backends.
"""

import argparse
import os
import sys
from pathlib import Path

try:
    import torch
    from huggingface_hub import hf_hub_download, list_repo_files
    from safetensors.torch import save_file
    from rich.console import Console
    from rich.progress import Progress, SpinnerColumn, BarColumn, TextColumn
except ImportError as e:
    print(f"Missing dependency: {e}")
    print("Run with: uv run scripts/download_maverick_weights.py")
    sys.exit(1)


console = Console()


MODELS = {
    "ontonotes": {
        "repo_id": "sapienzanlp/maverick-mes-ontonotes",
        "description": "General coreference (news, web, broadcast)",
        "score": "83.6 CoNLL-F1 on OntoNotes",
        "singletons": False,
    },
    "litbank": {
        "repo_id": "sapienzanlp/maverick-mes-litbank",
        "description": "Literary texts (novels, fiction)",
        "score": "78.0 CoNLL-F1 on LitBank",
        "singletons": True,
    },
    "preco": {
        "repo_id": "sapienzanlp/maverick-mes-preco",
        "description": "Large-scale with singletons",
        "score": "87.4 CoNLL-F1 on PreCo",
        "singletons": True,
    },
}


def get_cache_dir() -> Path:
    """Get anno cache directory."""
    if custom := os.environ.get("ANNO_CACHE_DIR"):
        return Path(custom)
    
    import platform
    if platform.system() == "Darwin":
        return Path.home() / "Library/Caches/anno"
    else:
        xdg_cache = os.environ.get("XDG_CACHE_HOME", str(Path.home() / ".cache"))
        return Path(xdg_cache) / "anno"


def convert_checkpoint_to_safetensors(ckpt_path: Path, output_path: Path) -> dict:
    """Convert PyTorch Lightning checkpoint to safetensors.
    
    Maverick uses PyTorch Lightning .ckpt format which contains:
    - state_dict: model weights
    - hyper_parameters: config
    - various training state
    
    We extract just the model weights for inference.
    """
    console.print(f"[dim]Loading checkpoint: {ckpt_path}[/]")
    
    # Load checkpoint (this is a pickle file)
    checkpoint = torch.load(ckpt_path, map_location="cpu", weights_only=False)
    
    # Extract state dict
    if "state_dict" in checkpoint:
        state_dict = checkpoint["state_dict"]
    else:
        # Might be a raw state dict
        state_dict = checkpoint
    
    # Clean up keys (remove 'model.' prefix if present)
    cleaned_state_dict = {}
    for key, value in state_dict.items():
        # Remove common prefixes from Lightning modules
        clean_key = key
        for prefix in ["model.", "encoder.", "_orig_mod."]:
            if clean_key.startswith(prefix):
                clean_key = clean_key[len(prefix):]
        
        # Only keep float tensors (skip buffers, etc.)
        if isinstance(value, torch.Tensor) and value.dtype in [torch.float32, torch.float16, torch.bfloat16]:
            # Convert to float32 for safetensors compatibility
            cleaned_state_dict[clean_key] = value.float()
    
    console.print(f"[dim]Saving {len(cleaned_state_dict)} tensors to safetensors[/]")
    
    # Save as safetensors
    save_file(cleaned_state_dict, output_path)
    
    # Return metadata
    return {
        "num_tensors": len(cleaned_state_dict),
        "total_params": sum(t.numel() for t in cleaned_state_dict.values()),
        "file_size_mb": output_path.stat().st_size / (1024 * 1024),
    }


def download_model(model_name: str, output_dir: Path) -> Path:
    """Download and convert a Maverick model.
    
    Args:
        model_name: One of 'ontonotes', 'litbank', 'preco'
        output_dir: Output directory
        
    Returns:
        Path to safetensors file
    """
    if model_name not in MODELS:
        raise ValueError(f"Unknown model: {model_name}. Choose from: {list(MODELS.keys())}")
    
    info = MODELS[model_name]
    repo_id = info["repo_id"]
    
    output_dir = output_dir / "maverick"
    output_dir.mkdir(parents=True, exist_ok=True)
    
    console.print(f"\n[bold blue]Downloading {model_name}...[/]")
    console.print(f"[dim]{info['description']}[/]")
    console.print(f"[dim]{info['score']}[/]")
    
    # List files in repo
    files = list_repo_files(repo_id)
    
    # Find the checkpoint file
    ckpt_file = None
    for f in files:
        if f.endswith(".ckpt"):
            ckpt_file = f
            break
    
    if not ckpt_file:
        raise ValueError(f"No .ckpt file found in {repo_id}")
    
    with Progress(
        SpinnerColumn(),
        TextColumn("[progress.description]{task.description}"),
        BarColumn(),
        console=console,
    ) as progress:
        task = progress.add_task("Downloading...", total=None)
        
        # Download checkpoint
        ckpt_path = Path(hf_hub_download(
            repo_id=repo_id,
            filename=ckpt_file,
            cache_dir=output_dir / ".cache",
        ))
        
        progress.update(task, description="Converting to safetensors...")
        
        # Convert to safetensors
        output_file = output_dir / f"maverick-{model_name}.safetensors"
        stats = convert_checkpoint_to_safetensors(ckpt_path, output_file)
        
        progress.update(task, description="Done!")
    
    console.print(f"[green]Saved: {output_file}[/]")
    console.print(f"[dim]  {stats['num_tensors']} tensors, {stats['total_params']:,} params, {stats['file_size_mb']:.1f} MB[/]")
    
    # Also download config if available
    for config_file in ["config.yaml", "config.json"]:
        if config_file in files:
            config_path = Path(hf_hub_download(
                repo_id=repo_id,
                filename=config_file,
                cache_dir=output_dir / ".cache",
            ))
            # Copy to output dir
            import shutil
            shutil.copy(config_path, output_dir / f"maverick-{model_name}-config.yaml")
            console.print(f"[dim]  Config: {config_file}[/]")
            break
    
    return output_file


def main():
    parser = argparse.ArgumentParser(
        description="Download Maverick coreference model weights",
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    parser.add_argument(
        "--model",
        choices=list(MODELS.keys()),
        help="Model to download",
    )
    parser.add_argument(
        "--all",
        action="store_true",
        help="Download all models",
    )
    parser.add_argument(
        "--output-dir",
        type=Path,
        default=None,
        help="Output directory (default: anno cache/models)",
    )
    parser.add_argument(
        "--list",
        action="store_true",
        help="List available models",
    )
    
    args = parser.parse_args()
    
    if args.list:
        console.print("\n[bold]Available Maverick models:[/]\n")
        for name, info in MODELS.items():
            console.print(f"  [green]{name}[/]")
            console.print(f"    {info['description']}")
            console.print(f"    {info['score']}")
            console.print(f"    Singletons: {'Yes' if info['singletons'] else 'No'}")
            console.print()
        return
    
    if not args.model and not args.all:
        parser.print_help()
        console.print("\n[yellow]Specify --model NAME or --all[/]")
        sys.exit(1)
    
    output_dir = args.output_dir or (get_cache_dir() / "models")
    
    models_to_download = list(MODELS.keys()) if args.all else [args.model]
    
    console.print("[bold]Maverick Coreference Model Download[/]")
    console.print(f"[dim]Output: {output_dir}[/]")
    
    try:
        for model_name in models_to_download:
            download_model(model_name, output_dir)
        
        console.print("\n[bold green]All downloads complete![/]")
        
    except KeyboardInterrupt:
        console.print("\n[yellow]Download cancelled.[/]")
        sys.exit(1)
    except Exception as e:
        console.print(f"\n[red]Error: {e}[/]")
        import traceback
        traceback.print_exc()
        sys.exit(1)


if __name__ == "__main__":
    main()

