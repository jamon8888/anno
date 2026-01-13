#!/usr/bin/env python3
# /// script
# requires-python = ">=3.10"
# dependencies = ["boto3>=1.34", "rich>=13.0"]
# ///
"""
Orchestrator for distributed spot evaluation using runctl.

Uses runctl for instance lifecycle management while keeping SQS for task distribution.
This provides the best of both worlds:
- runctl's robust instance management, SSM sync, resource tracking
- SQS-based task distribution for parallel evaluation

Usage:
    uv run scripts/spot/orchestrate_runctl.py generate   # Generate tasks for queue
    uv run scripts/spot/orchestrate_runctl.py launch     # Launch spot instances via runctl
    uv run scripts/spot/orchestrate_runctl.py status     # Check progress
    uv run scripts/spot/orchestrate_runctl.py results    # Aggregate results
    uv run scripts/spot/orchestrate_runctl.py teardown   # Clean up
    uv run scripts/spot/orchestrate_runctl.py full       # Generate + launch + wait + results
"""

import argparse
import json
import os
import subprocess
import sys
import time
from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path
from typing import Iterator

import boto3
from rich.console import Console
from rich.progress import Progress, SpinnerColumn, TextColumn
from rich.table import Table

console = Console()

# ============================================================================
# Configuration
# ============================================================================

@dataclass
class Config:
    region: str = "us-east-1"
    bucket: str = "arc-anno-data"
    queue_name: str = "anno-eval-tasks"
    queue_url: str = ""
    instance_type: str = "c7i.xlarge"
    fleet_size: int = 4
    runctl_bin: str = "runctl"
    
    def __post_init__(self):
        # Load from environment
        self.region = os.environ.get("ANNO_SPOT_REGION", self.region)
        self.bucket = os.environ.get("ANNO_SPOT_BUCKET", self.bucket)
        self.queue_name = os.environ.get("ANNO_SPOT_QUEUE", self.queue_name)
        self.fleet_size = int(os.environ.get("ANNO_SPOT_FLEET_SIZE", self.fleet_size))
        self.instance_type = os.environ.get("ANNO_SPOT_INSTANCE_TYPE", self.instance_type)
        
        # Find runctl binary
        if not self.runctl_bin or not self._check_runctl():
            # Try to find runctl in PATH or ../runctl
            for path in [
                "runctl",
                str(Path(__file__).parent.parent.parent / "runctl" / "target" / "release" / "runctl"),
                str(Path(__file__).parent.parent.parent / "runctl" / "target" / "debug" / "runctl"),
            ]:
                if self._check_runctl_at(path):
                    self.runctl_bin = path
                    break
            else:
                console.print("[red]ERROR: runctl not found. Install with: cargo install --path ../runctl[/red]")
                sys.exit(1)
    
    def _check_runctl(self) -> bool:
        """Check if runctl binary exists and works."""
        try:
            result = subprocess.run(
                [self.runctl_bin, "--version"],
                capture_output=True,
                timeout=5
            )
            return result.returncode == 0
        except (FileNotFoundError, subprocess.TimeoutExpired):
            return False
    
    def _check_runctl_at(self, path: str) -> bool:
        """Check if runctl exists at specific path."""
        try:
            result = subprocess.run(
                [path, "--version"],
                capture_output=True,
                timeout=5
            )
            return result.returncode == 0
        except (FileNotFoundError, subprocess.TimeoutExpired):
            return False


# ============================================================================
# Task Generation (same as original)
# ============================================================================

BACKENDS_ZERO_DEP = ["heuristic", "stacked"]
BACKENDS_ONNX = ["gliner", "gliner2", "nuner"]
BACKENDS_CANDLE = []
BACKENDS = BACKENDS_ZERO_DEP + BACKENDS_ONNX + BACKENDS_CANDLE

DATASETS = [
    "WikiGold", "Wnut17", "MitMovie", "MitRestaurant", "CoNLL2003Sample",
    "BC5CDR", "NCBIDisease", "MultiNERD", "FewNERD",
    "GAP", "PreCo", "LitBank",
]

DEFAULT_SEEDS = [42, 123, 456, 789, 999]

