#!/usr/bin/env bash
# Ralph Wiggum Loop - Autonomous Claude Code Development
# Runs Claude in a loop, each session picks up the next phase from SPEC.md

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
LOG_DIR="$SCRIPT_DIR/.ralph-logs"
PROMPT_FILE="$SCRIPT_DIR/PROMPT.md"
MAX_ITERATIONS=20  # Safety limit
ITERATION=0

mkdir -p "$LOG_DIR"

unset ANTHROPIC_API_KEY

log() {
    echo "[$(date '+%Y-%m-%d %H:%M:%S')] $*" | tee -a "$LOG_DIR/ralph.log"
}

check_all_phases_done() {
    # Check if all phase checkboxes are complete in SPEC.md
    # Returns 0 if all done, 1 if work remains
    local unchecked
    unchecked=$(grep -c '^\- \[ \]' "$SCRIPT_DIR/SPEC.md" 2>/dev/null || echo "0")
    if [[ "$unchecked" == "0" ]]; then
        return 0
    else
        return 1
    fi
}

run_iteration() {
    local iter=$1
    local log_file="$LOG_DIR/iteration-$(printf '%03d' "$iter")-$(date '+%Y%m%d-%H%M%S').log"

    log "=== Starting iteration $iter ==="
    log "Log file: $log_file"

    # Run Claude with the prompt, capturing output
    # Use bunx to ensure claude is available regardless of PATH
    if bunx @anthropic-ai/claude-code -p "$(cat "$PROMPT_FILE")" --dangerously-skip-permissions 2>&1 | tee "$log_file"; then
        log "Iteration $iter completed successfully"
        return 0
    else
        log "Iteration $iter failed with exit code $?"
        return 1
    fi
}

main() {
    log "Ralph Wiggum Loop starting in: $SCRIPT_DIR"
    log "Max iterations: $MAX_ITERATIONS"

    # Check we're in the right directory
    if [[ ! -f "$SCRIPT_DIR/SPEC.md" ]]; then
        log "ERROR: SPEC.md not found in $SCRIPT_DIR"
        exit 1
    fi

    if [[ ! -f "$PROMPT_FILE" ]]; then
        log "ERROR: PROMPT.md not found"
        exit 1
    fi

    while [[ $ITERATION -lt $MAX_ITERATIONS ]]; do
        ITERATION=$((ITERATION + 1))

        # Check if all work is done
        if check_all_phases_done; then
            log "All phases appear complete! Stopping loop."
            break
        fi

        # Run an iteration
        if ! run_iteration "$ITERATION"; then
            log "Iteration failed. Waiting 30s before retry..."
            sleep 30
        fi

        # Brief pause between iterations
        log "Pausing 5s before next iteration..."
        sleep 5
    done

    if [[ $ITERATION -ge $MAX_ITERATIONS ]]; then
        log "Reached maximum iterations ($MAX_ITERATIONS). Stopping."
    fi

    log "Ralph Wiggum Loop finished after $ITERATION iterations"
}

# Handle Ctrl+C gracefully
trap 'log "Interrupted by user. Exiting..."; exit 130' INT

main "$@"
