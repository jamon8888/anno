# AWS Spot Instance Evaluation Infrastructure

Fast, parallel NER/Coref evaluation across all backends and datasets using AWS Spot instances.

## Overview

This infrastructure uses:
- **Spot Fleet** with capacity-optimized allocation for cost efficiency (~70-90% savings)
- **Persistent EBS volume** for cargo/sccache caching (never rebuild deps on instance churn)
- **Parallel work distribution** via SQS queue
- **S3** for results aggregation

## Quick Start

### Local (no AWS, free)

```bash
# Run evaluation locally (no AWS credentials needed)
just eval-local-quick

# Custom backends/datasets
just eval-local BACKENDS="heuristic,stacked" DATASETS="WikiGold,Wnut17" MAX=100
```

### AWS Spot (distributed, ~$1-2)

```bash
# 1. Initial setup (one-time)
./scripts/spot/setup.sh

# 2. Launch evaluation fleet
just spot-eval

# 3. Check progress
just spot-status

# 4. Collect results
just spot-results
```

## Architecture

```
┌─────────────────┐     ┌─────────────┐     ┌─────────────────┐
│  Orchestrator   │────▶│  SQS Queue  │◀────│  Spot Workers   │
│  (local/lambda) │     │  (tasks)    │     │  (c7i.xlarge)   │
└─────────────────┘     └─────────────┘     └────────┬────────┘
        │                                            │
        │                                            ▼
        │              ┌─────────────────────────────────────┐
        └─────────────▶│          S3 Bucket                  │
                       │  - datasets/ (cached)               │
                       │  - models/ (cached)                 │
                       │  - results/ (output)                │
                       └─────────────────────────────────────┘
                                     ▲
                                     │
                       ┌─────────────────────────────────────┐
                       │      Persistent EBS Volume          │
                       │  - /mnt/cache/cargo (CARGO_HOME)    │
                       │  - /mnt/cache/sccache               │
                       │  - /mnt/cache/target (build cache)  │
                       └─────────────────────────────────────┘
```

## Work Distribution

Each task in the queue is a (backend, dataset, seed) triple:

```json
{
  "backend": "gliner",
  "dataset": "WikiGold",
  "seed": 42,
  "max_examples": 500,
  "task": "ner"
}
```

Workers pull tasks, run evaluation, write results to S3.

## Cost Estimates

| Scenario | Instances | Time | Cost |
|----------|-----------|------|------|
| Quick (3 datasets, 3 backends) | 1x c7i.xlarge | ~5 min | ~$0.02 |
| Standard (10 datasets, 5 backends) | 4x c7i.xlarge | ~15 min | ~$0.20 |
| Full (20 datasets, 12 backends, 5 seeds) | 8x c7i.2xlarge | ~30 min | ~$1.00 |

*Spot prices vary; estimates assume ~$0.05/hr for c7i.xlarge*

## Files

| File | Purpose |
|------|---------|
| `setup.sh` | One-time infrastructure setup (VPC, roles, bucket) |
| `launch.sh` | Launch spot fleet with specified capacity |
| `worker.sh` | Runs on each spot instance (pulls tasks, runs eval) |
| `orchestrate.py` | Generate tasks, monitor progress, aggregate results |
| `teardown.sh` | Clean up all AWS resources |
| `cloudformation.yaml` | Full infrastructure as code |

## Environment Variables

| Variable | Description | Default |
|----------|-------------|---------|
| `ANNO_SPOT_REGION` | AWS region | us-east-1 |
| `ANNO_SPOT_BUCKET` | S3 bucket for data | arc-anno-data |
| `ANNO_SPOT_QUEUE` | SQS queue name | anno-eval-tasks |
| `ANNO_SPOT_FLEET_SIZE` | Number of spot instances | 4 |
| `ANNO_SPOT_INSTANCE_TYPES` | Comma-separated types | c7i.xlarge,c7a.xlarge,m7i.xlarge |

## Interruption Handling

Spot instances may be reclaimed with 2-minute notice:
1. Worker detects interruption via instance metadata
2. Current task marked as incomplete in SQS (returns to queue)
3. Fleet automatically requests replacement instance
4. New instance picks up the task (idempotent)

## Caching Strategy

### Build Caching (EBS)

1. **EBS volume** mounted at `/mnt/cache`:
   - `CARGO_HOME=/mnt/cache/cargo` - toolchains, registry
   - `SCCACHE_DIR=/mnt/cache/sccache` - compiled artifacts
   - `CARGO_TARGET_DIR=/mnt/cache/target/anno` - build output

2. First instance builds; subsequent instances reuse via shared EBS.

### Data Caching (S3)

On-demand fetch with local caching:
- Datasets: `ANNO_S3_CACHE=1` enables S3 fallback
- Models: `HF_HOME=/mnt/cache/models` for ONNX models

### Prediction Caching (Incremental Eval)

For frequent/cheap re-evaluation, cache predictions:

| Layer | What's Cached | Key | Storage |
|-------|---------------|-----|---------|
| **Dataset** | Parsed gold annotations | DatasetId | S3 + local |
| **Predictions** | NER output per text | (text_hash, backend, version) | JSONL |
| **Metrics** | Final scores | (backend, dataset, seed) | `reports/eval-results.jsonl` |