def generate_tasks(
    backends: list[str] | None = None,
    datasets: list[str] | None = None,
    seeds: list[int] | None = None,
    max_examples: int = 50,
) -> Iterator[dict]:
    """Generate (backend, dataset, seed) task combinations."""
    backends = backends or BACKENDS
    datasets = datasets or DATASETS
    seeds = seeds or DEFAULT_SEEDS
    
    for backend in backends:
        for dataset in datasets:
            for seed in seeds:
                yield {
                    "backend": backend,
                    "dataset": dataset,
                    "seed": seed,
                    "max_examples": max_examples,
                }

def enqueue_tasks(config: Config, tasks: list[dict]) -> int:
    """Send tasks to SQS queue."""
    sqs = boto3.client("sqs", region_name=config.region)
    
    # Get queue URL if not cached
    if not config.queue_url:
        resp = sqs.get_queue_url(QueueName=config.queue_name)
        config.queue_url = resp["QueueUrl"]
    
    # Send in batches of 10 (SQS limit)
    sent = 0
    for i in range(0, len(tasks), 10):
        batch = tasks[i:i+10]
        entries = [
            {
                "Id": str(j),
                "MessageBody": json.dumps(task),
                "MessageGroupId": task["backend"] if config.queue_name.endswith(".fifo") else None,
            }
            for j, task in enumerate(batch)
        ]
        # Remove None MessageGroupId for standard queues
        entries = [{k: v for k, v in e.items() if v is not None} for e in entries]
        sqs.send_message_batch(QueueUrl=config.queue_url, Entries=entries)
        sent += len(batch)
    
    return sent

def cmd_generate(args, config: Config):
    """Generate and enqueue evaluation tasks."""
    tasks = list(generate_tasks(
        backends=args.backends,
        datasets=args.datasets,
        seeds=args.seeds,
        max_examples=args.max_examples,
    ))
    
    if args.dry_run:
        console.print(f"[yellow]Would generate {len(tasks)} tasks:[/yellow]")
        for task in tasks[:10]:
            console.print(f"  {task['backend']}/{task['dataset']}/seed={task['seed']}")
        if len(tasks) > 10:
            console.print(f"  ... and {len(tasks) - 10} more")
        return
    
    sent = enqueue_tasks(config, tasks)
    console.print(f"[green]Enqueued {sent} tasks to {config.queue_name}[/green]")


# ============================================================================
# Instance Management (using runctl)
# ============================================================================

def create_spot_instance_via_runctl(config: Config, instance_type: str) -> str | None:
    """Create a spot instance using runctl and return instance ID."""
    try:
        # Build command with IAM instance profile if available
        cmd = [
            config.runctl_bin,
            "aws",
            "create",
            instance_type,
            "--spot",
            "--wait",
            "--output", "instance-id",
        ]
        
        # Add IAM instance profile if we have one (check for anno-eval-worker-profile or anno-eval-spot-profile)
        # Try to find an appropriate profile
        import boto3
        try:
            iam = boto3.client("iam", region_name=config.region)
            profiles = ["anno-eval-worker-profile", "anno-eval-spot-profile", "runctl-ssm-profile"]
            for profile_name in profiles:
                try:
                    iam.get_instance_profile(InstanceProfileName=profile_name)
                    cmd.extend(["--iam-instance-profile", profile_name])
                    console.print(f"  [cyan]Using IAM profile: {profile_name}[/cyan]")
                    break
                except iam.exceptions.NoSuchEntityException:
                    continue
        except Exception:
            pass  # If IAM check fails, continue without profile
        
        result = subprocess.run(
            cmd,
            capture_output=True,
            text=True,
            timeout=600,  # 10 min timeout (spot requests can take longer)
            env={**os.environ, "AWS_DEFAULT_REGION": config.region},
        )
        
        if result.returncode == 0:
            # Extract instance ID from output (may have warnings before it)
            # Look for line starting with "i-"
            instance_id = None
            for line in result.stdout.split('\n'):
                line = line.strip()
                if line.startswith('i-') and len(line) > 10:  # Valid instance ID format
                    instance_id = line
                    break
            
            if instance_id:
                return instance_id
            else:
                console.print(f"[red]Could not extract instance ID from runctl output[/red]")
                console.print(f"[yellow]Output: {result.stdout[:200]}...[/yellow]")
                return None
        else:
            console.print(f"[red]Failed to create instance[/red]")
            if result.stderr:
                console.print(f"[red]Error: {result.stderr}[/red]")
            if result.stdout:
                console.print(f"[yellow]Output: {result.stdout}[/yellow]")
            return None
    except subprocess.TimeoutExpired:
        console.print("[red]Instance creation timed out[/red]")
        return None
    except Exception as e:
        console.print(f"[red]Error creating instance: {e}[/red]")
        return None

