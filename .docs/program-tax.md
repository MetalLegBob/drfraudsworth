---
doc_id: program-tax
title: "Tax Program -- Program Specification"
wave: 2
requires: [architecture, data-model]
provides: [program-tax]
status: draft
decisions_referenced: [architecture-truth, on-chain-programs, doc-reconciliation]
needs_verification: []
---

# Tax Program -- Program Specification

## Overview

The Tax Program is the swap orchestration hub for Dr Fraudsworth. Every SOL pool swap (buy or sell of CRIME/FRAUD tokens) flows through this program. It sits between the user and the AMM, performing three critical functions in a single atomic transaction:

1. **Tax calculation** -- reads the current epoch's VRF-derived tax rates from EpochState and computes the tax amount in basis points.
2. **Tax distribution** -- splits collected tax into three destinations: 71% to staking escrow, 24% to Carnage Fund, 5% to treasury.
3. **Swap execution** -- forwards the post-tax amount to the AMM via CPI for constant-product swap execution.

The program exposes two lanes:

- **`swap` lane (public, taxed)** -- `swap_sol_buy` and `swap_sol_sell`, callable by any user. Tax is always applied.
- **`swap_exempt` lane (protocol-only, untaxed)** -- callable only by the Epoch Program's Carnage authority PDA. No tax applied; only the AMM's 1% LP fee applies.

**Program ID:** `43fZGRtmEsP7ExnJE1dbTbNjaP1ncvVmMPusSeksWGEj`

**Key invariants:**

- Tax distribution always sums to the total tax amount: `staking + carnage + treasury == total_tax`.
- The protocol-enforced minimum output floor (50%) prevents zero-slippage sandwich attacks.
- Only the Epoch Program can invoke `swap_exempt` -- enforced by `seeds::program` constraint on the Carnage authority PDA.
- The CPI chain depth for Carnage swaps is exactly at Solana's 4-level limit: Epoch -> Tax -> AMM -> Token-2022 -> Transfer Hook.

---

## Instructions

### swap_sol_buy

**Access:** public

**Accounts:**

| Account | Type | Mutable | Signer | Description |
|---------|------|---------|--------|-------------|
| user | `Signer` | Yes | Yes | User initiating the swap; pays SOL for tax and swap |
| epoch_state | `AccountInfo` | No | No | EpochState PDA from Epoch Program; provides current tax rates |
| swap_authority | `AccountInfo` (PDA) | No | No | Tax Program PDA; signs AMM CPI. Seeds: `["swap_authority"]` |
| tax_authority | `AccountInfo` (PDA) | No | No | Tax Program PDA; signs Staking deposit_rewards CPI. Seeds: `["tax_authority"]` |
| pool | `AccountInfo` | Yes | No | AMM PoolState account; mutable for reserve updates |
| pool_vault_a | `InterfaceAccount<TokenAccount>` | Yes | No | Pool's WSOL vault (Token A) |
| pool_vault_b | `InterfaceAccount<TokenAccount>` | Yes | No | Pool's CRIME/FRAUD vault (Token B) |
| mint_a | `InterfaceAccount<Mint>` | No | No | WSOL mint (NATIVE_MINT) |
| mint_b | `InterfaceAccount<Mint>` | No | No | CRIME or FRAUD mint (Token-2022) |
| user_token_a | `InterfaceAccount<TokenAccount>` | Yes | No | User's WSOL token account |
| user_token_b | `InterfaceAccount<TokenAccount>` | Yes | No | User's CRIME/FRAUD token account |
| stake_pool | `AccountInfo` (PDA) | Yes | No | Staking Program's StakePool PDA; updated by deposit_rewards CPI. Seeds: `["stake_pool"]`, program: Staking |
| staking_escrow | `AccountInfo` (PDA) | Yes | No | Staking escrow; receives 71% of tax. Seeds: `["escrow_vault"]`, program: Staking |
| carnage_vault | `AccountInfo` (PDA) | Yes | No | Carnage Fund vault; receives 24% of tax. Seeds: `["carnage_sol_vault"]`, program: Epoch |
| treasury | `AccountInfo` | Yes | No | Protocol treasury; receives 5% of tax. Address: compile-time constant |
| amm_program | `AccountInfo` | No | No | AMM Program; address-constrained to known program ID |
| token_program_a | `Interface<TokenInterface>` | No | No | SPL Token program (for WSOL) |
| token_program_b | `Interface<TokenInterface>` | No | No | Token-2022 program (for CRIME/FRAUD) |
| system_program | `Program<System>` | No | No | System program for native SOL transfers |
| staking_program | `AccountInfo` | No | No | Staking Program; address-constrained to known program ID |

**remaining_accounts:** Transfer Hook extra accounts (4 per mint involved in the AMM swap). Forwarded to AMM CPI for Token-2022 transfer_checked calls.

**Args:**

| Arg | Type | Description | Constraints |
|-----|------|-------------|-------------|
| amount_in | u64 | Total SOL to spend (including tax) | Must produce sol_to_swap > 0 after tax deduction |
| minimum_output | u64 | Minimum tokens expected (slippage protection) | Must be >= 50% of constant-product expected output |
| is_crime | bool | true = CRIME pool, false = FRAUD pool | Caller-supplied **witness** — runtime cross-checks it against the validated mint and reverts on disagreement (see step 1b). Retained on the IDL for ABI stability. |

**Behavior:**

