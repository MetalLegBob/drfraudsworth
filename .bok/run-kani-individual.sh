#!/bin/bash
# Run each Kani harness individually with a 5-minute timeout
# Uses background process + kill pattern (macOS compatible)

source "$HOME/.cargo/env"
export PATH="$HOME/.cargo/bin:$HOME/.local/share/solana/install/active_release/bin:/opt/homebrew/bin:/usr/bin:/bin:/usr/sbin:/sbin:$PATH"
cd "$SCRIPT_DIR/worktree"

TIMEOUT_SECS=300  # 5 minutes
RESULTS_DIR="$SCRIPT_DIR/results"

run_harness() {
    local package=$1
    local harness=$2
    local outfile="$RESULTS_DIR/${package}-kani-${harness}.txt"

    echo "[$package] Running $harness ..."

    cargo kani --harness "$harness" -p "$package" --tests > "$outfile" 2>&1 &
    local KANI_PID=$!

    # Timer subprocess
    ( sleep $TIMEOUT_SECS && kill $KANI_PID 2>/dev/null ) &
    local TIMER_PID=$!

    wait $KANI_PID
    local EXIT_CODE=$?

    # Kill timer if harness finished before timeout
    kill $TIMER_PID 2>/dev/null
    wait $TIMER_PID 2>/dev/null

    if [ $EXIT_CODE -eq 137 ] || [ $EXIT_CODE -eq 143 ]; then
        echo "INCONCLUSIVE (timeout ${TIMEOUT_SECS}s)" >> "$outfile"
        echo "[$package] $harness -> TIMEOUT"
    elif [ $EXIT_CODE -eq 0 ]; then
        # Check if actually proven
        if grep -q "SUCCESSFUL" "$outfile"; then
            echo "[$package] $harness -> PROVEN"
        else
            echo "[$package] $harness -> COMPLETED (exit 0, check output)"
        fi
    else
        echo "[$package] $harness -> FAILED (exit $EXIT_CODE)"
    fi
}

echo "=== Kani Individual Harness Runs (${TIMEOUT_SECS}s timeout) ==="
echo ""

# AMM (5 harnesses)
echo "--- AMM Program ---"
for h in inv_amm_002_output_bounded_by_reserve inv_amm_003_fee_never_exceeds_principal inv_amm_006_zero_input_zero_output inv_amm_007_u128_no_overflow_fee_calc inv_amm_007_u128_no_overflow_swap_calc; do
    run_harness amm "$h"
done

echo ""
echo "--- Staking Program ---"
for h in inv_sr_002_cumulative_monotonicity inv_sr_003_no_panic_add_to_cumulative inv_sr_003_no_panic_update_rewards inv_sr_006_zero_reward_epoch inv_sr_009_precision_delta_non_negative inv_sr_012_u128_no_overflow_add_to_cumulative inv_sr_012_u128_no_overflow_update_rewards inv_sr_018_precision_loss_bounded; do
    run_harness staking "$h"
done

echo ""
echo "--- Bonding Curve Program ---"
for h in inv_bc_001_round_trip_value_non_creation inv_bc_002_price_monotonicity inv_bc_003_input_monotonicity inv_bc_005_sell_tax_ceil_gte_floor inv_bc_006_sell_tax_no_overflow inv_bc_012_precision_no_overflow inv_bc_013_wallet_cap_not_bypassed_partial_fill; do
    run_harness bonding_curve "$h"
done

echo ""
echo "=== ALL DONE ==="