def list_runctl_instances(config: Config) -> list[dict]:
    """List instances managed by runctl."""
    try:
        result = subprocess.run(
            [
                config.runctl_bin,
                "resources",
                "list",
                "--platform", "aws",
                "--output", "json",
            ],
            capture_output=True,
            text=True,
            timeout=30,
            env={**os.environ, "AWS_DEFAULT_REGION": config.region},
        )
        
        if result.returncode == 0:
            try:
                data = json.loads(result.stdout)
                return data.get("resources", [])
            except json.JSONDecodeError:
                return []
        return []
    except Exception:
        return []

def start_worker_via_ssm(config: Config, instance_id: str) -> bool:
    """Start worker script on instance via SSM.
    
    For runctl-created instances, we:
    1. First try runctl's code sync (if available) to upload the repo
    2. Otherwise, clone and build via SSM (requires GitHub auth for private repos)
    3. Then build and start the worker
    """
    import boto3
    import subprocess
    import os
    
    ssm = boto3.client("ssm", region_name=config.region)
    
    # Use direct S3 tarball sync (simpler than runctl train approach)
    # This mirrors runctl's SSM sync: create tar.gz, upload to S3, download via SSM
    console.print(f"  Syncing code via S3 tarball (SSM-based)...")
    code_synced = False
    try:
        import tarfile
        import tempfile
        import uuid
        
        workspace_root = os.path.abspath(os.path.join(os.path.dirname(__file__), "../.."))
        s3 = boto3.client("s3", region_name=config.region)
        ssm = boto3.client("ssm", region_name=config.region)
        
        # Step 1: Create tar.gz archive
        console.print(f"  Creating code archive...")
        temp_archive = os.path.join(tempfile.gettempdir(), f"anno-code-{uuid.uuid4().hex[:8]}.tar.gz")
        
        with tarfile.open(temp_archive, "w:gz") as tar:
            # Add all files except build artifacts, caches, and large generated files
            exclude_extensions = {".pyc", ".pyo", ".pyd", ".so", ".dylib", ".dll", ".exe", ".o", ".a"}
            exclude_dirs = {".git", "target", "node_modules", "__pycache__", ".pytest_cache", 
                           ".mypy_cache", "checkpoints", "logs", "reports", "archive", "models", 
                           "data", ".venv", "venv", ".idea", ".vscode", "dist", "build"}
            # Note: benches/ is needed (Cargo.toml references it), but we can exclude large test data
            # Allow these hidden files
            allowed_hidden = {".gitignore", ".gitattributes", ".cursorignore", ".github", ".cargo"}
            
            for root, dirs, files in os.walk(workspace_root):
                # Get relative path components for this directory
                rel_root = os.path.relpath(root, workspace_root)
                rel_parts = rel_root.split(os.sep) if rel_root != "." else []
                
                # Filter out excluded directories
                dirs[:] = [d for d in dirs 
                          if d not in exclude_dirs 
                          and not any(d.startswith(excl.rstrip("*")) for excl in exclude_dirs)
                          and not (d.startswith(".") and d not in allowed_hidden)]
                
                for file in files:
                    # Skip hidden files except allowed ones
                    if file.startswith(".") and file not in allowed_hidden:
                        continue
                    # Skip excluded extensions
                    if any(file.endswith(ext) for ext in exclude_extensions):
                        continue
                    file_path = os.path.join(root, file)
                    rel_path = os.path.relpath(file_path, workspace_root)
                    # Skip if any path component is excluded
                    if any(part in exclude_dirs for part in rel_parts + [file]):
                        continue
                    try:
                        tar.add(file_path, arcname=rel_path)
                    except Exception as e:
                        # Skip files that can't be added (permissions, etc.)
                        console.print(f"  [yellow]Skipping {rel_path}: {e}[/yellow]")
                        continue
        
        archive_size_mb = os.path.getsize(temp_archive) / (1024 * 1024)
        console.print(f"  Archive created: {archive_size_mb:.1f} MB")
        
        # Step 2: Upload to S3
        console.print(f"  Uploading to S3...")
        s3_key = f"runctl-temp/{instance_id}/{uuid.uuid4()}.tar.gz"
        s3_path = f"s3://{config.bucket}/{s3_key}"
        
        s3.upload_file(temp_archive, config.bucket, s3_key)
        console.print(f"  Uploaded to {s3_path}")
        
        # Step 3: Download and extract on instance via SSM
        console.print(f"  Downloading and extracting on instance...")
        download_cmd = f"""cd /root && \
mkdir -p anno && \
cd anno && \
echo 'Downloading code archive from S3...' && \
aws s3 cp {s3_path} code.tar.gz && \
echo 'Extracting archive...' && \
tar -xzf code.tar.gz && \
echo 'Cleaning up...' && \
rm -f code.tar.gz && \
echo 'Code sync complete' && \
ls -la | head -10"""
        
        # Send command and wait for completion
        resp = ssm.send_command(
            InstanceIds=[instance_id],
            DocumentName="AWS-RunShellScript",
            Parameters={"commands": [download_cmd]},
        )
        command_id = resp["Command"]["CommandId"]
        
        # Wait for command to complete (up to 5 minutes)
        console.print(f"  Waiting for extraction to complete...")
        import time
        for i in range(60):  # 60 * 5s = 5 minutes
            time.sleep(5)
            try:
                invoc = ssm.get_command_invocation(
                    CommandId=command_id,
                    InstanceId=instance_id
                )
                status = invoc.get("Status")
                if status in ["Success", "Failed", "Cancelled", "TimedOut"]:
                    if status == "Success":
                        output = invoc.get("StandardOutputContent", "")
                        if "Code sync complete" in output or "Cargo.toml exists" in output:
                            console.print(f"  [green]Code sync completed successfully[/green]")
                            code_synced = True
                        else:
                            console.print(f"  [yellow]Code sync completed but verification unclear[/yellow]")
                            code_synced = True  # Assume success, will verify later
                    else:
                        error = invoc.get("StandardErrorContent", "")
                        console.print(f"  [yellow]Code sync command {status.lower()}: {error[:200]}[/yellow]")
                    break
            except Exception as e:
                if "does not exist" in str(e) or "not found" in str(e):
                    # Command not ready yet, continue waiting
                    continue
                else:
                    console.print(f"  [yellow]Error checking command status: {e}[/yellow]")
                    break
        
        # Clean up local archive
        try:
            os.remove(temp_archive)
        except:
            pass
        
        # Clean up S3 file (best effort, after sync completes)
        if code_synced:
            try:
                s3.delete_object(Bucket=config.bucket, Key=s3_key)
            except:
                pass
    except Exception as e:
        console.print(f"  [yellow]S3 code sync failed: {e}[/yellow]")
        code_synced = False
    
    # Wait for SSM to be ready (up to 3 minutes for spot instances)
    console.print(f"  Waiting for SSM on {instance_id}...")
    ssm_ready = False
    for attempt in range(36):  # 36 * 5s = 3 minutes (spot instances may take longer)
        try:
            resp = ssm.describe_instance_information(
                Filters=[{"Key": "InstanceIds", "Values": [instance_id]}]
            )
            if resp.get("InstanceInformationList"):
                info = resp["InstanceInformationList"][0]
                if info.get("PingStatus") == "Online":
                    ssm_ready = True
                    console.print(f"  [green]SSM ready after {attempt * 5}s[/green]")
                    break
        except Exception:
            pass
        if attempt % 6 == 0 and attempt > 0:  # Print every 30s
            console.print(f"  [yellow]Still waiting... ({attempt * 5}s)[/yellow]")
        time.sleep(5)
    
    if not ssm_ready:
        console.print(f"[yellow]  Warning: SSM not ready on {instance_id} after 3 minutes[/yellow]")
        console.print(f"  Worker will need to be started manually or via user-data")
        console.print(f"  Check IAM instance profile is attached and has SSM permissions")
        return False
    
    # Get queue URL
    sqs = boto3.client("sqs", region_name=config.region)
    try:
        queue_resp = sqs.get_queue_url(QueueName=config.queue_name)
        queue_url = queue_resp["QueueUrl"]
    except Exception:
        queue_url = ""
    
    # Setup script: clone repo, build, start worker
    # This is a simplified version - in production, you'd want to use runctl's code sync
    setup_script = f"""#!/bin/bash
set -e
export HOME=/root
export ANNO_SPOT_REGION={config.region}
export ANNO_SPOT_BUCKET={config.bucket}
export ANNO_SPOT_QUEUE={config.queue_name}
export ANNO_SPOT_QUEUE_URL={queue_url}
export ANNO_CACHE_MOUNT=/opt/cache
export ANNO_SOURCE_DIR=/root/anno
export CARGO_TARGET_DIR=/opt/cache/target
export SCCACHE_DIR=/opt/cache/sccache

# Install system dependencies (Amazon Linux 2023 / Ubuntu)
if command -v dnf &>/dev/null; then
    dnf install -y git gcc gcc-c++ openssl-devel pkg-config jq cmake || true
elif command -v yum &>/dev/null; then
    yum install -y git gcc gcc-c++ openssl-devel pkg-config jq cmake || true
elif command -v apt-get &>/dev/null; then
    apt-get update -y && apt-get install -y git build-essential libssl-dev pkg-config jq cmake || true
fi

       # Install Rust if needed
       if ! command -v cargo &>/dev/null; then
           echo "Installing Rust..."
           curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
           # Source cargo env and add to PATH
           if [ -f "$HOME/.cargo/env" ]; then
               source "$HOME/.cargo/env"
           fi
           export PATH="$HOME/.cargo/bin:$PATH"
           # Verify installation
           if command -v cargo &>/dev/null; then
               echo "Rust installed: $(cargo --version)"
           else
               echo "ERROR: Rust installation failed"
               exit 1
           fi
       else
           echo "Rust already installed: $(cargo --version)"
       fi
       
       # Ensure cargo is in PATH for subsequent commands
       export PATH="$HOME/.cargo/bin:$PATH"

# Clone anno if not exists (handle git auth issues)
# If runctl SSM code sync was used, the repo should already be there
# Otherwise, try git clone with available auth methods
if [[ ! -d /root/anno ]]; then
    # Option 1: Try git clone with token from environment
    if [[ -n "$GITHUB_TOKEN" ]]; then
        echo "Cloning with GitHub token..."
        export GIT_TERMINAL_PROMPT=0
        git config --global credential.helper "" || true
        git clone --depth 1 "https://$GITHUB_TOKEN@github.com/arclabs561/anno.git" /root/anno || exit 1
    # Option 2: Try public clone (will fail for private repos, but worth trying)
    else
        echo "Attempting git clone (may fail for private repos without auth)..."
        export GIT_TERMINAL_PROMPT=0
        git config --global credential.helper "" || true
        git clone --depth 1 https://github.com/arclabs561/anno.git /root/anno 2>&1 || {{
            echo "ERROR: Git clone failed. For private repos, use one of:"
            echo "  1. runctl code sync (preferred - done automatically if available)"
            echo "  2. Set GITHUB_TOKEN environment variable"
            echo "  3. Configure SSH keys for git@github.com access"
            exit 1
        }}
    fi
fi

       # Build anno if not exists
       cd /root/anno || exit 1
       if [[ ! -f /opt/cache/target/release/anno ]]; then
           echo "Building anno (this may take 5-10 minutes)..."
           # Ensure cargo is in PATH
           export PATH="$HOME/.cargo/bin:$PATH"
           # Source cargo env if available
           if [ -f "$HOME/.cargo/env" ]; then
               source "$HOME/.cargo/env"
           fi
           # Build with full output to log, then show tail
           cargo build --release --bin anno --features "cli,eval-advanced,onnx" 2>&1 | tee /tmp/build.log
           BUILD_EXIT=${PIPESTATUS[0]}
           if [[ $BUILD_EXIT -ne 0 ]]; then
               echo "Build failed with exit code $BUILD_EXIT"
               echo "Last 50 lines of build log:"
               tail -50 /tmp/build.log
               exit 1
           fi
           if [[ ! -f /opt/cache/target/release/anno ]]; then
               echo "Build completed but binary not found"
               echo "Last 50 lines of build log:"
               tail -50 /tmp/build.log
               exit 1
           fi
           echo "Build complete: $(ls -lh /opt/cache/target/release/anno)"
       else
           echo "Binary already exists: $(ls -lh /opt/cache/target/release/anno)"
       fi

# Get queue URL if not set
if [[ -z "$ANNO_SPOT_QUEUE_URL" ]]; then
    export ANNO_SPOT_QUEUE_URL=$(aws sqs get-queue-url --queue-name anno-eval-tasks --region us-east-1 --query QueueUrl --output text 2>&1 || echo "")
fi

# Start worker in background
cd /root/anno
nohup bash scripts/spot/worker.sh > /tmp/worker.log 2>&1 &
WORKER_PID=$!
echo "Worker started with PID: $WORKER_PID"
sleep 2
if ps -p $WORKER_PID > /dev/null; then
    echo "Worker is running (PID: $WORKER_PID)"
else
    echo "Worker failed to start, check /tmp/worker.log:"
    tail -20 /tmp/worker.log 2>/dev/null || echo "No log file"
fi
"""
    
    try:
        resp = ssm.send_command(
            InstanceIds=[instance_id],
            DocumentName="AWS-RunShellScript",
            Parameters={"commands": [setup_script]},
        )
        command_id = resp["Command"]["CommandId"]
        console.print(f"  [cyan]Worker setup started (command: {command_id})[/cyan]")
        
        # Don't wait for completion - it runs in background
        return True
    except Exception as e:
        console.print(f"[red]  Failed to start worker: {e}[/red]")
        return False

