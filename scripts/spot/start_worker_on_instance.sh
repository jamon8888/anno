#!/bin/bash
# Helper script to start worker on an existing instance
# Usage: ./start_worker_on_instance.sh <instance-id>

set -e

INSTANCE_ID="${1:-}"
if [ -z "$INSTANCE_ID" ]; then
    echo "Usage: $0 <instance-id>"
    echo "Example: $0 i-018b536475cbc9327"
    exit 1
fi

echo "Starting worker on instance: $INSTANCE_ID"
echo ""

# Use the orchestration script's function via uv run
uv run scripts/spot/orchestrate_runctl.py launch --fleet-size 0 2>/dev/null || true

# Actually, we need to call the function directly - use a Python wrapper
uv run -- python3 -c "
import sys
sys.path.insert(0, 'scripts/spot')
from orchestrate_runctl import Config, start_worker_via_ssm

config = Config()
result = start_worker_via_ssm(config, '$INSTANCE_ID')
if result:
    print('✓ Worker startup initiated')
    sys.exit(0)
else:
    print('✗ Worker startup failed')
    sys.exit(1)
"

echo ""
echo "Monitor with:"
echo "  uv run scripts/spot/orchestrate_runctl.py status"
echo "  aws ssm send-command --instance-ids $INSTANCE_ID --document-name AWS-RunShellScript --parameters 'commands=[\"tail -50 /tmp/worker.log\"]'"

