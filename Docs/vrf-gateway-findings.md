# Switchboard VRF Gateway Findings

**Date**: 2026-03-25
**Scope**: Oracle topology, failover options, and error patterns for Switchboard on-demand randomness (commit-reveal VRF)
**Sources**: SDK source code analysis (`@switchboard-xyz/on-demand` v3.7.x), Switchboard documentation, devnet operational experience (~3 months), mainnet crank operation (initial deployment)

## Architecture: Single-Oracle Binding

Each Switchboard randomness account is **bound to one specific oracle** at commit time. This is fundamental to the TEE-based security model and cannot be changed without protocol-level modifications.

### How Oracle Selection Works

1. **Commit phase** (`commitIx()`): The SDK calls `queue.fetchOracleByLatestVersion()`, which randomly selects one oracle from the queue's `oracleKeys` array. That oracle's pubkey is written into the randomness account on-chain.

2. **Reveal phase** (`revealIx()`): The SDK reads `data.oracle` from the on-chain randomness account, loads that oracle's account data, extracts the `gatewayUri` field, and contacts **only that gateway**. The oracle signs the randomness inside its TEE using SECP256k1. The signature is verified on-chain against that specific oracle's key.

3. **No gateway parameter on reveal**: `revealIx()` accepts only an optional `payer` — there is no way to override which gateway is contacted. The gateway URL comes from the oracle account data, not from the caller.

### Why Alternative Gateways Fail (Error 0x1780)

Each oracle has a unique SECP256k1 keypair inside its TEE enclave. When you contact a different oracle's gateway, that oracle signs with **its own** key. The on-chain `randomnessReveal` instruction verifies the signature against the oracle pubkey stored in the randomness account — which is the **original** oracle. Signature mismatch → error `0x1780`.

This is by design: if you could use any oracle to reveal, an attacker controlling one oracle could selectively reveal or withhold randomness outcomes.

## Mainnet Queue Topology

- **Queue**: `A43DyUGA7s8eXPxqEjJY6EBu1KKbNgfxF8h17VAHn13w`
- **Multiple oracles**: The mainnet queue has multiple registered oracle operators (the `oracleKeys` array on the queue account)
- **Each oracle runs its own gateway**: Stored in the `gatewayUri` field of each oracle's account data
- **Oracle selection is random**: `fetchOracleByLatestVersion()` picks randomly from the queue, so over many epochs, different oracles will be used

### Devnet vs Mainnet Differences

| Factor | Devnet | Mainnet |
|--------|--------|---------|
| Oracle count | Fewer operators | More operators (higher redundancy) |
| Uptime SLAs | Best-effort, occasional downtime | Production-grade, financially incentivized |
| TEE key rotation | Same schedule | Same schedule (hourly rotation windows) |
| Single-oracle binding | Yes | Yes (same protocol) |

## Failover Strategy

Given the single-oracle binding constraint, the only failover options are:

### 1. Retry the Same Gateway (Primary)
If the assigned gateway returns 503 or times out, retry with exponential backoff. The oracle may be temporarily overloaded or restarting. **This is the cheapest and most common recovery path.**

- Implementation: `tryReveal()` with exponential backoff 1s → 16s, 5 attempts (happy path) or 10 attempts (recovery path)
- Total wait: ~31s (happy) or ~93s (recovery)

### 2. VRF Timeout Recovery (Fallback)
If the assigned oracle is genuinely down (not just slow), wait for the VRF timeout window to expire, then create a **fresh randomness account**. The fresh account gets a newly-selected oracle (potentially a different, healthy one).

- Wait: `VRF_TIMEOUT_SLOTS` (300 slots ≈ 2 minutes)
- Create fresh randomness keypair
- Call `retry_epoch_vrf` on-chain
- Commit + reveal + consume with the new randomness
- The stale randomness account from the failed attempt is closed inline

### 3. Self-Hosted Crossbar (Not Applicable for Randomness)
Switchboard allows running your own Crossbar server for **data feeds** (price simulations, encoded update instructions). However, Crossbar is a relay/cache layer — it does not hold TEE signing keys. **Self-hosted Crossbar cannot produce randomness reveal signatures.** Only the assigned oracle's TEE enclave can do that.

## Error Patterns Observed

| Error | Cause | Frequency | Recovery |
|-------|-------|-----------|----------|
| Gateway 503 | Oracle overloaded or restarting | Occasional | Retry with backoff |
| Gateway timeout | Network issue or oracle down | Rare | Retry, then timeout recovery |
| `0x1780` (signature mismatch) | Contacted wrong oracle's gateway | Only during gateway rotation attempts | Don't rotate — use timeout recovery instead |
| `fetchRandomnessReveal` network error | DNS/connectivity to gateway URL | Rare | Retry with backoff |

## TEE Key Rotation Windows

Switchboard oracles periodically rotate their TEE enclave code. During the hour before rotation, randomness requests may be restricted. This is documented in Switchboard's approach docs: "we restrict request generation in the hour before rotation."

**Implication for crank**: If commit succeeds but reveal consistently fails with non-503 errors near rotation windows, the timeout recovery path handles this naturally.

## Instrumentation for Ongoing Monitoring

Phase 105 added 4 instrumentation fields to every epoch transition JSON log:

- `gateway_ms`: Wall-clock time for the successful `revealIx()` call
- `reveal_attempts`: Number of reveal retries before success
- `recovery_time_ms`: Total recovery path duration (0 = happy path)
- `commit_to_reveal_slots`: Slot delta between commit and successful reveal

These metrics will reveal:
- Which oracles are slow (high `gateway_ms`)
- Whether retries are trending up (early warning of oracle health issues)
- Recovery path frequency (how often timeout recovery fires)

## Conclusions

1. **Gateway rotation is not a viable failover strategy** — confirmed for both devnet and mainnet. This is an architectural constraint of TEE-signed randomness.
2. **Mainnet benefits from more oracle operators** — each fresh randomness account has a higher chance of landing on a healthy oracle.
3. **Our timeout recovery path is the correct fallback** — matches how Switchboard's own `commitAndReveal()` SDK method handles failures (infinite retry loop on the same gateway).
4. **No action needed beyond current implementation** — exponential backoff (105-02) + timeout recovery (existing) + instrumentation (105-02) + alerting (105-03) cover the operational requirements.