def cmd_launch(args, config: Config):
    """Launch spot instances using runctl and start workers."""
    fleet_size = args.fleet_size or config.fleet_size
    instance_type = args.instance_type or config.instance_type
    
    console.print(f"[bold]Launching {fleet_size} spot instances via runctl...[/bold]")
    console.print(f"  Instance type: {instance_type}")
    console.print(f"  Region: {config.region}")
    console.print(f"  Note: Spot availability is better for larger instances (c7i.xlarge recommended)")
    console.print()
    
    instance_ids = []
    with Progress(
        SpinnerColumn(),
        TextColumn("[progress.description]{task.description}"),
        console=console,
    ) as progress:
        task = progress.add_task(f"Creating {fleet_size} instances...", total=fleet_size)
        
        for i in range(fleet_size):
            progress.update(task, description=f"Creating instance {i+1}/{fleet_size}...")
            instance_id = create_spot_instance_via_runctl(config, instance_type)
            if instance_id:
                instance_ids.append(instance_id)
                console.print(f"[green]✓ Instance {i+1}: {instance_id}[/green]")
                
                # Start worker on instance
                if start_worker_via_ssm(config, instance_id):
                    console.print(f"  [green]Worker started[/green]")
                else:
                    console.print(f"  [yellow]Worker start delayed (SSM not ready)[/yellow]")
            else:
                console.print(f"[red]✗ Failed to create instance {i+1}[/red]")
            progress.advance(task)
    
    # Save instance IDs
    instance_file = Path(__file__).parent / "instances.txt"
    instance_file.write_text("\n".join(instance_ids))
    
    console.print()
    console.print(f"[green]Launched {len(instance_ids)}/{fleet_size} instances[/green]")
    if instance_ids:
        console.print(f"Instance IDs saved to: {instance_file}")
        console.print(f"Monitor with: {config.runctl_bin} resources list --platform aws")
        console.print(f"Check worker logs: aws ssm send-command --instance-ids <id> --document-name AWS-RunShellScript --parameters 'commands=[\"tail -50 /tmp/worker.log\"]'")

