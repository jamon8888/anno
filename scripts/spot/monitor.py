#!/usr/bin/env python3
# /// script
# requires-python = ">=3.10"
# dependencies = ["boto3>=1.34", "rich>=13.0"]
# ///
"""
Monitor spot evaluation workers via SSM (no SSH required).

Usage:
    uv run scripts/spot/monitor.py              # Show all workers
    uv run scripts/spot/monitor.py --watch      # Live updates
    uv run scripts/spot/monitor.py --logs i-xxx # Tail worker logs
    uv run scripts/spot/monitor.py --exec i-xxx "cargo --version"
"""

import argparse
import json
import os
import sys
import time
from dataclasses import dataclass
from pathlib import Path

import boto3
from rich.console import Console
from rich.live import Live
from rich.table import Table

console = Console()


@dataclass
class Config:
    region: str = "us-east-1"
    
    def __post_init__(self):
        self.region = os.environ.get("ANNO_SPOT_REGION", self.region)
        config_path = Path(__file__).parent / "config.env"
        if config_path.exists():
            for line in config_path.read_text().splitlines():
                if "=" in line and not line.startswith("#"):
                    key, value = line.split("=", 1)
                    if key == "ANNO_SPOT_REGION":
                        self.region = value


def get_anno_instances(ec2_client) -> list[dict]:
    """Find all running anno-eval-worker instances."""
    response = ec2_client.describe_instances(
        Filters=[
            {"Name": "tag:Name", "Values": ["anno-eval-worker"]},
            {"Name": "instance-state-name", "Values": ["running", "pending"]},
        ]
    )
    
    instances = []
    for reservation in response.get("Reservations", []):
        for instance in reservation.get("Instances", []):
            instances.append({
                "id": instance.get("InstanceId"),
                "type": instance.get("InstanceType"),
                "state": instance.get("State", {}).get("Name"),
                "launch_time": instance.get("LaunchTime"),
                "public_ip": instance.get("PublicIpAddress"),
                "private_ip": instance.get("PrivateIpAddress"),
            })
    
    return instances


def run_ssm_command(ssm_client, instance_id: str, command: str, timeout: int = 30) -> str:
    """Execute command on instance via SSM and return output."""
    response = ssm_client.send_command(
        InstanceIds=[instance_id],
        DocumentName="AWS-RunShellScript",
        Parameters={"commands": [command]},
        TimeoutSeconds=timeout,
    )
    
    command_id = response["Command"]["CommandId"]
    
    # Wait for completion
    for _ in range(timeout):
        time.sleep(1)
        result = ssm_client.get_command_invocation(
            CommandId=command_id,
            InstanceId=instance_id,
        )
        status = result["Status"]
        if status in ("Success", "Failed", "TimedOut", "Cancelled"):
            break
    
    if status == "Success":
        return result.get("StandardOutputContent", "")
    else:
        return f"[{status}] {result.get('StandardErrorContent', '')}"


def get_worker_status(ssm_client, instance_id: str) -> dict:
    """Get worker status from instance."""
    # Check if worker is ready and get basic stats
    cmd = """
    echo "ready:$(test -f /tmp/anno-worker-ready && echo yes || echo no)"
    echo "uptime:$(uptime -p 2>/dev/null || uptime)"
    echo "load:$(cat /proc/loadavg | cut -d' ' -f1-3)"
    echo "memory:$(free -h | awk '/^Mem:/{print $3"/"$2}')"
    if [ -f /var/log/anno-worker.log ]; then
        echo "log_lines:$(wc -l < /var/log/anno-worker.log)"
        echo "last_log:$(tail -1 /var/log/anno-worker.log | cut -c1-80)"
    fi
    """
    
    try:
        output = run_ssm_command(ssm_client, instance_id, cmd, timeout=10)
        status = {}
        for line in output.strip().split("\n"):
            if ":" in line:
                key, value = line.split(":", 1)
                status[key.strip()] = value.strip()
        return status
    except Exception as e:
        return {"error": str(e)}


