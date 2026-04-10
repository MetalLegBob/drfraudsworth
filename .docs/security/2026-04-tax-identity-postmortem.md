# Post-Mortem: Tax Program Pool/Mint Identity Mismatch

## Summary

A caller-controlled boolean argument (`is_crime`) in the Tax Program's swap handlers was used to select between CRIME and FRAUD tax schedules without being bound to the actual mint of the pool being swapped. An attacker could invoke a CRIME pool swap while passing `is_crime = false` (or vice versa) to pay the cheaper side's tax rate, gaining approximately 12-14% better execution in the actively-discounted direction. The vulnerability affected `swap_sol_buy` and `swap_sol_sell` on mainnet since launch. It was reported by a whitehat researcher, confirmed within 30 minutes, patched via a Squads-governed timelocked upgrade, and verified post-deploy on the same day.

## Timeline

| Date | Event |
|------|-------|
| 2026-03-14 | Mainnet Tax Program deployed (vulnerability present from day one) |
| 2026-04-07 07:00 UTC | Whitehat report received via X DM describing the `is_crime` trust boundary gap |
| 2026-04-07 07:30 UTC | Independent investigation confirms the bug -- code audit of `swap_sol_buy.rs:78` and `swap_sol_sell.rs:93` |
| 2026-04-07 08:00 UTC | Fix design agreed (Option B + C defense-in-depth) |
| 2026-04-07 | Plan 01 complete: code fix + 12-test LiteSVM regression suite, pre-patch replay confirms exploitability |
| 2026-04-08 | Plan 02 complete: devnet deploy, 4/4 scripted exploit rejections, 4/4 honest frontend swaps, SOS/BOK/sharp-edges audits clean |
| 2026-04-09 | Plan 03 complete: mainnet binary built, buffer staged, Squads 2-of-3 proposal created, 1-hour timelock observed, upgrade executed at slot 412060360 |
| 2026-04-09 | Post-deploy verification: multiple honest swaps verified by project owner on mainnet, 30-minute monitoring window clean |
| 2026-04-09 | Disclosure to reporter: fix confirmed live, bounty discussion initiated |

## Root Cause

The Tax Program's `swap_sol_buy` and `swap_sol_sell` handlers accepted a caller-supplied `is_crime: bool` argument and passed it directly to `EpochState::get_tax_bps(is_crime, is_buy)` to select the tax schedule. Neither the handler logic nor the Anchor account constraints verified that this flag matched the actual mint of the pool being swapped.

Specifically:
- `swap_sol_buy.rs` line 78: `let tax_bps = epoch_state.get_tax_bps(is_crime, true);`
- `swap_sol_sell.rs` line 93: `let tax_bps = epoch_state.get_tax_bps(is_crime, false);`

The Tax Program had **zero knowledge** of the CRIME or FRAUD mint addresses -- no constants, no constraints, no runtime checks. The `mint_b` account in the instruction struct was typed but had no `address` constraint, and the `pool` account was a raw `AccountInfo` with only a mutability flag.

The AMM (called via CPI) validated its own internal pool consistency (vault-to-mint bindings, seed derivation) but had no opinion on which "logical" token identity the Tax Program intended. The trust boundary between the caller's declared intent and the on-chain state was entirely missing.

## Scope

**Affected entry points:**
- `swap_sol_buy` -- both CRIME/SOL and FRAUD/SOL pools
- `swap_sol_sell` -- both CRIME/SOL and FRAUD/SOL pools

**Not affected:**
- `swap_exempt` -- `_is_crime` argument is unused (leading underscore), and the instruction is gated behind a `carnage_authority` signer
- All Carnage paths (`execute_carnage_atomic`, etc.) -- they route through `swap_exempt`
- PROFIT pool swaps -- handled by the AMM directly, no Tax Program involvement
- AMM internal accounting -- pool reserves and PDA derivation are sound

## Impact

**Severity: High** -- tax evasion / revenue mispricing.

**What could happen:** An attacker could pay the cheaper epoch tax rate on any swap by lying about the token identity, gaining approximately 12-14% better execution when the tax differential between CRIME and FRAUD was at its widest.

**What could NOT happen:**
- No pool drain (AMM reserves are not affected by which tax schedule is applied)
- No token theft (the swap itself executes correctly through the AMM)
- No authority compromise
- No state corruption

**Estimated exploitation:** None detected. The bug, while real, required understanding the internal tax schedule rotation mechanism and constructing a custom transaction with a mismatched `is_crime` flag -- not something that would happen through normal frontend usage. No anomalous tax patterns were observed in mainnet swap history during the investigation.