def cmd_status(args, config: Config):
    """Check evaluation progress."""
    sqs = boto3.client("sqs", region_name=config.region)
    ec2 = boto3.client("ec2", region_name=config.region)
    
    # Queue status
    if not config.queue_url:
        try:
            resp = sqs.get_queue_url(QueueName=config.queue_name)
            config.queue_url = resp["QueueUrl"]
        except Exception:
            config.queue_url = ""
    
    pending = 0
    in_flight = 0
    if config.queue_url:
        attrs = sqs.get_queue_attributes(
            QueueUrl=config.queue_url,
            AttributeNames=["ApproximateNumberOfMessages", "ApproximateNumberOfMessagesNotVisible"],
        )
        pending = int(attrs["Attributes"].get("ApproximateNumberOfMessages", 0))
        in_flight = int(attrs["Attributes"].get("ApproximateNumberOfMessagesNotVisible", 0))
    
    # Count results
    s3 = boto3.client("s3", region_name=config.region)
    results_count = 0
    try:
        paginator = s3.get_paginator("list_objects_v2")
        pages = paginator.paginate(Bucket=config.bucket, Prefix="results/")
        for page in pages:
            results_count += len(page.get("Contents", []))
    except Exception:
        pass
    
    # Count instances
    instances = list_runctl_instances(config)
    running_instances = [i for i in instances if i.get("status") == "running"]
    
    # Create status table
    table = Table(title="Evaluation Status")
    table.add_column("Metric", style="cyan")
    table.add_column("Value", style="magenta")
    
    table.add_row("Tasks Pending", str(pending))
    table.add_row("Tasks In-Flight", str(in_flight))
    table.add_row("Results Uploaded", str(results_count))
    table.add_row("Instances Running", str(len(running_instances)))
    
    total_tasks = pending + in_flight + results_count
    if total_tasks > 0:
        progress_pct = (results_count / total_tasks) * 100
        table.add_row("Progress", f"{progress_pct:.1f}%")
    
    console.print(table)