1. **Validate EpochState.** Owner must be Epoch Program (prevents fake 0% tax accounts). Anchor discriminator validated via `try_deserialize`. `initialized` flag checked.
1b. **Bind tax-side identity to validated mint.** Derives `is_crime` from `mint_b.key()` against the cluster-pinned `crime_mint()` / `fraud_mint()` constants. If `mint_b` is neither, returns `UnknownTaxedMint`. If the derived value disagrees with the caller-supplied flag, returns `TaxIdentityMismatch`. From this point on, the **derived** value is the only source of truth for which tax schedule applies; the caller-supplied flag is purely a witness.
2. **Read tax rate.** Calls `epoch_state.get_tax_bps(derived_is_crime, true)` to get the buy tax rate for the appropriate faction. `derived_is_crime` (not the caller-supplied flag) is used here so on-chain state is the only source of truth.
3. **Calculate tax.** `tax_amount = amount_in * tax_bps / 10_000` (u128 intermediate math, floor division).
4. **Calculate post-tax swap amount.** `sol_to_swap = amount_in - tax_amount`. Rejects if `sol_to_swap == 0`.
5. **Enforce minimum output floor (SEC-10) and bind pool.mint_b to passed mint_b account.** Reads pool reserves and the pool's stored token-side mint via the extended raw byte reader (`read_pool_reserves_with_token_mint`). Verifies the pool's stored token mint matches the passed `mint_b` account; if not, returns `PoolMintMismatch`. Then computes the expected output via constant-product with `sol_to_swap` (post-tax, not `amount_in`); floor = `expected_output * 5000 / 10_000`; rejects if `minimum_output < floor`.
6. **Split tax distribution.** `split_distribution(tax_amount)` returns `(staking_portion, carnage_portion, treasury_portion)` per the 71/24/5 split. Below `MICRO_TAX_THRESHOLD` (4 lamports), all tax goes to staking.
7. **Distribute tax via native SOL transfers.** Three `system_instruction::transfer` CPIs from user to staking_escrow, carnage_vault, treasury. User signature propagates. If staking_portion > 0, also CPIs to Staking Program's `deposit_rewards` to update the `pending_rewards` counter (tax_authority PDA signs).
8. **Snapshot output token balance.** Records `user_token_b.amount` before CPI for accurate balance-diff calculation.
9. **Execute AMM CPI.** Builds raw `swap_sol_pool` instruction with precomputed Anchor discriminator `[0xde, 0x80, 0x1e, 0x7b, 0x55, 0x27, 0x91, 0x8a]`, direction = `AtoB` (0), and `minimum_output` as the AMM's minimum. swap_authority PDA signs via `invoke_signed`. remaining_accounts forwarded for Transfer Hook support.
10. **Compute actual output.** Reloads `user_token_b` and computes `tokens_received = balance_after - balance_before`.
11. **Emit TaxedSwap event** with full breakdown.

**Tax deduction point:** INPUT -- SOL is deducted before swap.

**Error Cases:**

| Error | Code | When | Recovery |
|-------|------|------|----------|
| InvalidEpochState | 6003 | EpochState owner mismatch, bad discriminator, or not initialized | Pass correct EpochState PDA |
| TaxOverflow | 6001 | Arithmetic overflow in tax calculation | Should not occur with valid inputs |
| InsufficientInput | 6004 | sol_to_swap == 0 after tax deduction | Increase amount_in |
| MinimumOutputFloorViolation | 6017 | minimum_output below 50% of expected output | Set minimum_output >= floor |
| SlippageExceeded | 6002 | AMM output < minimum_output | Increase slippage tolerance or retry |
| InvalidPoolOwner | 6018 | Pool account not owned by AMM program | Pass correct pool account |
| TaxIdentityMismatch | 6019 | Caller-supplied `is_crime` does not match the identity derived from the validated `mint_b` account | Pair `is_crime` with the actual mint being swapped |
| PoolMintMismatch | 6020 | Pool's stored token mint does not match the passed `mint_b` account | Pass the `mint_b` account that matches the pool you're swapping against |
| UnknownTaxedMint | 6021 | `mint_b` is not one of the cluster-pinned taxed mints (CRIME / FRAUD) | Use a recognized taxed mint |

**Events Emitted:** `TaxedSwap` -- user, pool_type (derived from validated mint), direction (Buy), input_amount, output_amount, tax_amount, tax_rate_bps, staking/carnage/treasury portions, epoch, slot.

---

### swap_sol_sell

**Access:** public

**Accounts:**

| Account | Type | Mutable | Signer | Description |
|---------|------|---------|--------|-------------|
| user | `Signer` | Yes | Yes | User initiating the swap; signs SPL Token transfer of tax WSOL |
| epoch_state | `AccountInfo` | No | No | EpochState PDA from Epoch Program |
| swap_authority | `AccountInfo` (PDA) | Yes | No | Tax Program PDA; **mutable** because it receives lamports from close_account and distributes them. Seeds: `["swap_authority"]` |
| tax_authority | `AccountInfo` (PDA) | No | No | Tax Program PDA; signs Staking deposit_rewards CPI. Seeds: `["tax_authority"]` |
| pool | `AccountInfo` | Yes | No | AMM PoolState account |
| pool_vault_a | `InterfaceAccount<TokenAccount>` | Yes | No | Pool's WSOL vault |
| pool_vault_b | `InterfaceAccount<TokenAccount>` | Yes | No | Pool's CRIME/FRAUD vault |
| mint_a | `InterfaceAccount<Mint>` | No | No | WSOL mint |
| mint_b | `InterfaceAccount<Mint>` | No | No | CRIME or FRAUD mint (Token-2022) |
| user_token_a | `InterfaceAccount<TokenAccount>` | Yes | No | User's WSOL token account; receives gross AMM output |
| user_token_b | `InterfaceAccount<TokenAccount>` | Yes | No | User's CRIME/FRAUD token account; sends tokens to AMM |
| stake_pool | `AccountInfo` (PDA) | Yes | No | Staking Program's StakePool PDA |
| staking_escrow | `AccountInfo` (PDA) | Yes | No | Staking escrow (71% destination) |
| carnage_vault | `AccountInfo` (PDA) | Yes | No | Carnage Fund vault (24% destination) |
| treasury | `AccountInfo` | Yes | No | Protocol treasury (5% destination) |
| wsol_intermediary | `AccountInfo` (PDA) | Yes | No | Protocol-owned WSOL intermediary for tax extraction. Seeds: `["wsol_intermediary"]` |
| amm_program | `AccountInfo` | No | No | AMM Program |
| token_program_a | `Interface<TokenInterface>` | No | No | SPL Token program |
| token_program_b | `Interface<TokenInterface>` | No | No | Token-2022 program |
| system_program | `Program<System>` | No | No | System program |
| staking_program | `AccountInfo` | No | No | Staking Program |