## Fix

Two independent, complementary checks were applied (defense-in-depth):

**Option B -- Validate `is_crime` against `mint_b.key()`:**
- Added `crime_mint()` and `fraud_mint()` constants to the Tax Program, feature-gated for devnet/mainnet
- In both swap handlers, the `is_crime` flag is now derived from the validated `mint_b` account key and cross-checked against the caller-supplied argument
- The caller-supplied argument becomes a **witness** (assertion of intent), not an input -- if it disagrees with the derived value, the transaction reverts with `TaxIdentityMismatch`
- If `mint_b` is neither CRIME nor FRAUD, the transaction reverts with `UnknownTaxedMint`

**Option C -- Bind `mint_b` to the pool's stored mint:**
- Extended `pool_reader.rs` with `read_pool_reserves_with_token_mint()` that extracts the non-SOL mint from the pool's raw byte layout
- Both swap handlers now verify `require_keys_eq!(pool_token_mint, ctx.accounts.mint_b.key())`, reverting with `PoolMintMismatch` on disagreement
- This closes the residual gap where a caller could pair a CRIME pool with a FRAUD mint_b account

**New error codes:**
- `TaxIdentityMismatch` (6019) -- `is_crime` flag does not match derived identity
- `PoolMintMismatch` (6020) -- pool's stored mint does not match passed mint_b
- `UnknownTaxedMint` (6021) -- mint_b is not a recognized taxed token

**What was intentionally NOT changed:**
- The `is_crime` argument was kept for IDL stability (non-breaking change)
- No changes to `EpochState::get_tax_bps()` (the bug was upstream of it)
- No changes to any other program (AMM, Epoch, Staking, Vault, Bonding Curve)

## What Went Right

1. **Whitehat acted in good faith.** The reporter disclosed privately, waited for the fix, and coordinated timing. This is exactly how responsible disclosure should work.

2. **Investigation was fast.** The bug was confirmed within 30 minutes of the initial report through direct code inspection.

3. **LiteSVM regression suite caught the bug definitively.** A 12-test suite was written that:
   - Proved all 4 exploit variants (CRIME-lie-as-FRAUD buy/sell, FRAUD-lie-as-CRIME buy/sell) are rejected on the patched code
   - Proved 2 core exploit variants **succeed** on pre-patch code via `git stash` replay -- direct regression proof

4. **Squads timelocked upgrade worked as designed.** The 2-of-3 multisig with 1-hour timelock executed correctly, preserving the upgrade authority throughout.

5. **Defense-in-depth.** Two independent checks (Option B + C) mean a single future refactoring mistake cannot silently re-open the vulnerability.

6. **Build pipeline hardening.** During the fix, a separate treasury constant drift was discovered and reverted, and the build pipeline was hardened with cfg-aware patching and a mandatory hardcoded-address sweep.

## What Went Wrong

1. **Bug existed since mainnet launch (~24 days).** The vulnerability was present in every Tax Program build from the initial deployment.

2. **Original audit scope did not catch it.** The SOS audit of the Tax Program was scoped to focus areas that did not include trust-boundary analysis between caller-supplied arguments and on-chain state. This is a lesson for future audit scoping.

3. **No mint identity constants in the Tax Program.** The Tax Program was built without any knowledge of the CRIME/FRAUD mint addresses, making it structurally impossible for any constraint to enforce the correct binding until constants were added.

## Action Items

1. **Future audits must explicitly check trust boundaries.** Any caller-supplied tag, flag, or enum that selects program behavior must be verified as bound to validated on-chain state. This should be a standard audit checklist item.

2. **Hardcoded address sweep is now mandatory.** The `.docs/standalone/hardcoded-address-sweep.md` checklist was created during this hotfix and is now linked from the mainnet deploy and vault upgrade checklists as a pre/post-build gate.

3. **Pre-existing test harnesses modernized.** The original `test_swap_sol_buy.rs` / `test_swap_sol_sell.rs` test files used ephemeral keypair mints incompatible with pinned mint constants. The `identity_mismatch.rs` regression suite using `svm.set_account` at production pubkeys is the canonical test pattern going forward.

## Credit

Reported by a whitehat security researcher via private X DM on 2026-04-07. Bounty paid. Reporter handle withheld pending their permission for public attribution.

---

*Hotfix phase: 122.1-tax-pool-identity-fix*
*Mainnet deploy slot: 412060360*
*Binary sha256: 0dd98c85aff9dd88856dc15a8d3a8ee8f712df05dca62bd3388db10c68f46ad0*
