#!/bin/bash
# Run bonding curve Kani harnesses individually with 5-minute timeout
source $HOME/.cargo/env
export PATH="$HOME/.cargo/bin:$HOME/.local/share/solana/install/active_release/bin:/opt/homebrew/bin:/usr/bin:/bin:/usr/sbin:/sbin:$PATH"
cd "/path/to/project/.bok/worktree"

TIMEOUT_SECS=300
RESULTS_DIR="/path/to/project/.bok/results"
SUMMARY="$RESULTS_DIR/bc-kani-summary.log"
echo "=== Bonding Curve Kani Results ===" > "$SUMMARY"

for harness in inv_bc_001_round_trip_value_non_creation inv_bc_002_price_monotonicity inv_bc_003_input_monotonicity inv_bc_005_sell_tax_ceil_gte_floor inv_bc_006_sell_tax_no_overflow inv_bc_012_precision_no_overflow inv_bc_013_wallet_cap_not_bypassed_partial_fill; do
    outfile="$RESULTS_DIR/bonding_curve-kani-${harness}.txt"
    echo "Running $harness ..."

    # Run kani in background
    cargo kani --harness "$harness" -p bonding-curve --tests > "$outfile" 2>&1 &
    KANI_PID=$!

    # Timer to kill after timeout
    ( sleep $TIMEOUT_SECS && kill -TERM $KANI_PID 2>/dev/null && sleep 5 && kill -9 $KANI_PID 2>/dev/null ) &
    TIMER_PID=$!

    wait $KANI_PID
    EXIT_CODE=$?

    # Clean up timer
    kill $TIMER_PID 2>/dev/null
    wait $TIMER_PID 2>/dev/null

    # Also kill any orphaned cbmc processes for this harness
    pkill -f "cbmc.*${harness}" 2>/dev/null

    if [ $EXIT_CODE -eq 137 ] || [ $EXIT_CODE -eq 143 ] || [ $EXIT_CODE -eq 15 ]; then
        echo "$harness: INCONCLUSIVE (timeout ${TIMEOUT_SECS}s)" | tee -a "$SUMMARY"
    elif [ $EXIT_CODE -eq 0 ]; then
        if grep -q "SUCCESSFUL" "$outfile"; then
            echo "$harness: PROVEN" | tee -a "$SUMMARY"
        else
            echo "$harness: COMPLETED (exit 0)" | tee -a "$SUMMARY"
        fi
    else
        if grep -q "VERIFICATION:- FAILED" "$outfile"; then
            echo "$harness: FAILED (exit $EXIT_CODE)" | tee -a "$SUMMARY"
        else
            echo "$harness: ERROR (exit $EXIT_CODE)" | tee -a "$SUMMARY"
        fi
    fi
done

echo "" >> "$SUMMARY"
echo "=== DONE ===" | tee -a "$SUMMARY"