**remaining_accounts:** Transfer Hook extra accounts forwarded to AMM CPI.

**Args:**

| Arg | Type | Description | Constraints |
|-----|------|-------------|-------------|
| amount_in | u64 | Token amount to sell (CRIME or FRAUD) | Must be > 0 |
| minimum_output | u64 | Minimum SOL to receive AFTER tax | Must be >= 50% of constant-product expected output |
| is_crime | bool | true = CRIME pool, false = FRAUD pool | Caller-supplied **witness** — runtime cross-checks it against the validated mint and reverts on disagreement (see step 1b). Retained on the IDL for ABI stability. |

**Behavior:**

1. **Validate EpochState.** Same triple validation as swap_sol_buy (owner, discriminator, initialized).
1b. **Bind tax-side identity to validated mint.** Same derivation as `swap_sol_buy` step 1b: derive `is_crime` from `mint_b.key()` against the cluster-pinned `crime_mint()` / `fraud_mint()` constants; reject `UnknownTaxedMint` or `TaxIdentityMismatch` accordingly. From here on, the derived value is the only source of truth.
2. **Read tax rate.** `epoch_state.get_tax_bps(derived_is_crime, false)` -- sell direction.
3. **Record WSOL balance before swap.** Snapshots `user_token_a.amount` for balance-diff after CPI.
4. **Enforce minimum output floor (SEC-10) and bind pool.mint_b to passed mint_b account.** For sell (BtoA): `reserve_in = token_reserve`, `reserve_out = sol_reserve`. The extended pool reader also surfaces the pool's stored token-side mint, which is required to match the passed `mint_b` account (else `PoolMintMismatch`). Floor checked against `minimum_output` before CPI executes -- catches bots that send `minimum_output=0` before spending compute.
5. **Compute gross floor for AMM.** `gross_floor = ceil(minimum_output * 10000 / (10000 - tax_bps))`. This is passed as the AMM's `minimum_amount_out` so the AMM rejects swaps that would fail the post-tax slippage check. Prevents wasting compute on swaps doomed to fail.
6. **Execute AMM CPI.** Direction = `BtoA` (1). `minimum_amount_out = gross_floor`. swap_authority PDA signs.
7. **Calculate gross output.** Reloads `user_token_a`, computes `gross_output = wsol_after - wsol_before`.
8. **Calculate tax on gross output.** `tax_amount = gross_output * tax_bps / 10_000`.
9. **Calculate net output and check guards.** `net_output = gross_output - tax_amount`. Rejects if `net_output == 0` (InsufficientOutput). Rejects if `net_output < minimum_output` (SlippageExceeded). Slippage checked AFTER tax deduction.
10. **Split tax distribution.** Same `split_distribution()` as buy path.
11. **Transfer-Close-Distribute-Reinit cycle.** Four-step atomic WSOL tax extraction (see WSOL Intermediary section below).
12. **Emit TaxedSwap event** with net_output as output_amount.

**Tax deduction point:** OUTPUT -- WSOL is deducted after swap.

**Error Cases:**

| Error | Code | When | Recovery |
|-------|------|------|----------|
| InvalidEpochState | 6003 | EpochState validation failure | Pass correct EpochState PDA |
| TaxOverflow | 6001 | Arithmetic overflow | Should not occur with valid inputs |
| MinimumOutputFloorViolation | 6017 | minimum_output below 50% floor | Increase minimum_output |
| InsufficientOutput | 6016 | Tax >= gross output (sell amount too small) | Increase sell amount |
| SlippageExceeded | 6002 | net_output < minimum_output | Increase slippage tolerance |
| InvalidPoolOwner | 6018 | Pool not owned by AMM | Pass correct pool account |

**Events Emitted:** `TaxedSwap` -- same fields as buy, direction = Sell, output_amount = net_output (post-tax).

---

### swap_exempt

**Access:** protocol-only (Epoch Program Carnage authority PDA)

**Accounts:**

| Account | Type | Mutable | Signer | Description |
|---------|------|---------|--------|-------------|
| carnage_authority | `Signer` (PDA) | No | Yes | Carnage authority PDA. Seeds: `["carnage_signer"]`, program: Epoch Program. `seeds::program` constraint enforces only Epoch Program can produce valid signer |
| swap_authority | `AccountInfo` (PDA) | No | No | Tax Program PDA; signs AMM CPI. Seeds: `["swap_authority"]` |
| pool | `AccountInfo` | Yes | No | AMM PoolState account |
| pool_vault_a | `InterfaceAccount<TokenAccount>` | Yes | No | Pool's WSOL vault |
| pool_vault_b | `InterfaceAccount<TokenAccount>` | Yes | No | Pool's CRIME/FRAUD vault |
| mint_a | `InterfaceAccount<Mint>` | No | No | WSOL mint |
| mint_b | `InterfaceAccount<Mint>` | No | No | CRIME or FRAUD mint |
| user_token_a | `InterfaceAccount<TokenAccount>` | Yes | No | Carnage's WSOL token account |
| user_token_b | `InterfaceAccount<TokenAccount>` | Yes | No | Carnage's CRIME/FRAUD token account |
| amm_program | `AccountInfo` | No | No | AMM Program; address-constrained |
| token_program_a | `Interface<TokenInterface>` | No | No | SPL Token program |
| token_program_b | `Interface<TokenInterface>` | No | No | Token-2022 program |
| system_program | `Program<System>` | No | No | System program |

**remaining_accounts:** Transfer Hook extra accounts forwarded to AMM CPI.

**Args:**

| Arg | Type | Description | Constraints |
|-----|------|-------------|-------------|
| amount_in | u64 | Amount to swap | Must be > 0 |
| direction | u8 | 0 = buy (AtoB, SOL->Token), 1 = sell (BtoA, Token->SOL) | Must be 0 or 1 |
| is_crime | bool | true = CRIME pool, false = FRAUD pool | Used for event logging |

**Behavior:**