def cmd_teardown(args, config: Config):
    """Terminate instances and optionally clean up queue."""
    instance_file = Path(__file__).parent / "instances.txt"
    
    if instance_file.exists():
        instance_ids = [line.strip() for line in instance_file.read_text().splitlines() if line.strip()]
        
        console.print(f"[bold]Terminating {len(instance_ids)} instances...[/bold]")
        
        for instance_id in instance_ids:
            try:
                subprocess.run(
                    [config.runctl_bin, "aws", "terminate", instance_id, "--force"],
                    capture_output=True,
                    timeout=30,
                    env={**os.environ, "AWS_DEFAULT_REGION": config.region},
                )
                console.print(f"[green]✓ Terminated {instance_id}[/green]")
            except Exception as e:
                console.print(f"[red]✗ Failed to terminate {instance_id}: {e}[/red]")
        
        instance_file.unlink()
        console.print("[green]All instances terminated[/green]")
    else:
        console.print("[yellow]No instances file found[/yellow]")
    
    # Optionally purge queue
    if args.purge_queue:
        sqs = boto3.client("sqs", region_name=config.region)
        if not config.queue_url:
            try:
                resp = sqs.get_queue_url(QueueName=config.queue_name)
                config.queue_url = resp["QueueUrl"]
            except Exception:
                pass
        
        if config.queue_url:
            console.print(f"[bold]Purging queue: {config.queue_name}[/bold]")
            try:
                sqs.purge_queue(QueueUrl=config.queue_url)
                console.print("[green]Queue purged[/green]")
            except Exception as e:
                console.print(f"[red]Failed to purge queue: {e}[/red]")