**Why this matters:**
- Re-scoring against new gold data: No re-inference needed
- Adding new metrics: Just re-compute from cached predictions
- Cross-node sharing: S3 as shared prediction cache

```bash
# Check prediction cache stats
anno cache stats

# Invalidate predictions for updated model
anno cache invalidate --model gliner
```

## Manual Commands

```bash
# Check fleet status
aws ec2 describe-spot-fleet-requests --region us-east-1

# View queue depth
aws sqs get-queue-attributes --queue-url $QUEUE_URL --attribute-names ApproximateNumberOfMessages

# Cancel fleet (graceful)
aws ec2 cancel-spot-fleet-requests --spot-fleet-request-ids $FLEET_ID --terminate-instances

# List results
aws s3 ls s3://arc-anno-data/results/
```

## Relationship to CI Tests

The spot evaluation complements the CI randomized matrix tests:

| Aspect | CI (`randomized_matrix_ci`) | Spot Evaluation |
|--------|----------------------------|-----------------|
| **When** | Every PR, every push | On-demand, nightly, pre-release |
| **Scope** | 3-5 backend×dataset pairs | 780 combinations (12×13×5) |
| **Examples** | 15-50 per dataset | 500 per dataset |
| **Duration** | ~2 min (CI timeout) | ~100 min (4 workers) |
| **Cost** | Free (GitHub Actions) | ~$0.40 per full run |
| **Goal** | Catch regressions fast | Comprehensive coverage |

### Integration Points

1. **Badness Scores Flow to CI**:
   Spot evaluation computes "badness scores" for each backend×dataset pair.
   CI can use `ANNO_HISTORY_FILE` to prioritize worst-performing combinations:
   ```bash
   # After spot run, export badness history
   just spot-results --badness-export reports/badness-history.csv
   
   # CI uses it for MAB-style sampling
   ANNO_HISTORY_FILE=reports/badness-history.csv cargo test --test randomized_matrix_ci
   ```

2. **CI Seeds Trace Back to Spot**:
   When CI finds a regression, reproduce it in spot:
   ```bash
   # CI failed with seed 12345 on gliner×WikiGold
   just spot-eval-custom "gliner" "WikiGold" 1
   ```

3. **Pre-Release Checklist**:
   ```bash
   # Before release, run comprehensive spot evaluation
   just spot-upload-src
   just spot-eval
   
   # Verify no regressions vs previous release
   just spot-results --compare-baseline reports/v0.1.0-baseline.json
   ```

### Shared Components

Both use the same evaluation harness:
- `anno::eval::task_evaluator::TaskEvaluator`
- `anno::eval::backend_factory::BackendFactory`  
- `anno::eval::loader::DatasetLoader`

The spot worker script calls the same `anno benchmark` CLI that the CI tests use internally.

## Just Commands Reference

### Local (no AWS)

| Command | Description | Cost |
|---------|-------------|------|
| `just eval-local-quick` | Quick local eval (zero-dep backends) | Free |
| `just eval-local BACKENDS DATASETS MAX` | Custom local eval | Free |

### AWS Spot

| Command | Description | Cost |
|---------|-------------|------|
| `just spot-setup` | One-time AWS infrastructure setup | Free |
| `just spot-upload-src` | Upload source code to S3 | Free |
| `just spot-eval-quick` | Quick test (1 worker, 3 backends, 2 datasets) | ~$0.02 |
| `just spot-eval` | Full evaluation (4 workers, all combinations) | ~$1-2 |
| `just spot-eval-ml` | ML backends only (ONNX/Candle) | ~$0.50 |
| `just spot-monitor` | Monitor workers via SSM | Free |
| `just spot-status` | Check fleet and queue status | Free |
| `just spot-results` | Download and aggregate results | Free |
| `just spot-teardown` | Cancel fleet, cleanup | Free |
| `just ci-matrix-local` | Run CI test with spot badness history | Free |

## trainctl Integration (Optional)

If [trainctl](../../../trainctl) is available, additional features are unlocked:

```bash
just spot-dash              # Interactive ratatui dashboard
just spot-ps                # Process list on worker
just spot-top               # Interactive top on worker
just spot-cost              # Show fleet costs
just spot-sync-datasets     # Fast parallel dataset download
```

The `trainctl-bridge.sh` script provides the integration. Build trainctl with:
```bash
cd ../trainctl && cargo build --release
```

The self-contained approach (`just spot-eval`) works without trainctl.

## Troubleshooting

**Instances not launching:**
- Check Spot capacity in selected AZs
- Try adding more instance types
- Check IAM role has required permissions

**Builds failing:**
- Verify EBS volume is attached and mounted
- Check `/var/log/cloud-init-output.log`
- Ensure CARGO_HOME points to persistent storage

**Tasks not completing:**
- Check CloudWatch logs: `/aws/anno-eval/workers`
- Verify SQS visibility timeout > max task duration
- Check for OOM (increase instance size)