1. **Validate input.** `amount_in > 0` and `direction <= 1`.
2. **Build and execute AMM CPI.** Same `swap_sol_pool` instruction as taxed swaps. Direction passed through from caller. `minimum_amount_out = 0` (Carnage accepts market execution per Carnage_Fund_Spec Section 9.3). swap_authority PDA signs.
3. **Emit ExemptSwap event** for off-chain monitoring.

**No tax. No slippage protection. Only the AMM's 1% LP fee applies.**

**CPI depth constraint:** This instruction is at depth 1 in the Carnage CPI chain. The full chain (Epoch -> Tax -> AMM -> Token-2022 -> Transfer Hook) is at Solana's 4-level maximum. No additional CPI calls may be added to this path.

**Error Cases:**

| Error | Code | When | Recovery |
|-------|------|------|----------|
| InsufficientInput | 6004 | amount_in == 0 | Pass non-zero amount |
| InvalidPoolType | 6000 | direction > 1 | Pass 0 or 1 |
| (AMM errors) | varies | AMM-level validation failures | Fix account configuration |

**Events Emitted:** `ExemptSwap` -- authority, pool, amount_a, direction, slot.

---

### initialize_wsol_intermediary

**Access:** admin (upgrade authority only)

**Accounts:**

| Account | Type | Mutable | Signer | Description |
|---------|------|---------|--------|-------------|
| admin | `Signer` | Yes | Yes | Admin (payer); must be Tax Program's upgrade authority |
| wsol_intermediary | `AccountInfo` (PDA) | Yes | No | WSOL intermediary PDA; must not exist yet. Seeds: `["wsol_intermediary"]` |
| swap_authority | `AccountInfo` (PDA) | No | No | Will be set as owner of the WSOL token account. Seeds: `["swap_authority"]` |
| mint | `AccountInfo` | No | No | WSOL mint (NATIVE_MINT) |
| token_program | `AccountInfo` | No | No | SPL Token program |
| program | `Program<TaxProgram>` | No | No | Tax Program; used to look up ProgramData address |
| program_data | `Account<ProgramData>` | No | No | ProgramData; `upgrade_authority_address` must match admin |
| system_program | `Program<System>` | No | No | System program |

**Args:** None.

**Behavior:**

1. **Create account at PDA address.** Admin pays rent-exempt minimum (165 bytes = `spl_token::state::Account::LEN`). Intermediary PDA signs via `invoke_signed`.
2. **Initialize as WSOL token account.** Uses `InitializeAccount3` (SPL Token discriminator 18), which takes the owner as instruction data (no rent sysvar needed). Owner set to `swap_authority` PDA.

**Admin check:** The `program_data` constraint verifies `upgrade_authority_address == Some(admin.key())`, ensuring only the program's upgrade authority can call this instruction.

**Must be called once before the first sell swap.** The intermediary is reused across all subsequent sells via the Transfer-Close-Distribute-Reinit cycle.

**Error Cases:**

| Error | Code | When | Recovery |
|-------|------|------|----------|
| ConstraintRaw | Anchor | admin is not upgrade authority | Use correct admin keypair |
| (account already exists) | System | PDA already initialized | No action needed; already set up |

**Events Emitted:** Log message: "WSOL intermediary initialized at {address}".

---

## Accounts

### EpochState (read-only mirror)

**Size:** 164 bytes (data) + 8 bytes (discriminator) = 172 bytes total
**Seeds:** `["epoch_state"]` (derived from Epoch Program, not Tax Program)
**Bump:** Stored in EpochState itself

This is a read-only mirror struct that exactly replicates the Epoch Program's EpochState layout for cross-program deserialization. The Tax Program does not own or write to this account.