def cmd_full(args, config: Config):
    """Run complete pipeline: generate → launch → wait → results."""
    # Generate tasks
    console.print("\n[bold blue]Step 1: Generating tasks...[/bold blue]")
    tasks = list(generate_tasks(
        backends=args.backends,
        datasets=args.datasets,
        seeds=args.seeds,
        max_examples=args.max_examples,
    ))
    sent = enqueue_tasks(config, tasks)
    console.print(f"[green]Enqueued {sent} tasks[/green]")
    
    # Launch instances
    console.print("\n[bold blue]Step 2: Launching instances...[/bold blue]")
    fleet_size = args.fleet_size or config.fleet_size
    instance_type = args.instance_type or config.instance_type
    
    instance_ids = []
    for i in range(fleet_size):
        instance_id = create_spot_instance_via_runctl(config, instance_type)
        if instance_id:
            instance_ids.append(instance_id)
            # Start worker on instance
            start_worker_via_ssm(config, instance_id)
    
    instance_file = Path(__file__).parent / "instances.txt"
    instance_file.write_text("\n".join(instance_ids))
    console.print(f"[green]Launched {len(instance_ids)} instances[/green]")
    
    # Wait for completion
    console.print("\n[bold blue]Step 3: Waiting for completion...[/bold blue]")
    console.print("  (Press Ctrl+C to stop waiting and aggregate results)")
    
    try:
        while True:
            time.sleep(30)
            sqs = boto3.client("sqs", region_name=config.region)
            if not config.queue_url:
                resp = sqs.get_queue_url(QueueName=config.queue_name)
                config.queue_url = resp["QueueUrl"]
            
            attrs = sqs.get_queue_attributes(
                QueueUrl=config.queue_url,
                AttributeNames=["ApproximateNumberOfMessages", "ApproximateNumberOfMessagesNotVisible"],
            )
            pending = int(attrs["Attributes"].get("ApproximateNumberOfMessages", 0))
            in_flight = int(attrs["Attributes"].get("ApproximateNumberOfMessagesNotVisible", 0))
            
            if pending == 0 and in_flight == 0:
                console.print("[green]All tasks completed![/green]")
                break
            
            console.print(f"  Pending: {pending}, In-flight: {in_flight}")
    except KeyboardInterrupt:
        console.print("\n[yellow]Interrupted - aggregating results...[/yellow]")
    
    # Aggregate results
    console.print("\n[bold blue]Step 4: Aggregating results...[/bold blue]")
    # Use existing aggregate.py script
    subprocess.run([
        sys.executable,
        str(Path(__file__).parent / "aggregate.py"),
        "--download",
        "--local-dir", "reports/spot/",
        "--output", "reports/eval-results.jsonl",
        "--summary", "reports/eval-summary.json",
        "--markdown", "reports/RESULTS.md",
        "--html", "reports/RESULTS.html",
    ])