def build_status_table(instances: list[dict], statuses: dict[str, dict]) -> Table:
    """Build rich table showing worker status."""
    table = Table(title="Anno Evaluation Workers")
    table.add_column("Instance", style="cyan")
    table.add_column("Type", style="white")
    table.add_column("State", style="green")
    table.add_column("Ready", style="yellow")
    table.add_column("Load", style="white")
    table.add_column("Memory", style="white")
    table.add_column("Last Activity", style="dim")
    
    for inst in instances:
        status = statuses.get(inst["id"], {})
        ready = status.get("ready", "?")
        ready_style = "green" if ready == "yes" else "red"
        
        table.add_row(
            inst["id"],
            inst["type"],
            inst["state"],
            f"[{ready_style}]{ready}[/]",
            status.get("load", "-"),
            status.get("memory", "-"),
            status.get("last_log", "-")[:40],
        )
    
    return table


def cmd_status(args, config: Config):
    """Show worker status."""
    ec2 = boto3.client("ec2", region_name=config.region)
    ssm = boto3.client("ssm", region_name=config.region)
    
    instances = get_anno_instances(ec2)
    
    if not instances:
        console.print("[yellow]No anno-eval-worker instances found[/yellow]")
        return
    
    if args.watch:
        with Live(console=console, refresh_per_second=0.2) as live:
            while True:
                instances = get_anno_instances(ec2)
                statuses = {}
                for inst in instances:
                    statuses[inst["id"]] = get_worker_status(ssm, inst["id"])
                
                live.update(build_status_table(instances, statuses))
                time.sleep(5)
    else:
        statuses = {}
        for inst in instances:
            statuses[inst["id"]] = get_worker_status(ssm, inst["id"])
        
        console.print(build_status_table(instances, statuses))


def cmd_logs(args, config: Config):
    """Tail worker logs."""
    ssm = boto3.client("ssm", region_name=config.region)
    
    cmd = f"tail -n {args.lines} /var/log/anno-worker.log"
    if args.follow:
        cmd = f"tail -f /var/log/anno-worker.log"
    
    console.print(f"[bold]Logs from {args.instance}:[/bold]\n")
    
    if args.follow:
        # For follow mode, we need to poll
        last_lines = 0
        while True:
            output = run_ssm_command(ssm, args.instance, 
                f"wc -l < /var/log/anno-worker.log && tail -n 20 /var/log/anno-worker.log",
                timeout=10)
            lines = output.strip().split("\n")
            if lines:
                try:
                    current_lines = int(lines[0])
                    if current_lines > last_lines:
                        for line in lines[1:]:
                            console.print(line)
                        last_lines = current_lines
                except ValueError:
                    pass
            time.sleep(2)
    else:
        output = run_ssm_command(ssm, args.instance, cmd, timeout=30)
        console.print(output)


def cmd_exec(args, config: Config):
    """Execute command on instance."""
    ssm = boto3.client("ssm", region_name=config.region)
    
    console.print(f"[bold]Executing on {args.instance}:[/bold] {args.command}\n")
    output = run_ssm_command(ssm, args.instance, args.command, timeout=60)
    console.print(output)


def main():
    parser = argparse.ArgumentParser(description="Monitor spot evaluation workers")
    parser.add_argument("--watch", "-w", action="store_true", help="Live updates")
    parser.add_argument("--logs", metavar="INSTANCE", help="Tail logs from instance")
    parser.add_argument("--lines", "-n", type=int, default=50, help="Number of log lines")
    parser.add_argument("--follow", "-f", action="store_true", help="Follow logs")
    parser.add_argument("--exec", dest="exec_cmd", nargs=2, metavar=("INSTANCE", "CMD"),
                       help="Execute command on instance")
    
    args = parser.parse_args()
    config = Config()
    
    if args.logs:
        args.instance = args.logs
        cmd_logs(args, config)
    elif args.exec_cmd:
        args.instance = args.exec_cmd[0]
        args.command = args.exec_cmd[1]
        cmd_exec(args, config)
    else:
        cmd_status(args, config)


if __name__ == "__main__":
    main()

