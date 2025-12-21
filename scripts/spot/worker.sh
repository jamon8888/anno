#!/bin/bash
# Spot instance worker script
#
# Runs on each spot instance:
# 1. Mounts EBS cache volume
# 2. Syncs datasets/models from S3
# 3. Pulls tasks from SQS, runs eval, pushes results
# 4. Monitors for spot interruption
#
# Usually launched via SSM RunCommand or in user-data.

set -euo pipefail

# Load config
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
if [[ -f "$SCRIPT_DIR/config.env" ]]; then
    source "$SCRIPT_DIR/config.env"
fi

REGION="${ANNO_SPOT_REGION:-us-east-1}"
BUCKET="${ANNO_SPOT_BUCKET:-arc-anno-data}"
QUEUE_URL="${ANNO_SPOT_QUEUE_URL:-}"
CACHE_MOUNT="/mnt/cache"
ANNO_DIR="$CACHE_MOUNT/anno"

# Logging
LOG_GROUP="/aws/anno-eval/workers"
INSTANCE_ID=$(curl -s http://169.254.169.254/latest/meta-data/instance-id || echo "local")

log() {
    local level="$1"
    shift
    echo "[$(date -uIs)] [$level] [$INSTANCE_ID] $*"
}

log_info() { log "INFO" "$@"; }
log_warn() { log "WARN" "$@"; }
log_error() { log "ERROR" "$@"; }

# ============================================================================
# Cache/Volume Setup
# ============================================================================

setup_cache_volume() {
    log_info "Setting up cache volume..."
    
    # Check if already mounted
    if mountpoint -q "$CACHE_MOUNT"; then
        log_info "Cache already mounted at $CACHE_MOUNT"
        return 0
    fi
    
    mkdir -p "$CACHE_MOUNT"
    
    # Find the attached EBS volume (expects /dev/xvdf or /dev/nvme1n1)
    local device=""
    for dev in /dev/xvdf /dev/nvme1n1 /dev/sdf; do
        if [[ -b "$dev" ]]; then
            device="$dev"
            break
        fi
    done
    
    if [[ -z "$device" ]]; then
        log_warn "No cache volume attached, using local storage"
        return 0
    fi
    
    # Check if formatted
    if ! blkid "$device" &>/dev/null; then
        log_info "Formatting $device with xfs..."
        mkfs.xfs "$device"
    fi
    
    mount "$device" "$CACHE_MOUNT"
    log_info "Mounted $device at $CACHE_MOUNT"
    
    # Create directory structure
    mkdir -p "$CACHE_MOUNT"/{cargo,rustup,sccache,target,datasets,models}
    chown -R "$(whoami):$(whoami)" "$CACHE_MOUNT"
}

setup_rust_env() {
    log_info "Setting up Rust environment..."
    
    # Point to persistent cache
    export CARGO_HOME="$CACHE_MOUNT/cargo"
    export RUSTUP_HOME="$CACHE_MOUNT/rustup"
    export SCCACHE_DIR="$CACHE_MOUNT/sccache"
    export CARGO_TARGET_DIR="$CACHE_MOUNT/target"
    export ANNO_CACHE_DIR="$CACHE_MOUNT"
    
    # Install Rust if not present
    if [[ ! -f "$CARGO_HOME/bin/cargo" ]]; then
        log_info "Installing Rust toolchain..."
        curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | \
            sh -s -- -y --default-toolchain stable --no-modify-path
    fi
    
    export PATH="$CARGO_HOME/bin:$PATH"
    
    # Install sccache if not present
    if [[ ! -f "$CARGO_HOME/bin/sccache" ]]; then
        log_info "Installing sccache..."
        cargo install sccache
    fi
    
    export RUSTC_WRAPPER="$CARGO_HOME/bin/sccache"
    
    log_info "Rust $(rustc --version), sccache available"
}

sync_from_s3() {
    log_info "Syncing datasets and models from S3..."
    
    # Prefer s5cmd for speed
    if command -v s5cmd &>/dev/null; then
        s5cmd sync "s3://$BUCKET/datasets/*" "$CACHE_MOUNT/datasets/" 2>/dev/null || true
        s5cmd sync "s3://$BUCKET/models/*" "$CACHE_MOUNT/models/" 2>/dev/null || true
    else
        aws s3 sync "s3://$BUCKET/datasets/" "$CACHE_MOUNT/datasets/" --region "$REGION" || true
        aws s3 sync "s3://$BUCKET/models/" "$CACHE_MOUNT/models/" --region "$REGION" || true
    fi
    
    log_info "S3 sync complete"
}

clone_and_build() {
    log_info "Setting up anno repository..."
    
    if [[ ! -d "$ANNO_DIR/.git" ]]; then
        git clone --depth 1 https://github.com/your-org/anno.git "$ANNO_DIR"
    else
        cd "$ANNO_DIR"
        git fetch origin main --depth 1
        git reset --hard origin/main
    fi
    
    cd "$ANNO_DIR"
    
    # Build release with all eval features
    log_info "Building anno (this may take a few minutes on first run)..."
    cargo build --release --bin anno --features "cli,eval-advanced,onnx,candle" 2>&1 | tail -5
    
    log_info "Build complete: $(./target/release/anno --version 2>/dev/null || echo 'built')"
}

# ============================================================================
# Task Processing
# ============================================================================

receive_task() {
    # Poll SQS for next task
    local response
    response=$(aws sqs receive-message \
        --queue-url "$QUEUE_URL" \
        --region "$REGION" \
        --max-number-of-messages 1 \
        --wait-time-seconds 20 \
        --visibility-timeout 600 \
        --output json 2>/dev/null)
    
    if [[ -z "$response" ]] || [[ "$(echo "$response" | jq -r '.Messages')" == "null" ]]; then
        return 1
    fi
    
    echo "$response"
}

process_task() {
    local task_json="$1"
    
    local body
    body=$(echo "$task_json" | jq -r '.Messages[0].Body')
    local receipt
    receipt=$(echo "$task_json" | jq -r '.Messages[0].ReceiptHandle')
    
    local backend dataset seed max_examples task_type
    backend=$(echo "$body" | jq -r '.backend')
    dataset=$(echo "$body" | jq -r '.dataset')
    seed=$(echo "$body" | jq -r '.seed // 42')
    max_examples=$(echo "$body" | jq -r '.max_examples // 500')
    task_type=$(echo "$body" | jq -r '.task // "ner"')
    
    log_info "Processing: backend=$backend dataset=$dataset seed=$seed"
    
    local output_file="/tmp/result_${backend}_${dataset}_${seed}.json"
    local start_time
    start_time=$(date +%s)
    
    cd "$ANNO_DIR"
    
    # Enable S3 fallback for dataset loading
    export ANNO_S3_CACHE=1
    export ANNO_S3_BUCKET="$BUCKET"
    
    # Run evaluation (note: plural flags --backends/--datasets)
    local exit_code=0
    ./target/release/anno benchmark \
        --backends "$backend" \
        --datasets "$dataset" \
        --seed "$seed" \
        --max-examples "$max_examples" \
        --output "$output_file" 2>&1 || exit_code=$?
    
    local end_time
    end_time=$(date +%s)
    local duration=$((end_time - start_time))
    
    # Wrap result with metadata
    local result_key="results/${backend}/${dataset}/seed_${seed}_$(date +%Y%m%d_%H%M%S).json"
    
    if [[ -f "$output_file" ]]; then
        # Add metadata wrapper
        jq --arg backend "$backend" \
           --arg dataset "$dataset" \
           --arg seed "$seed" \
           --arg instance "$INSTANCE_ID" \
           --arg duration "$duration" \
           --arg exit_code "$exit_code" \
           '. + {_meta: {backend: $backend, dataset: $dataset, seed: ($seed|tonumber), instance: $instance, duration_secs: ($duration|tonumber), exit_code: ($exit_code|tonumber), timestamp: now | todate}}' \
           "$output_file" > "${output_file}.wrapped"
        
        # Upload to S3
        aws s3 cp "${output_file}.wrapped" "s3://$BUCKET/$result_key" --region "$REGION"
        log_info "Result uploaded: s3://$BUCKET/$result_key"
    else
        # Upload error marker
        echo "{\"error\": \"evaluation failed\", \"exit_code\": $exit_code, \"backend\": \"$backend\", \"dataset\": \"$dataset\"}" | \
            aws s3 cp - "s3://$BUCKET/${result_key}.error" --region "$REGION"
        log_error "Evaluation failed for $backend/$dataset (exit $exit_code)"
    fi
    
    # Delete message from queue (task complete)
    aws sqs delete-message \
        --queue-url "$QUEUE_URL" \
        --receipt-handle "$receipt" \
        --region "$REGION"
    
    log_info "Task complete: $backend/$dataset in ${duration}s"
}

# ============================================================================
# Interruption Handling
# ============================================================================

check_spot_interruption() {
    # Check for spot termination notice
    local action
    action=$(curl -s -f http://169.254.169.254/latest/meta-data/spot/instance-action 2>/dev/null || true)
    
    if [[ -n "$action" ]]; then
        log_warn "SPOT INTERRUPTION: $action"
        return 0
    fi
    return 1
}

cleanup() {
    log_info "Cleaning up..."
    
    # Stop sccache to flush
    sccache --stop-server 2>/dev/null || true
    
    # Sync any remaining data
    sync
    
    log_info "Cleanup complete"
}

trap cleanup EXIT

# ============================================================================
# Main Loop
# ============================================================================

main() {
    log_info "Starting anno evaluation worker..."
    
    # Setup
    setup_cache_volume
    setup_rust_env
    sync_from_s3
    clone_and_build
    
    log_info "Worker ready, polling for tasks..."
    
    local idle_count=0
    local max_idle=30  # Exit after ~10 minutes of no tasks (30 * 20s polls)
    
    while true; do
        # Check for spot interruption
        if check_spot_interruption; then
            log_warn "Exiting due to spot interruption notice"
            break
        fi
        
        # Try to get a task
        local task
        if task=$(receive_task); then
            idle_count=0
            process_task "$task"
        else
            ((idle_count++))
            log_info "No tasks available (idle $idle_count/$max_idle)"
            
            if [[ $idle_count -ge $max_idle ]]; then
                log_info "Max idle reached, shutting down"
                break
            fi
        fi
    done
    
    log_info "Worker shutting down"
}

# Run if executed directly
if [[ "${BASH_SOURCE[0]}" == "${0}" ]]; then
    main "$@"
fi