def main():
    parser = argparse.ArgumentParser(description="Orchestrate spot evaluation using runctl")
    subparsers = parser.add_subparsers(dest="command", required=True)
    
    # Generate
    gen = subparsers.add_parser("generate", help="Generate and enqueue tasks")
    gen.add_argument("--backends", nargs="+")
    gen.add_argument("--datasets", nargs="+")
    gen.add_argument("--seeds", type=int, nargs="+")
    gen.add_argument("--max-examples", type=int, default=50)
    gen.add_argument("--dry-run", action="store_true")
    
    # Launch
    launch = subparsers.add_parser("launch", help="Launch spot instances via runctl")
    launch.add_argument("--fleet-size", type=int)
    launch.add_argument("--instance-type", type=str)
    
    # Status
    status = subparsers.add_parser("status", help="Check evaluation progress")
    
    # Teardown
    td = subparsers.add_parser("teardown", help="Terminate instances and cleanup")
    td.add_argument("--purge-queue", action="store_true")
    
    # Full pipeline
    full = subparsers.add_parser("full", help="Run complete pipeline")
    full.add_argument("--backends", nargs="+")
    full.add_argument("--datasets", nargs="+")
    full.add_argument("--seeds", type=int, nargs="+")
    full.add_argument("--max-examples", type=int, default=50)
    full.add_argument("--fleet-size", type=int)
    full.add_argument("--instance-type", type=str)
    
    args = parser.parse_args()
    config = Config()
    
    if args.command == "generate":
        cmd_generate(args, config)
    elif args.command == "launch":
        cmd_launch(args, config)
    elif args.command == "status":
        cmd_status(args, config)
    elif args.command == "teardown":
        cmd_teardown(args, config)
    elif args.command == "full":
        cmd_full(args, config)

if __name__ == "__main__":
    main()

