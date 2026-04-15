#!/usr/bin/env bash
#
# verify-simd-0268.sh
# Checks SIMD-0268 (Raise CPI Nesting Limit) feature gate status on devnet and mainnet.
# Informational only -- exits 0 regardless of active/inactive.
#
# Usage: bash scripts/verify/verify-simd-0268.sh

set -euo pipefail

# Source Solana environment
source "$HOME/.cargo/env"
export PATH="$HOME/.local/share/solana/install/active_release/bin:$PATH"

FEATURE_GATE="6TkHkRmP7JZy1fdM6fg5uXn76wChQBWGokHBJzrLB3mj"

echo "=== SIMD-0268: Raise CPI Nesting Limit ==="
echo "Feature gate: $FEATURE_GATE"
echo ""

# --- Devnet ---
echo "--- Devnet ---"
DEVNET_OUTPUT=$(solana feature status "$FEATURE_GATE" -ud 2>&1 || true)
echo "$DEVNET_OUTPUT"

if echo "$DEVNET_OUTPUT" | grep -qi "active"; then
  if echo "$DEVNET_OUTPUT" | grep -qi "inactive"; then
    DEVNET_STATUS="INACTIVE"
  else
    DEVNET_STATUS="ACTIVE"
    DEVNET_SLOT=$(echo "$DEVNET_OUTPUT" | grep -oP '\d{5,}' | head -1 || echo "NA")
  fi
else
  DEVNET_STATUS="UNKNOWN"
fi
echo ""

# --- Mainnet ---
echo "--- Mainnet ---"
MAINNET_OUTPUT=$(solana feature status "$FEATURE_GATE" -um 2>&1 || true)
echo "$MAINNET_OUTPUT"

if echo "$MAINNET_OUTPUT" | grep -qi "active"; then
  if echo "$MAINNET_OUTPUT" | grep -qi "inactive"; then
    MAINNET_STATUS="INACTIVE"
  else
    MAINNET_STATUS="ACTIVE"
    MAINNET_SLOT=$(echo "$MAINNET_OUTPUT" | grep -oP '\d{5,}' | head -1 || echo "NA")
  fi
else
  MAINNET_STATUS="UNKNOWN"
fi
echo ""

# --- Summary ---
echo "=== Summary ==="
echo "  Devnet:  $DEVNET_STATUS (activation slot: ${DEVNET_SLOT:-NA})"
echo "  Mainnet: $MAINNET_STATUS (activation slot: ${MAINNET_SLOT:-NA})"
echo ""
echo "Impact on v1.7: All new CPI paths are depth 3 max. CPI depth limit (4) is not a blocker."

exit 0