| Field | Type | Offset | Description |
|-------|------|--------|-------------|
| genesis_slot | u64 | 0 | Epoch system genesis slot |
| current_epoch | u32 | 8 | Current epoch number |
| epoch_start_slot | u64 | 12 | Slot when current epoch started |
| cheap_side | u8 | 20 | 0 = CRIME is cheap, 1 = FRAUD is cheap |
| low_tax_bps | u16 | 21 | Low tax rate in basis points |
| high_tax_bps | u16 | 23 | High tax rate in basis points |
| crime_buy_tax_bps | u16 | 25 | CRIME buy tax rate (cached) |
| crime_sell_tax_bps | u16 | 27 | CRIME sell tax rate (cached) |
| fraud_buy_tax_bps | u16 | 29 | FRAUD buy tax rate (cached) |
| fraud_sell_tax_bps | u16 | 31 | FRAUD sell tax rate (cached) |
| vrf_request_slot | u64 | 33 | VRF request slot (Tax doesn't use) |
| vrf_pending | bool | 41 | VRF pending flag (Tax doesn't use) |
| taxes_confirmed | bool | 42 | Taxes confirmed flag (Tax doesn't use) |
| pending_randomness_account | Pubkey | 43 | VRF randomness account (Tax doesn't use) |
| carnage_pending | bool | 75 | Carnage pending flag (Tax doesn't use) |
| carnage_target | u8 | 76 | Carnage target (Tax doesn't use) |
| carnage_action | u8 | 77 | Carnage action (Tax doesn't use) |
| carnage_deadline_slot | u64 | 78 | Carnage deadline (Tax doesn't use) |
| carnage_lock_slot | u64 | 86 | Carnage lock slot (Tax doesn't use) |
| last_carnage_epoch | u32 | 94 | Last carnage epoch (Tax doesn't use) |
| reserved | [u8; 64] | 98 | Padding for future schema evolution (DEF-03) |
| initialized | bool | 162 | Whether EpochState has been initialized |
| bump | u8 | 163 | PDA canonical bump |

**Invariants:**

- Struct name must be exactly `EpochState` -- Anchor discriminator is `sha256("account:EpochState")[0..8]`.
- Field layout must match exactly including `#[repr(C)]` and the 64-byte reserved padding.
- **Compile-time assertion (DEF-08):** `const _: () = assert!(EpochState::DATA_LEN == 164);` -- if the Epoch Program changes its layout, Tax Program fails to compile.

**Tax rate lookup:**

```rust
pub fn get_tax_bps(&self, is_crime: bool, is_buy: bool) -> u16 {
    match (is_crime, is_buy) {
        (true, true)   => self.crime_buy_tax_bps,
        (true, false)  => self.crime_sell_tax_bps,
        (false, true)  => self.fraud_buy_tax_bps,
        (false, false) => self.fraud_sell_tax_bps,
    }
}
```

Tax rates are discrete values set by VRF randomness in the Epoch Program: 4 low rates (100/200/300/400 bps = 1-4%) and 4 high rates (1100/1200/1300/1400 bps = 11-14%).

---

## PDAs

The Tax Program derives the following PDAs from its own program ID:

| PDA | Seeds | Purpose |
|-----|-------|---------|
| swap_authority | `["swap_authority"]` | Signs all AMM CPI calls. Derived from Tax Program but recognized by AMM via `seeds::program = TAX_PROGRAM_ID` in the AMM's swap accounts struct. |
| tax_authority | `["tax_authority"]` | Signs Staking Program `deposit_rewards` CPI to update the pending_rewards counter. |
| wsol_intermediary | `["wsol_intermediary"]` | Protocol-owned WSOL token account for sell tax extraction. Owned by swap_authority. Closed and recreated each sell. |

The Tax Program also validates PDAs from other programs:

| PDA | Seeds | Program | Purpose |
|-----|-------|---------|---------|
| staking_escrow | `["escrow_vault"]` | Staking | 71% tax destination |
| carnage_vault | `["carnage_sol_vault"]` | Epoch | 24% tax destination |
| stake_pool | `["stake_pool"]` | Staking | Updated by deposit_rewards CPI |
| carnage_authority | `["carnage_signer"]` | Epoch | swap_exempt caller authentication |
| epoch_state | `["epoch_state"]` | Epoch | Tax rate source |

### SwapAuthority PDA Derivation

The swap_authority PDA is the linchpin of the Tax-AMM integration. It is derived from the **Tax Program** (not the AMM), using seeds `["swap_authority"]`. The AMM's swap instruction validates this PDA via a `seeds::program = TAX_PROGRAM_ID` constraint, ensuring only the Tax Program can authorize swaps. This design means all swap traffic must flow through the Tax Program -- there is no way to bypass taxation by calling the AMM directly.

---

## Tax Calculation

### Rate Source

Tax rates are read from the Epoch Program's EpochState account, which is populated by VRF randomness each epoch. The four cached rate fields (`crime_buy_tax_bps`, `crime_sell_tax_bps`, `fraud_buy_tax_bps`, `fraud_sell_tax_bps`) encode the "tax flip" -- the volume multiplier mechanism where one faction has low rates and the other has high rates, flipping unpredictably via VRF.

### Calculation Formula

```
tax_amount = floor(amount * tax_bps / 10_000)
```

Uses u128 intermediate math to prevent overflow. The function rejects `tax_bps > 10_000` as invalid.

- **Buy path:** Tax is calculated on `amount_in` (SOL input). `sol_to_swap = amount_in - tax_amount`.
- **Sell path:** Tax is calculated on `gross_output` (WSOL received from AMM). `net_output = gross_output - tax_amount`.

### Distribution Split (71/24/5)

| Destination | BPS | Percentage | Calculation | PDA |
|-------------|-----|------------|-------------|-----|
| Staking escrow | 7,100 | 71% | `floor(total_tax * 7100 / 10000)` | `["escrow_vault"]` @ Staking |
| Carnage Fund | 2,400 | 24% | `floor(total_tax * 2400 / 10000)` | `["carnage_sol_vault"]` @ Epoch |
| Treasury | 500 | ~5% | `total_tax - staking - carnage` (remainder) | Compile-time hardcoded pubkey |

Treasury absorbs rounding dust, ensuring the invariant `staking + carnage + treasury == total_tax` always holds.

### Micro-Tax Threshold

When `total_tax < MICRO_TAX_THRESHOLD` (4 lamports), all tax goes to staking. This avoids splitting dust across three destinations where individual portions could round to zero.

```
split_distribution(3) -> (3, 0, 0)  // all to staking
split_distribution(4) -> (2, 0, 2)  // normal split
```

### Staking Notification

After transferring SOL to the staking escrow, the Tax Program CPIs to `Staking::deposit_rewards` (signed by `tax_authority` PDA) to update the `pending_rewards` counter. The SOL is already in escrow; the CPI just updates accounting state. The instruction data is manually constructed: 8-byte discriminator (`sha256("global:deposit_rewards")[0..8]`) + 8-byte amount (u64 LE).

---

## WSOL Intermediary (Sell Path)

The sell path requires a special mechanism because tax must be deducted from WSOL swap output, not the user's native SOL balance. The protocol uses a Transfer-Close-Distribute-Reinit cycle through a PDA-owned WSOL intermediary account.

### Why Not system_instruction::transfer

On the buy path, tax is deducted from SOL input, so native SOL transfers work directly. On the sell path, the AMM outputs WSOL (SPL Token) to the user's WSOL ATA. The tax portion must be extracted from this WSOL and converted to native SOL for distribution. The intermediary pattern handles this conversion atomically.

### Transfer-Close-Distribute-Reinit Cycle

1. **Transfer:** SPL Token `transfer` (discriminator 3) of `tax_amount` WSOL from `user_token_a` to `wsol_intermediary`. User signs (signature propagates from top-level TX).
2. **Close:** SPL Token `close_account` (discriminator 9) closes `wsol_intermediary` to `swap_authority`. This unwraps WSOL to native SOL -- all lamports (token balance + rent) transfer to swap_authority. swap_authority signs as the intermediary's owner.
3. **Distribute:** Three `system_instruction::transfer` CPIs from `swap_authority` to staking_escrow, carnage_vault, and treasury. Only tax portions are distributed; rent lamports are retained for the next step. swap_authority PDA signs all three.
4. **Reinit:** `system_instruction::create_account` creates a new account at the intermediary PDA (165 bytes, rent-exempt funded by swap_authority's retained lamports). Then `InitializeAccount3` (discriminator 18) initializes it as a WSOL token account owned by swap_authority. Both swap_authority (funder) and wsol_intermediary (PDA) sign the create.

This cycle executes atomically within a single transaction. If any step fails, the entire transaction rolls back.

---

## Protocol Minimum Output Floor (SEC-10)

### Purpose

Prevents zero-slippage sandwich attacks where bots or malicious frontends set `minimum_output = 0`. Without this floor, a user could submit a swap with zero slippage protection, making them a trivial sandwich target.

### Calculation

```
expected_output = reserve_out * amount_in / (reserve_in + amount_in)
output_floor = expected_output * MINIMUM_OUTPUT_FLOOR_BPS / 10_000
```

Where `MINIMUM_OUTPUT_FLOOR_BPS = 5000` (50%).

### Buy vs Sell Floor Inputs

- **Buy (AtoB):** `reserve_in = sol_reserve`, `reserve_out = token_reserve`, `amount_in = sol_to_swap` (post-tax). Using `sol_to_swap` (not `amount_in`) is critical because tax is deducted from input before the swap -- using `amount_in` would compute a higher expected output than achievable, making the floor too tight.
- **Sell (BtoA):** `reserve_in = token_reserve`, `reserve_out = sol_reserve`, `amount_in = token_amount`. Checked BEFORE the CPI executes, catching bots early before spending compute on the swap.

### Sell Floor Propagation

On the sell path, the user's `minimum_output` represents the post-tax minimum. The Tax Program computes a gross floor to pass to the AMM:

```
gross_floor = ceil(minimum_output * 10000 / (10000 - tax_bps))
```

This ensures the AMM rejects swaps where the gross output would be too low to satisfy the user's net minimum after tax deduction. This saves compute by failing fast at the AMM level rather than completing the full swap only to fail the post-tax slippage check.

### No LP Fee Adjustment

The 50% floor provides massive tolerance that naturally absorbs the AMM's ~1% LP fee. Raw constant-product math is simpler and sufficient.

---

## Pool Reader

The Tax Program reads AMM pool reserves from raw AccountInfo bytes without importing the AMM crate, avoiding cross-crate coupling.

### PoolState Byte Layout

| Offset | Field | Size | Description |
|--------|-------|------|-------------|
| [0..8] | Anchor discriminator | 8 bytes | Account type identifier |
| [8] | pool_type | 1 byte | Pool type enum |
| [9..41] | mint_a | 32 bytes | Canonical mint A (Pubkey) |
| [41..73] | mint_b | 32 bytes | Canonical mint B (Pubkey) |
| [73..105] | vault_a | 32 bytes | Vault A (Pubkey) |
| [105..137] | vault_b | 32 bytes | Vault B (Pubkey) |
| [137..145] | reserve_a | 8 bytes | Reserve A (u64 LE) |
| [145..153] | reserve_b | 8 bytes | Reserve B (u64 LE) |

### Owner Verification (DEF-01)

Before reading bytes, validates `pool_info.owner == amm_program_id()`. Without this check, an attacker could pass a fake account with arbitrary reserve values to manipulate the slippage floor calculation, potentially making the floor so low that sandwich attacks become profitable.

### is_reversed Detection (DEF-02)

Reads `mint_a` from bytes `[9..41]` and compares to `NATIVE_MINT` (`So11111111111111111111111111111111111111112`):

- `mint_a == NATIVE_MINT`: normal order. `reserve_a` = SOL, `reserve_b` = token.
- `mint_a != NATIVE_MINT`: reversed. `reserve_b` = SOL, `reserve_a` = token.

The function always returns `(sol_reserve, token_reserve)` regardless of canonical ordering. For SOL pools, `NATIVE_MINT` (first byte `0x06`) sorts before all token mints, so `mint_a` is always SOL in practice. The explicit detection is defense-in-depth.

---

## Cross-Program Invocations (CPI)

### Outgoing CPIs

| Target Program | Instruction | When | Signer Seeds |
|---------------|-------------|------|--------------|
| AMM Program | `swap_sol_pool` | Every swap (buy, sell, exempt) | `["swap_authority", bump]` |
| Staking Program | `deposit_rewards` | After staking portion transfer (buy and sell, if staking_portion > 0) | `["tax_authority", bump]` |
| System Program | `transfer` | Tax distribution (buy: user->destinations; sell: swap_authority->destinations) | None (user signs) or `["swap_authority", bump]` |
| SPL Token | `transfer` (3) | Sell: user WSOL -> intermediary | None (user signs) |
| SPL Token | `close_account` (9) | Sell: close intermediary to swap_authority | `["swap_authority", bump]` |
| SPL Token | `InitializeAccount3` (18) | Sell: reinitialize intermediary | None (account already created) |
| System Program | `create_account` | Sell: recreate intermediary after close | `["swap_authority", bump]`, `["wsol_intermediary", bump]` |

### Incoming CPIs (from other programs)

| Source Program | Instruction | Validation |
|---------------|-------------|------------|
| Epoch Program (Carnage) | `swap_exempt` | `carnage_authority` must be PDA with seeds `["carnage_signer"]` from Epoch Program. Enforced by `seeds::program = epoch_program_id()` constraint. |

---

## MEV Defense Properties

The Tax Program's variable per-epoch tax rates provide a natural defense against sandwich attacks. This is not a dedicated MEV defense feature -- it is a beneficial side effect of the yield-generation tax structure.

### Why Sandwiching Is Unprofitable

A sandwich bot must pay tax on both legs:

- **Front-run buy:** 1-14% tax on position size
- **Back-run sell:** 1-14% tax on position size

The round-trip minimum tax cost is 2% (both legs at minimum 1% rate). In practice, one leg faces the higher rate due to the asymmetric tax flip, making the cost 12-28% in many epochs. Typical sandwich profit is 0.1-0.5% of the victim's trade size -- the tax cost dwarfs any possible slippage extraction.

### Asymmetric Rate Amplification

Because epoch tax assignments are determined by on-chain VRF (Switchboard randomness), bots cannot predict which leg will be expensive. Every epoch is unprofitable for sandwich attacks. The 50% minimum output floor (SEC-10) provides additional protection against zero-slippage bots.

---

## Events

### TaxedSwap

Emitted by `swap_sol_buy` and `swap_sol_sell`.

| Field | Type | Description |
|-------|------|-------------|
| user | Pubkey | User who initiated the swap |
| pool_type | PoolType | `SolCrime` or `SolFraud` |
| direction | SwapDirection | `Buy` or `Sell` |
| input_amount | u64 | Amount user put in (SOL for buy, tokens for sell) |
| output_amount | u64 | Amount user received (tokens for buy, net SOL for sell) |
| tax_amount | u64 | Total tax collected (SOL lamports) |
| tax_rate_bps | u16 | Tax rate applied (basis points) |
| staking_portion | u64 | SOL sent to staking escrow (71%) |
| carnage_portion | u64 | SOL sent to Carnage Fund (24%) |
| treasury_portion | u64 | SOL sent to treasury (5%) |
| epoch | u32 | Epoch number when swap occurred |
| slot | u64 | Slot when swap occurred |

### ExemptSwap

Emitted by `swap_exempt`.

| Field | Type | Description |
|-------|------|-------------|
| authority | Pubkey | Carnage authority PDA that initiated the swap |
| pool | Pubkey | AMM pool used for the swap |
| amount_a | u64 | Amount swapped (SOL for buy, token for sell) |
| direction | u8 | 0 = buy (AtoB), 1 = sell (BtoA) |
| slot | u64 | Slot when swap occurred |

---

## Error Codes

Tax Program errors start at Anchor offset 6000:

| Code | Name | Message | When | Recovery |
|------|------|---------|------|----------|
| 6000 | InvalidPoolType | Invalid pool type for this operation | Wrong pool type or direction > 1 | Fix pool or direction parameter |
| 6001 | TaxOverflow | Tax calculation overflow | Arithmetic overflow (should not occur with valid inputs) | Report as bug |
| 6002 | SlippageExceeded | Slippage tolerance exceeded | Net output < minimum_output | Increase slippage tolerance or retry |
| 6003 | InvalidEpochState | Invalid epoch state | EpochState owner mismatch, bad discriminator, or not initialized | Pass correct EpochState PDA |
| 6004 | InsufficientInput | Insufficient input amount | sol_to_swap == 0 or amount_in == 0 | Increase input amount |
| 6005 | OutputBelowMinimum | Output amount below minimum | Net output below minimum (legacy) | Increase slippage tolerance |
| 6006 | InvalidSwapAuthority | Invalid swap authority PDA | swap_authority PDA derivation incorrect | Fix PDA derivation |
| 6007 | WsolProgramMismatch | Token program mismatch (SPL Token) | Expected SPL Token for WSOL operations | Pass correct token program |
| 6008 | Token2022ProgramMismatch | Token program mismatch (Token-2022) | Expected Token-2022 for CRIME/FRAUD | Pass correct token program |
| 6009 | InvalidTokenOwner | Invalid token account owner | Token account owner mismatch | Fix token account ownership |
| 6010 | UnauthorizedCarnageCall | Carnage-only instruction | Non-Carnage caller on swap_exempt | Only Epoch Program can call |
| 6011 | InvalidStakingEscrow | Staking escrow PDA mismatch | Staking escrow address doesn't match PDA derivation | Pass correct PDA |
| 6012 | InvalidCarnageVault | Carnage vault PDA mismatch | Carnage vault address doesn't match PDA derivation | Pass correct PDA |
| 6013 | InvalidTreasury | Treasury address mismatch | Treasury doesn't match compile-time constant | Pass correct treasury address |
| 6014 | InvalidAmmProgram | AMM program address mismatch | AMM program ID doesn't match constant | Pass correct AMM program |
| 6015 | InvalidStakingProgram | Staking program address mismatch | Staking program ID doesn't match constant | Pass correct Staking program |
| 6016 | InsufficientOutput | Tax exceeds gross output | Tax >= gross_output on sell (amount too small) | Sell a larger amount |
| 6017 | MinimumOutputFloorViolation | Minimum output below protocol floor | minimum_output < 50% of expected output | Set minimum_output >= floor |
| 6018 | InvalidPoolOwner | Pool not owned by AMM program | Pool account owner is not AMM program ID | Pass correct pool account |

---

## Constants

| Constant | Value | Type | Description |
|----------|-------|------|-------------|
| SWAP_AUTHORITY_SEED | `"swap_authority"` | &[u8] | Signs AMM CPI. Must match AMM's SWAP_AUTHORITY_SEED. |
| TAX_AUTHORITY_SEED | `"tax_authority"` | &[u8] | Signs Staking deposit_rewards CPI. |
| CARNAGE_SIGNER_SEED | `"carnage_signer"` | &[u8] | Carnage authority PDA from Epoch Program. |
| WSOL_INTERMEDIARY_SEED | `"wsol_intermediary"` | &[u8] | Sell tax extraction intermediary. |
| EPOCH_STATE_SEED | `"epoch_state"` | &[u8] | EpochState PDA from Epoch Program. |
| ESCROW_VAULT_SEED | `"escrow_vault"` | &[u8] | Staking escrow PDA. |
| CARNAGE_SOL_VAULT_SEED | `"carnage_sol_vault"` | &[u8] | Carnage Fund vault PDA. |
| STAKE_POOL_SEED | `"stake_pool"` | &[u8] | StakePool PDA from Staking Program. |
| BPS_DENOMINATOR | 10,000 | u128 | Basis points denominator (10,000 = 100%). |
| STAKING_BPS | 7,100 | u128 | 71% to staking escrow. |
| CARNAGE_BPS | 2,400 | u128 | 24% to Carnage Fund. |
| TREASURY_BPS | 500 | u128 | 5% to treasury (computed as remainder, not used in calculation). |
| MICRO_TAX_THRESHOLD | 4 | u64 | Below this, all tax goes to staking. |
| MINIMUM_OUTPUT_FLOOR_BPS | 5,000 | u64 | 50% minimum output floor. |
| DEPOSIT_REWARDS_DISCRIMINATOR | `[52, 249, 112, 72, 206, 161, 196, 1]` | [u8; 8] | Precomputed `sha256("global:deposit_rewards")[0..8]`. |

### Cross-Program ID Functions

| Function | Returns | Description |
|----------|---------|-------------|
| `amm_program_id()` | `5JsSAL3kJDUWD4ZveYXYZmgm1eVqueesTZVdAvtZg8cR` | AMM Program ID (hardcoded). |
| `epoch_program_id()` | `4Heqc8QEjJCspHR8y96wgZBnBfbe3Qb8N6JBZMQt9iw2` | Epoch Program ID (hardcoded). |
| `staking_program_id()` | `12b3t1cNiAUoYLiWFEnFa4w6qYxVAiqCWU7KZuzLPYtH` | Staking Program ID (hardcoded). |
| `treasury_pubkey()` | Feature-gated | Devnet: `8kPzh...`. Mainnet: `8kPzh...`. Localnet: `Pubkey::default()`. |

---

## Security Considerations

### Fake EpochState Prevention
Every taxed swap validates `epoch_state.owner == epoch_program_id()` before reading tax rates. Without this check, an attacker could create a fake EpochState account with 0% tax rates, bypassing all taxation.

### Pool Spoofing Prevention (DEF-01)
`read_pool_reserves()` validates `pool_info.owner == amm_program_id()` before reading bytes. A spoofed pool with manipulated reserves could make the output floor calculation return 0, disabling sandwich protection.

### Sandwich Attack Protection (SEC-10)
The 50% minimum output floor rejects swaps where `minimum_output < 50% * expected_output`. Combined with the variable 1-14% tax rates, sandwich attacks are unprofitable under every possible tax configuration. See the MEV Defense Properties section.

### Carnage Authority Validation
The `swap_exempt` instruction's `carnage_authority` account uses `seeds::program = epoch_program_id()` to ensure the PDA is derived from the Epoch Program. Only the Epoch Program can produce a valid signer with the `["carnage_signer"]` seeds.

### Treasury Address Hardcoding
The treasury address is validated against a compile-time constant via the `address = treasury_pubkey()` constraint. Feature gating ensures different addresses for devnet, localnet, and mainnet.

### Compile-Time Layout Assertion (DEF-08)
`const _: () = assert!(EpochState::DATA_LEN == 164);` ensures the mirror struct's layout matches the source-of-truth in the Epoch Program. If the Epoch Program changes its layout, the Tax Program will fail to compile.

### Arithmetic Safety
All tax calculations use u128 intermediate math and checked arithmetic operations. Functions return `Option<T>` or `Result<T>` -- no panicking arithmetic. The `calculate_tax` function rejects `tax_bps > 10_000`.

### Manual CPI Construction
The Tax Program builds CPI instructions manually with `invoke_signed` rather than using Anchor SPL's `transfer_checked`. This is because Anchor SPL's `transfer_checked` does not forward `remaining_accounts` to the Token-2022 program, causing Transfer Hook failures. All CPI discriminators are precomputed and hardcoded.

### CPI Depth Budget
The Carnage path (Epoch -> Tax -> AMM -> Token-2022 -> Transfer Hook) is at Solana's 4-level maximum. No additional CPI calls may be added to `swap_exempt`. This is documented as an architectural constraint in the source code.

---

## Compute Budget

| Instruction | Estimated CU | Notes |
|-------------|-------------|-------|
| swap_sol_buy | ~150,000-200,000 | Tax calc + 3 SOL transfers + deposit_rewards CPI + AMM CPI (with Transfer Hook) |
| swap_sol_sell | ~200,000-250,000 | AMM CPI + tax calc + Transfer-Close-Distribute-Reinit cycle (4 SPL Token ops + 3 SOL transfers + deposit_rewards CPI) |
| swap_exempt | ~100,000-150,000 | AMM CPI only (no tax calc, no distribution) |
| initialize_wsol_intermediary | ~50,000 | One-time: create_account + InitializeAccount3 |

The sell path is the most compute-intensive due to the WSOL intermediary cycle. The AMM CPI includes Token-2022 `transfer_checked` with Transfer Hook execution, which adds significant compute. All instructions should include a `SetComputeUnitLimit` instruction in the transaction to avoid default 200k limit on the sell path.

---

## Design Decisions

### Two Swap Lanes
The Tax Program exposes two distinct swap lanes -- taxed (public) and exempt (protocol-only). This separation ensures that Carnage Fund rebalancing does not pay tax on its own recycled funds, while all user swaps are guaranteed to be taxed. The exempt lane is locked to a single PDA signer from the Epoch Program.

### Tax on Input (Buy) vs Output (Sell)
Buy tax is deducted from SOL input before the swap. Sell tax is deducted from WSOL output after the swap. This asymmetry is intentional: buy-side deduction is simpler (native SOL transfers), while sell-side deduction requires the WSOL intermediary pattern because the output is in WSOL format.

### Raw Byte Reading for Pool Reserves
The Tax Program reads AMM pool reserves at known byte offsets rather than importing the AMM crate as a dependency. This avoids tight cross-crate coupling and is the same proven pattern used by the Epoch Program's Carnage code. The AMM crate is listed as a Cargo dependency (for CPI features) but the PoolState struct is never imported.

### Treasury as Remainder
Treasury receives `total_tax - staking - carnage` rather than `floor(total_tax * 500 / 10000)`. This ensures the distribution invariant (sum == total) always holds without rounding errors. Treasury absorbs at most 1-2 lamports of rounding dust.

### 50% Output Floor (Not Tighter)
The 50% floor was chosen as a generous threshold that catches obvious attacks (minimum_output = 0) while not interfering with legitimate trades. At 50%, the AMM's ~1% LP fee is naturally absorbed. A tighter floor (e.g., 95%) would require LP fee adjustment in the calculation and could reject legitimate trades during high volatility.

### Precomputed Discriminators
All CPI discriminators (AMM swap_sol_pool, Staking deposit_rewards, SPL Token transfer/close/InitializeAccount3) are hardcoded as byte arrays rather than computed at runtime. This saves compute and avoids SHA-256 hashing in the hot path.
