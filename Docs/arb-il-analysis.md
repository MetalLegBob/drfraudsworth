# Arbitrage & Impermanent Loss Analysis: Four-Pool Futarchy Architecture

## Document Purpose

This document maps every arbitrage route across the current two-pool and planned four-pool architecture, analyses how IL interacts with the variable tax regime, and provides worked examples for simulation design. It is the foundation for building quantitative simulations of protocol behaviour under different market conditions.

---

## 1. Protocol Mechanics Summary

### 1.1 Current Pool Structure (Two-Pool)

| Pool | Quote | Base | LP Fee | Status |
|------|-------|------|--------|--------|
| CRIME/SOL | WSOL | CRIME | 1% (100 bps) | Active |
| FRAUD/SOL | WSOL | FRAUD | 1% (100 bps) | Active |

### 1.2 Planned Pool Structure (Four-Pool, Futarchy Expansion)

| Pool | Quote | Base | LP Fee | Status |
|------|-------|------|--------|--------|
| CRIME/SOL | WSOL | CRIME | 1% (100 bps) | Active |
| FRAUD/SOL | WSOL | FRAUD | 1% (100 bps) | Active |
| CRIME/USDC | USDC | CRIME | 1% (100 bps) | Planned |
| FRAUD/USDC | USDC | FRAUD | 1% (100 bps) | Planned |

### 1.3 Tax Regime (VRF-Driven, 30-Minute Epochs)

Each epoch, VRF determines independently for each token:

**Cheap side (buy incentivised):**
- Buy tax: 1-4% (100-400 bps, VRF byte selects magnitude)
- Sell tax: 11-14% (1100-1400 bps, VRF byte selects magnitude)

**Expensive side (sell incentivised):**
- Buy tax: 11-14% (1100-1400 bps)
- Sell tax: 1-4% (100-400 bps)

**Flip probability:** 75% per epoch (byte 0 < 192)

**Tax parity rule (four-pool):** CRIME/SOL and CRIME/USDC always have identical tax rates. Same for FRAUD. Tax rates are per-token, not per-pool.

### 1.4 Tax Application

- **Buy path:** Tax deducted from INPUT SOL *before* AMM swap. User sends X SOL, protocol takes tax, AMM receives (X - tax).
- **Sell path:** Tax deducted from OUTPUT SOL *after* AMM swap. AMM outputs Y SOL, protocol takes tax, user receives (Y - tax).
- **LP fee (1%):** Applied inside the AMM swap calculation, separate from tax. Stays in pool reserves permanently.

### 1.5 Tax Distribution (Immutable)

```
71% → Staking rewards (SOL yield for PROFIT stakers)
24% → Carnage fund (buyback-and-burn)
 5% → Treasury (operations)
```

### 1.6 Conversion Vault

- Fixed rate: 100 CRIME = 1 PROFIT, 100 FRAUD = 1 PROFIT (and reverse)
- Zero fees, zero slippage
- Enables CRIME ↔ FRAUD arbitrage via: CRIME → PROFIT → FRAUD (or reverse)
- Round-trip vault friction: 0% (but loses `N % 100` lamports as dust)

### 1.7 Constant-Product AMM Formula

```
effective_input = amount_in × (10000 - 100) / 10000    // 1% LP fee
output = reserve_out × effective_input / (reserve_in + effective_input)
```

Key property: `k_after >= k_before` always. Rounding favours protocol.

### 1.8 Protocol Output Floor

50% minimum output on all user swaps. If output < 50% of expected, transaction fails. This prevents sandwich attacks but does NOT apply to Carnage (swap_exempt path).

---

## 2. Arbitrage Routes

### 2.1 Current Architecture (Two-Pool): The Organic Volume → Flip → Arb Cycle

#### Route A: Cross-Token Soft-Peg Arbitrage (CRIME ↔ FRAUD via Vault)

Cross-token arb does NOT happen spontaneously from the soft peg. It is the product of a **three-phase cycle** driven by organic trading volume and tax regime flips:

#### Phase 1: Organic Volume Pushes Prices Apart (During Current Epoch)

When CRIME is on the cheap side (low buy tax) and FRAUD is on the expensive side (high buy tax), rational traders preferentially:
- **Buy CRIME** (cheap to buy at 1-4% tax) → CRIME price gets pushed UP
- **Sell FRAUD** (cheap to sell at 1-4% tax) → FRAUD price gets pushed DOWN

This organic volume creates a **price divergence** between CRIME/SOL and FRAUD/SOL. The divergence is bounded — traders stop buying CRIME when the price has risen enough that even the low buy tax makes further buying unattractive, and similarly for FRAUD selling. The **expensive tax side acts as a ceiling/floor** that limits how far prices can diverge (buying CRIME becomes unprofitable when the sell tax of 11-14% exceeds the expected gain, and vice versa).

The divergence accumulates over the epoch (30 minutes) and potentially across multiple consecutive same-direction epochs if the VRF doesn't flip (25% chance of no flip per epoch).

#### Phase 2: Tax Regime Flips (75% Probability Per Epoch)

The VRF triggers a flip. Now:
- CRIME (which was cheap to buy, and got pushed UP) is now **cheap to SELL** (1-4% sell tax)
- FRAUD (which was cheap to sell, and got pushed DOWN) is now **cheap to BUY** (1-4% buy tax)

The accumulated price divergence from Phase 1 is now sitting behind the LOWEST possible friction window.

#### Phase 3: Arb Closes the Spread

The arb opportunity is now:
```
1. Buy FRAUD (pushed down, now cheap to buy: 1-4% buy tax + 1% LP)
2. Convert via vault: 100 FRAUD → 1 PROFIT → 100 CRIME (0% fee)
3. Sell CRIME (pushed up, now cheap to sell: 1-4% sell tax + 1% LP)
```

**This is always the best-case friction scenario** because the arb naturally executes in the CE/EC state created by the flip:

```
Arb friction (post-flip):
  Buy tax (now-cheap token):  1-4%
  LP fee (buy pool):          1%
  Vault:                      0%
  LP fee (sell pool):         1%
  Sell tax (now-cheap token): 1-4%
  Total:                      4-10%
```

The arb closes the spread back toward parity. If the spread was 8% and friction is 5%, the arber captures ~3% profit. The protocol captures tax revenue on both legs plus LP fees on both legs.

#### What Happens If There's No Flip?

If the VRF doesn't flip (25% chance), the same tax regime continues. Organic volume continues pushing prices in the same direction, but at a diminishing rate (the spread approaches the expensive-side ceiling). No arb fires because the friction on the arb direction is still HIGH (the token you'd want to buy is still expensive to buy, the token you'd want to sell is still expensive to sell).

The divergence accumulates further, meaning the NEXT flip creates an even larger arb opportunity.

#### What Happens If There's No Organic Volume?

If nobody trades organically between flips, the CRIME/FRAUD prices stay at parity. When the tax flips, there's no divergence to close. **No arb opportunity exists.** The arb is entirely dependent on organic volume creating the spread that the flip then unlocks.

This is a critical property: **the protocol only "leaks" IL when it has first collected tax revenue from the organic volume that created the divergence.** Volume → tax revenue → divergence → flip → arb (which leaks some value back). The leak is always a fraction of the revenue that caused it.

#### Full Cycle Diagram

```
Epoch N (CRIME cheap, FRAUD expensive):
  Organic traders buy CRIME (low tax) → CRIME price ↑
  Organic traders sell FRAUD (low tax) → FRAUD price ↓
  Tax revenue collected on every trade (71/24/5 split)
  Spread accumulates: CRIME 5% above parity, FRAUD 5% below
                                    │
                                    ▼
Epoch N+1 (VRF flips → CRIME expensive, FRAUD cheap):
  CRIME is now cheap to SELL (was pushed up, sell tax only 1-4%)
  FRAUD is now cheap to BUY (was pushed down, buy tax only 1-4%)
  Arb window opens at 4-10% friction
  Spread of ~10% > friction of ~5% → arb profitable
                                    │
                                    ▼
  Arber: Buy FRAUD (cheap) → vault → Sell CRIME (cheap)
  Arber profit: ~5% (spread - friction)
  Protocol revenue: taxes on both legs + LP fees on both legs
  Spread closes back toward parity
                                    │
                                    ▼
Epoch N+2 (next regime):
  Cycle begins again from new baseline
  Pools are slightly deeper (LP fees compound)
  CRIME/FRAUD supply slightly lower (if Carnage fired)
```

#### Friction Table (Post-Flip Arb — The Only Scenario Where Arb Naturally Fires)

Because arb only fires after a flip, the arber is always buying the now-cheap-to-buy token and selling the now-cheap-to-sell token:

| Buy Tax (now-cheap) | Sell Tax (now-cheap) | LP (×2) | Vault | Total Friction |
|---------------------|----------------------|---------|-------|----------------|
| 1% | 1% | 2% | 0% | **4%** |
| 2% | 3% | 2% | 0% | **7%** |
| 4% | 4% | 2% | 0% | **10%** |
| 1% | 4% | 2% | 0% | **7%** |

**Realistic friction range: 4-10%.** The 14-30% friction scenarios from a naive analysis don't occur naturally because no rational arber would execute against the expensive tax side — the flip creates the window where both legs are cheap.

**Arb is profitable when:** accumulated organic spread > post-flip friction (4-10%)

**Arb does NOT fire when:**
- No organic volume pushed prices apart (no spread to close)
- Spread < 4% (friction exceeds opportunity)
- No flip occurred (arb direction still faces expensive taxes)

### 2.2 Four-Pool Architecture: 6 Arb Routes

With four pools, the arbitrage surface expands to six distinct routes:

#### Route 1: CRIME/SOL ↔ CRIME/USDC (Cross-Denomination, Same Token)

**Trigger:** SOL/USD price movement causes CRIME to have different USD-implied prices across denominations.

**Execution (SOL pumps scenario):**
```
1. Buy CRIME from USDC pool (cheaper in USD terms)    [pay: CRIME buy tax + 1% LP]
2. Sell CRIME into SOL pool (more expensive in USD terms) [pay: CRIME sell tax + 1% LP]
3. Swap SOL → USDC on Jupiter                          [pay: ~0.1%]
```

**Friction:**
| CRIME Tax Side | Buy Tax | LP In | LP Out | Sell Tax | Jupiter | Total |
|----------------|---------|-------|--------|----------|---------|-------|
| Cheap side | 1-4% | 1% | 1% | 11-14% | ~0.1% | **14.1-20.1%** |
| Expensive side | 11-14% | 1% | 1% | 1-4% | ~0.1% | **14.1-20.1%** |

**Critical observation:** Because both pools have identical CRIME tax rates (tax parity), cross-denomination arb always costs the same total tax regardless of which side CRIME is on. The buy tax on one side is always paired with the opposite sell tax. Total tax = low + high = 12-18%. Plus 2% LP + 0.1% Jupiter = **14.1-20.1%** always.

This means cross-denomination arb only fires on large SOL/USD moves (>14-20%).

#### Route 2: FRAUD/SOL ↔ FRAUD/USDC (Cross-Denomination, Same Token)

Identical mechanics to Route 1, substituting FRAUD tax rates for CRIME.

**Friction: 14.1-20.1%** (same range — tax parity means buy+sell always sums to low+high)

#### Route 3: CRIME/SOL ↔ FRAUD/SOL (Cross-Token, Same Denomination — Existing)

This is Route A from the two-pool architecture, operating on the SOL side.

**Friction: 4-30%** (variable, depends on relative tax sides of CRIME vs FRAUD)

#### Route 4: CRIME/USDC ↔ FRAUD/USDC (Cross-Token, Same Denomination — New)

Same mechanics as Route 3, but operating on the USDC side.

**Execution:**
```
1. Buy cheap token from USDC pool          [pay: buy tax + 1% LP]
2. Convert via vault (100:1 → PROFIT → 100:1) [pay: 0%]
3. Sell expensive token into USDC pool     [pay: sell tax + 1% LP]
```

**Friction: 4-30%** (same as Route 3 — vault is denomination-agnostic)

#### Route 5: CRIME/SOL ↔ FRAUD/USDC (Cross-Denomination + Cross-Token)

**Trigger:** Combined SOL/USD movement AND tax regime asymmetry between CRIME/FRAUD.

**Execution (example: CRIME cheap on SOL side, FRAUD expensive on USDC side):**
```
1. Buy CRIME from SOL pool (cheap buy tax)     [pay: CRIME buy tax + 1% LP]
2. Convert: 100 CRIME → 1 PROFIT → 100 FRAUD  [pay: 0%]
3. Sell FRAUD into USDC pool (cheap sell tax)   [pay: FRAUD sell tax + 1% LP]
4. Swap USDC → SOL on Jupiter (to repeat)      [pay: ~0.1%]
```

**Friction:** Depends on CRIME buy + FRAUD sell + 2% LP + 0.1% Jupiter.

| CRIME Side | FRAUD Side | CRIME Buy | FRAUD Sell | LP | Jupiter | Total |
|------------|------------|-----------|------------|-----|---------|-------|
| Cheap | Cheap | 1-4% | 11-14% | 2% | 0.1% | **14.1-20.1%** |
| Cheap | Expensive | 1-4% | 1-4% | 2% | 0.1% | **4.1-10.1%** |
| Expensive | Cheap | 11-14% | 11-14% | 2% | 0.1% | **24.1-30.1%** |
| Expensive | Expensive | 11-14% | 1-4% | 2% | 0.1% | **14.1-20.1%** |

**Best case: 4.1%** when CRIME is cheap (low buy) and FRAUD is expensive (low sell).

#### Route 6: FRAUD/SOL ↔ CRIME/USDC (Cross-Denomination + Cross-Token)

Mirror of Route 5. Same friction analysis, swapping CRIME and FRAUD roles.

---

## 3. Arb Friction Summary Matrix

### 3.1 All Routes by Tax Regime State

The system has four possible tax regime states (CRIME and FRAUD each independently cheap or expensive):

| State | CRIME Side | FRAUD Side | Probability |
|-------|------------|------------|-------------|
| CC | Cheap | Cheap | ~25% |
| CE | Cheap | Expensive | ~25% |
| EC | Expensive | Cheap | ~25% |
| EE | Expensive | Expensive | ~25% |

(Probabilities approximate — 75% flip chance per token per epoch means the distribution varies over time but averages roughly 25% each)

### 3.2 Minimum Friction by Route and State

| Route | CC State | CE State | EC State | EE State |
|-------|----------|----------|----------|----------|
| 1. CRIME cross-denom | 14.1-20.1% | 14.1-20.1% | 14.1-20.1% | 14.1-20.1% |
| 2. FRAUD cross-denom | 14.1-20.1% | 14.1-20.1% | 14.1-20.1% | 14.1-20.1% |
| 3. CRIME↔FRAUD SOL | 14-20% | **4-10%** | **24-30%** | 14-20% |
| 4. CRIME↔FRAUD USDC | 14-20% | **4-10%** | **24-30%** | 14-20% |
| 5. CRIME/SOL↔FRAUD/USDC | 14.1-20.1% | **4.1-10.1%** | 24.1-30.1% | 14.1-20.1% |
| 6. FRAUD/SOL↔CRIME/USDC | 14.1-20.1% | 24.1-30.1% | **4.1-10.1%** | 14.1-20.1% |

**Key takeaways:**
- **Cross-denomination routes (1,2):** Always ~14-20% friction. Only fire on large SOL/USD moves.
- **Cross-token routes (3,4):** Variable 4-30%. Cheapest in CE/EC states when tokens are on opposite sides.
- **Combined routes (5,6):** Variable 4-30%. Route 5 cheapest in CE state, Route 6 cheapest in EC state.
- **In CE or EC states, THREE routes open at 4-10% friction simultaneously** (Route 3 or 4 + Route 5 or 6).
- **In CC or EE states, all routes are 14%+ friction.** The protocol is maximally protected.

### 3.3 IL Shield Strength by Tax State

Understanding from the organic volume → flip → arb cycle: **arb fires AFTER a flip, not during a state.** The relevant question is: which state CREATES divergence (Phase 1) and which state ALLOWS arb (Phase 3)?

```
CE state (CRIME cheap buy, FRAUD cheap sell):
  DIVERGENCE CREATION: Strong
    - Organic traders buy CRIME (cheap) → price UP
    - Organic traders sell FRAUD (cheap) → price DOWN
    - Spread accumulates throughout epoch(s)
  ARB EXECUTION: Only if flipped FROM a state that created divergence
    - If previous state was EC, spread exists in opposite direction
    - Post-flip friction: 4-10% (both legs on cheap side)
    - Arb fires if accumulated spread > 4-10%

EC state (CRIME cheap sell, FRAUD cheap buy):
  Mirror of CE. Creates divergence in opposite direction.

CC state (both cheap to buy):
  DIVERGENCE CREATION: Weak
    - Both tokens incentivise buying → both prices pushed UP
    - Less relative divergence between CRIME and FRAUD
    - Some divergence from magnitude differences (one gets bought more)
  ARB EXECUTION: Weak
    - If previous state created divergence, arb must buy one (cheap)
      and sell the other (also cheap to buy = EXPENSIVE to sell)
    - One leg faces 11-14% sell tax → friction 14-20%
    - Only fires on very large accumulated spreads

EE state (both cheap to sell):
  Mirror of CC. Both pushed down, less relative divergence.

FOUR-POOL CROSS-DENOMINATION (Routes 1-2):
  All states: 14.1-20.1% friction (tax parity means buy+sell = low+high always)
  Only fires on large SOL/USD moves (>14-20%)
  Tax regime state is irrelevant for same-token cross-denomination arb
```

**The cycle that generates the MOST arb (and therefore most IL risk) is:**
```
CE epoch(s) → EC flip:  CRIME pushed up, FRAUD pushed down → flip → arb buys FRAUD, sells CRIME
EC epoch(s) → CE flip:  FRAUD pushed up, CRIME pushed down → flip → arb buys CRIME, sells FRAUD
```

**These CE↔EC transitions are the protocol's primary volume engine.** They simultaneously:
1. Generate tax revenue from organic volume (Phase 1)
2. Generate tax revenue from arb volume (Phase 3)
3. Compound LP fees into pool depth (both phases)
4. Create IL (arber's net profit after friction)

The simulation must answer: does (1 + 2 + 3) > (4) across realistic time horizons?

---

## 4. Worked Examples

### 4.1 The Organic Volume → Flip → Arb Cycle — Two-Pool Worked Example

**Setup:**
```
CRIME/SOL pool: 500 SOL + 200M CRIME
FRAUD/SOL pool: 500 SOL + 200M FRAUD
SOL price: $150 (constant for this example — SOL movement is irrelevant to
cross-token arb in the two-pool system)

Epoch N: CRIME cheap (buy 2%, sell 13%), FRAUD expensive (buy 12%, sell 3%)
```

**Phase 1: Organic volume during Epoch N**

Traders see CRIME is cheap to buy and start buying:
```
Trader buys 10 SOL worth of CRIME:
  Input: 10 SOL
  After 2% buy tax: 9.8 SOL enters AMM (0.2 SOL → protocol as tax)
  After 1% LP fee: effective input = 9.702 SOL
  CRIME received: 200M × 9.702 / (500 + 9.702) = 3.808M CRIME

  CRIME pool after: 509.702 SOL + 196.192M CRIME
  Implied CRIME/SOL price INCREASED (more SOL per CRIME)
```

Traders see FRAUD is cheap to sell and start selling:
```
Trader sells 3.808M FRAUD:
  AMM output: 500 × (3.808M × 0.99) / (200M + 3.808M × 0.99) = 9.272 SOL
  After 3% sell tax: 9.272 × 0.97 = 8.994 SOL to trader (0.278 SOL → protocol)

  FRAUD pool after: 490.728 SOL + 203.770M FRAUD
  Implied FRAUD/SOL price DECREASED (less SOL per FRAUD)
```

**After Epoch N organic volume:**
```
CRIME/SOL: 509.702 SOL + 196.192M CRIME
  Implied price: 509.702/196.192M = 0.000002598 SOL/CRIME

FRAUD/SOL: 490.728 SOL + 203.770M FRAUD
  Implied price: 490.728/203.770M = 0.000002408 SOL/FRAUD

Price divergence: (0.000002598 - 0.000002408) / 0.000002408 = 7.9%

Tax revenue collected during this organic volume:
  CRIME buy tax:  0.2 SOL (71/24/5 split)
  FRAUD sell tax: 0.278 SOL (71/24/5 split)
  Total: 0.478 SOL (before this arb cycle even begins)
```

**Phase 2: VRF flips at Epoch N+1**

New regime: CRIME expensive (buy 12%, sell 3%), FRAUD cheap (buy 2%, sell 13%)

Now CRIME is cheap to SELL (3% tax) and FRAUD is cheap to BUY (2% tax). The 7.9% spread is sitting behind a ~7% friction window (2% + 1% + 0% + 1% + 3%).

**Phase 3: Arb fires**
```
Step 1: Buy FRAUD (pushed down, now cheap to buy at 2% tax)
  Input: 10 SOL
  After 2% buy tax: 9.8 SOL → AMM (0.2 SOL → protocol)
  After 1% LP fee: effective = 9.702 SOL
  FRAUD received: 203.770M × 9.702 / (490.728 + 9.702) = 3.951M FRAUD

Step 2: Convert via vault
  3.951M FRAUD → 39,510 PROFIT → 3.951M CRIME (0% fee, ignoring dust)

Step 3: Sell CRIME (pushed up, now cheap to sell at 3% tax)
  AMM output: 509.702 × (3.951M × 0.99) / (196.192M + 3.951M × 0.99)
           = 509.702 × 3.911M / 200.103M
           = 9.968 SOL
  After 3% sell tax: 9.968 × 0.97 = 9.669 SOL to arber (0.299 SOL → protocol)

Arber P&L:
  Spent: 10 SOL
  Received: 9.669 SOL
  Loss: 0.331 SOL (3.3%)
```

**Hmm — the arb is NOT profitable in this example.** The 7.9% spread wasn't quite enough to overcome ~7% friction with the slippage from pool depth. With a larger accumulated spread (more organic volume, or multiple epochs without a flip), or at larger pool depths (less slippage), it would cross into profitability.

**Let's try with more organic volume (spread pushed to 12%):**

If multiple epochs of organic trading push CRIME up 12% and FRAUD down 12% relative to each other (24% total divergence), the post-flip arb would look like:
```
Gross opportunity: ~24% price spread
Friction: ~7% (2% + 1% + 0% + 1% + 3%)
Slippage: ~2-3% (pool depth dependent)
Net arber profit: ~14-15%

Protocol revenue from the arb trade:
  Buy tax: 2% of input
  Sell tax: 3% of output
  LP fees: 1% retained in each pool
  Total: ~7% of trade size → protocol
```

**Key insight from this example:**
1. The protocol collects tax revenue from BOTH the organic volume that creates the spread AND the arb trade that closes it
2. The arb only fires when the accumulated spread significantly exceeds the post-flip friction
3. Small spreads (<7%) never get arbed — the protocol keeps the full divergence as "stored energy" until a large enough spread accumulates
4. The 4-10% post-flip friction acts as a minimum spread threshold below which no value leaks

**Critical property:** In the two-pool system, SOL/USD price movements do NOT trigger arb. Both pools hold SOL, so SOL appreciation/depreciation affects both equally. Cross-token arb is purely driven by the organic volume → flip cycle. **This changes fundamentally in the four-pool architecture** where SOL/USD movement creates cross-denomination arb opportunities.

### 4.2 SOL Pumps 10% — Four-Pool Architecture

**Setup:**
```
Futarchy allocation: 70% SOL / 30% USDC
Total protocol liquidity: $200K quote-side equivalent

CRIME/SOL pool:   350 SOL ($52.5K) + 150M CRIME
FRAUD/SOL pool:   350 SOL ($52.5K) + 150M FRAUD
CRIME/USDC pool:  $15K USDC + 43M CRIME
FRAUD/USDC pool:  $15K USDC + 43M FRAUD

SOL price: $150 → $165 (10% increase)

Current epoch: CRIME cheap (buy 2%, sell 13%), FRAUD expensive (buy 12%, sell 3%)
```

**After SOL pumps, before any arb:**

```
SOL pools: SOL appreciated but reserves unchanged
  CRIME/SOL: 350 SOL (now worth $57.75K) + 150M CRIME
  FRAUD/SOL: 350 SOL (now worth $57.75K) + 150M FRAUD

USDC pools: Completely unaffected
  CRIME/USDC: $15K USDC + 43M CRIME
  FRAUD/USDC: $15K USDC + 43M FRAUD

Implied CRIME prices:
  SOL pool:  350 SOL / 150M = 0.00000233 SOL = $0.000385/CRIME (at new $165 SOL)
  USDC pool: $15,000 / 43M = $0.000349/CRIME

Price gap: $0.000385 vs $0.000349 = 10.3% difference
```

**Route 1 arb (CRIME cross-denomination): Can it fire?**

```
Friction (CRIME is on cheap side):
  Buy CRIME from USDC pool: 2% buy tax + 1% LP = 3%
  Sell CRIME into SOL pool: 13% sell tax + 1% LP = 14%
  Jupiter USDC→SOL swap: ~0.1%
  Total friction: ~17.1%

Price gap: 10.3%
Friction: 17.1%

RESULT: NOT PROFITABLE. Gap < friction.
```

SOL needs to pump ~17%+ for this arb to fire when CRIME is on the cheap side (low buy but high sell).

**But wait — what if CRIME were on the expensive side?**

```
Friction (CRIME on expensive side):
  Buy CRIME from USDC pool: 12% buy tax + 1% LP = 13%
  Sell CRIME into SOL pool: 3% sell tax + 1% LP = 4%
  Jupiter: ~0.1%
  Total friction: ~17.1%

SAME total friction! (Tax parity: buy + sell always sums to low + high)
```

This confirms: **cross-denomination arb friction is tax-regime-invariant for same-token routes.** The tax side doesn't matter — you always pay one low + one high. Only the total low+high range matters (12-18% tax + 2% LP + 0.1% Jupiter = 14.1-20.1%).

### 4.3 SOL Pumps 20% — Four-Pool Architecture

**Same setup as 4.2, SOL: $150 → $180**

```
Implied CRIME prices:
  SOL pool: 350 SOL / 150M = 0.00000233 SOL = $0.000420/CRIME (at $180)
  USDC pool: $15,000 / 43M = $0.000349/CRIME

Price gap: 20.3%
Minimum friction: 14.1% (at best tax rates: 1% + 1% + 1% + 11% + 0.1%)

RESULT: PROFITABLE at ~6.2% margin (20.3% - 14.1%)
```

**Arb execution (Route 1, CRIME cheap side):**

```
Step 1: Buy CRIME from USDC pool
  Input: $5,000 USDC
  After 2% buy tax: $4,900 available for swap
  After 1% LP fee in AMM: effective input = $4,851
  CRIME received: 43M × $4,851 / ($15,000 + $4,851) = 10.51M CRIME

  USDC pool after: $19,851 USDC + 32.49M CRIME (deeper in USDC)

Step 2: Sell CRIME into SOL pool
  Input: 10.51M CRIME
  After 1% LP fee in AMM: effective input = 10.40M CRIME
  SOL received (pre-tax): 350 × 10.40M / (150M + 10.40M) = 22.70 SOL
  After 13% sell tax: 22.70 × 0.87 = 19.75 SOL

  SOL pool after: 327.30 SOL + 160.51M CRIME (shallower in SOL)

Step 3: Convert 19.75 SOL → USDC on Jupiter
  At $180/SOL: 19.75 × $180 × 0.999 = $3,551 USDC (after 0.1% Jupiter fee)

Wait — we spent $5,000 and got back $3,551?
```

**Something's wrong. Let me recalculate more carefully.**

The issue is I was computing wrong. The arb profit comes from the *price difference*, not from round-tripping. Let me redo:

```
Step 1: Buy CRIME from USDC pool (cheap in USD terms: $0.000349/CRIME)
  Input: $1,000 USDC
  After 2% buy tax: $980
  AMM: CRIME out = 43M × ($980 × 0.99) / ($15,000 + $980 × 0.99)
     = 43M × $970.20 / $15,970.20
     = 2.613M CRIME

Step 2: Sell CRIME into SOL pool (expensive in USD terms: $0.000420/CRIME)
  AMM: SOL out = 350 × (2.613M × 0.99) / (150M + 2.613M × 0.99)
     = 350 × 2.587M / 152.587M
     = 5.934 SOL
  After 13% sell tax: 5.934 × 0.87 = 5.163 SOL
  USD value: 5.163 × $180 = $929.28

Step 3: Swap 5.163 SOL → USDC on Jupiter
  $929.28 × 0.999 = $928.35 USDC

RESULT: Spent $1,000, received $928.35. LOSS of $71.65 (7.2%)
```

**Still not profitable!** Even with a 20% SOL pump. Why?

Because the 13% sell tax on the SOL pool side is devastating. Let me check the opposite direction:

```
ALTERNATIVE: Buy CRIME from SOL pool, sell into USDC pool

Step 1: Buy CRIME from SOL pool (expensive in USD but paying in SOL)
  Input: 5 SOL (= $900 at $180)
  After 2% buy tax: 4.9 SOL
  AMM: CRIME out = 150M × (4.9 × 0.99) / (350 + 4.9 × 0.99)
     = 150M × 4.851 / 354.851
     = 2.051M CRIME

Step 2: Sell CRIME into USDC pool (cheap in USD terms)
  AMM: USDC out = $15,000 × (2.051M × 0.99) / (43M + 2.051M × 0.99)
     = $15,000 × 2.030M / 45.030M
     = $676.22
  After 13% sell tax: $676.22 × 0.87 = $588.30

RESULT: Spent $900 (5 SOL), received $588.30. Much worse — wrong direction.
```

**The correct arb direction when SOL pumps:**

When SOL pumps, CRIME is more expensive in the SOL pool (in USD terms). So you want to:
- Buy CRIME where it's cheap (USDC pool)
- Sell CRIME where it's expensive (SOL pool)
- Convert SOL back to USDC

But the **sell tax kills it** when the sell tax is high (13%). Let's try when CRIME is on the *expensive* side (high buy, low sell):

```
CRIME on expensive side: buy 12%, sell 3%

Step 1: Buy CRIME from USDC pool
  Input: $1,000 USDC
  After 12% buy tax: $880
  AMM: CRIME out = 43M × ($880 × 0.99) / ($15,000 + $880 × 0.99)
     = 43M × $871.20 / $15,871.20
     = 2.361M CRIME

Step 2: Sell CRIME into SOL pool
  AMM: SOL out = 350 × (2.361M × 0.99) / (150M + 2.361M × 0.99)
     = 350 × 2.337M / 152.337M
     = 5.370 SOL
  After 3% sell tax: 5.370 × 0.97 = 5.209 SOL
  USD value: 5.209 × $180 = $937.58

Step 3: Jupiter SOL→USDC: $937.58 × 0.999 = $936.64

RESULT: Spent $1,000, received $936.64. LOSS of $63.36 (6.3%)
```

**Still a loss!** Slightly better (6.3% vs 7.2%) but still negative.

### 4.4 So When Does Cross-Denomination Arb Actually Fire?

Let's find the breakeven SOL move. The issue is that the tax applies to both legs (buy AND sell), and total tax friction is always 12-18% regardless of which side CRIME is on.

```
Minimum friction: 1% buy tax + 1% LP + 1% LP + 11% sell tax + 0.1% Jupiter
                = 14.1%

But this minimum requires CRIME at 1% buy AND 11% sell simultaneously.
Since buy=1% means cheap side, sell must be HIGH (11-14%).
So minimum realistic is: 1% + 1% + 1% + 11% + 0.1% = 14.1%
```

For a 14.1% friction trade to break even, the price gap needs to be at least 14.1%. But there's also slippage from pool depth. For thin pools, the actual breakeven is even higher.

**For the specific pool sizes in our example ($15K USDC pool, 350 SOL pool), a conservative estimate is that SOL needs to move ~18-25% in a single epoch for cross-denomination arb to be profitable.**

### 4.5 The Temporal Dimension: Pressure and Release

Here's where it gets interesting. Imagine this sequence:

```
Epoch 1 (CE state, CRIME buy 2%, sell 13%):
  SOL pumps 5%. Gap opens to 5%.
  Friction = 14.1% minimum. No arb fires.
  Gap ACCUMULATES.

Epoch 2 (CE state continues, no flip):
  SOL pumps another 5%. Gap now 10%.
  Friction still 14.1%. No arb fires.
  Gap ACCUMULATES further.

Epoch 3 (EE state, both flip to expensive):
  SOL pumps another 5%. Gap now ~15%.
  Friction still 14.1%. BARELY at threshold.
  Small arb might fire on the margin.

Epoch 4 (CE state again):
  SOL stable. Gap still ~15%.
  Same friction. Arb slowly closes gap.

Epoch 5 (sudden flip to EC state):
  CRIME now expensive (buy 12%, sell 3%)
  FRAUD now cheap
  Friction recalculates but for cross-denom it's still 14.1%+
  However, Route 5 (CRIME/SOL → FRAUD/USDC) is now
  EC state = expensive CRIME × cheap FRAUD = 24.1-30.1%
  WORSE for combined routes.

  Meanwhile Route 6 (FRAUD/SOL → CRIME/USDC) is now:
  EC state = cheap FRAUD buy (1-4%) × expensive CRIME sell (1-4%) = 4.1-10.1%
  BEST CASE for Route 6!
```

**The tax regime doesn't change cross-denomination friction for same-token routes, but it dramatically affects cross-token and combined routes.** The "pressure and release" cycle applies to Routes 3-6, not Routes 1-2.

### 4.6 SOL Dumps 15% — Four-Pool, Mean-Reversion Effect

**Setup (same pools, SOL: $150 → $127.50)**

```
After dump:
  SOL pools: 350 SOL (now worth $44,625) + same CRIME/FRAUD
  USDC pools: $15K USDC + same CRIME/FRAUD (unchanged)

Implied CRIME prices:
  SOL pool: 0.00000233 SOL × $127.50 = $0.000298/CRIME
  USDC pool: $15,000 / 43M = $0.000349/CRIME

Gap: CRIME is now CHEAPER in SOL pool than USDC pool (reversed from pump scenario)
```

**Arb direction reverses:**
```
1. Buy CRIME from SOL pool (cheap at $0.000298)
2. Sell CRIME into USDC pool (expensive at $0.000349)
3. Swap USDC → SOL on Jupiter
```

**Net effect on pools:**
```
SOL pool gets DEEPER in SOL (SOL enters at cheap prices)
USDC pool gets SHALLOWER in USDC (USDC exits)

= Automatic dip-buying by the protocol
```

**If SOL then bounces back up:**
The protocol now holds MORE SOL (bought at the bottom by arb) and LESS USDC. The extra SOL exposure amplifies the recovery. This is the emergent mean-reversion engine from the futarchy spec.

### 4.7 How Futarchy Allocation Changes the Dynamics

**Setup comparison: 50/50 vs 80/20 SOL allocation, SOL pumps 20%**

```
50/50 allocation ($100K SOL / $100K USDC):
  SOL pool: 667 SOL + larger CRIME reserves
  USDC pool: $50K + larger CRIME reserves

  SOL appreciates: 667 SOL now worth $120K
  USDC unchanged: $50K
  Total: $170K (was $200K... wait, $200K total, SOL half was $100K → $120K)
  Total: $170K → no, $120K + $50K = $170K...

  Actually: started at $200K total ($100K SOL + $100K USDC)
  After 20% SOL pump: $120K SOL + $100K USDC = $220K
  USD gain from SOL side: $20K

  Cross-denom gap: 20% (proportional to SOL move)
  Arb opportunity SIZE: proportional to SOL pool depth ($100K base)

80/20 allocation ($160K SOL / $40K USDC):
  After 20% SOL pump: $192K SOL + $40K USDC = $232K
  USD gain from SOL side: $32K (60% more than 50/50!)

  Cross-denom gap: same 20%
  Arb opportunity SIZE: proportional to SOL pool depth ($160K base = 60% bigger)
```

**If the futarchy correctly predicted the pump (80/20 SOL):**
- Protocol captured $32K in SOL appreciation (vs $20K at 50/50)
- The thin USDC pool ($40K) limits arb extraction — small trades move its price quickly, slamming the arb window shut after minimal extraction
- The protocol KEEPS most of the SOL upside because arbers can't extract efficiently through the thin pool
- Result: **GROWTH** — pool depth increases in USD terms

**If the futarchy incorrectly predicted (20/80 SOL when SOL pumped):**
- Protocol only captured $8K in SOL appreciation (thin SOL pool)
- The thick USDC pool ($160K) enables large arb trades before price impact closes the window
- Arbers extract more value through the deep USDC pool
- BUT: every arb trade pays taxes (4-10%+ friction) distributed 71/24/5
- Result: **YIELD + DEFLATION** — pool depth decreases but tax revenue flows to stakers and Carnage

### 4.8 Pool Depth as Dynamic IL Shield

**This is the key insight connecting futarchy to IL:**

The thin pool acts as a natural arb throttle. Because constant-product AMMs have higher price impact on shallower pools, a small arb trade in the thin pool moves the price significantly, closing the arb window before much value can be extracted.

```
Thick pool ($160K USDC) + 20% SOL pump:
  Arber buys $10K CRIME from USDC pool
  Pool moves: $10K / $160K = 6.25% of pool depth
  Price impact: moderate — window stays open for more arb
  Multiple arb trades can execute before window closes

Thin pool ($40K USDC) + 20% SOL pump:
  Arber buys $10K CRIME from USDC pool
  Pool moves: $10K / $40K = 25% of pool depth
  Price impact: MASSIVE — window slams shut after ONE trade
  Arb is self-limiting
```

**Correct futarchy prediction = thin pool on the extraction side = self-limiting arb = protocol keeps upside.**

**Wrong futarchy prediction = thick pool on the extraction side = deep arb surface = more IL BUT more tax revenue.**

### 4.9 The No-Lose Flywheel

There is no state where the protocol loses net value to the ecosystem. Every outcome is positive, just in different forms:

```
FUTARCHY CORRECT:
  → Pool depth grows in USD terms (appreciation or dump protection)
  → Tokens get more expensive (deeper quote reserves = higher implied price)
  → Thin pool restricts arb extraction → protocol keeps the gains
  → Arb that DOES fire is small and pays taxes
  → PROFIT stakers benefit: underlying pool value increases
  → Result: GROWTH

FUTARCHY WRONG:
  → Thick pool enables heavy arb volume
  → IL occurs: pool depth decreases on quote side
  → BUT arbers pay 4-10%+ taxes on every trade
  → Tax distribution: 71% → PROFIT stakers, 24% → Carnage, 5% → Treasury
  → The "lost" pool depth is converted into direct SOL yield for stakers
  → Carnage uses its 24% to buy-and-burn tokens → deflationary pressure
  → PROFIT stakers benefit: direct SOL yield increases
  → Result: YIELD + DEFLATION (instead of growth)

WITH IN-HOUSE ARB BOTS:
  → Protocol executes the arb itself
  → Arber's profit goes to protocol (not external party)
  → 100% of extracted value flows into 71/24/5 split
  → IL becomes purely internal rebalancing — zero value leaves the ecosystem
  → Wrong predictions become PURE REVENUE EVENTS
  → Result: YIELD + DEFLATION with ZERO external leakage
```

**The futarchy accuracy determines the MIX of benefits, not WHETHER there are benefits:**

| Accuracy | Growth | Yield | Deflation | Net |
|----------|--------|-------|-----------|-----|
| 80% (great) | High | Moderate | Moderate | Strongly positive |
| 65% (baseline) | Moderate | Moderate | Moderate | Positive |
| 50% (coin flip) | Low | High | High | Positive (yield-heavy) |
| 30% (terrible) | Very low | Very high | Very high | Positive (yield-heavy) |
| 0% (always wrong) | Zero | Maximum | Maximum | Still positive (pure yield) |

Even at 0% accuracy (impossible in practice — coin flip is the floor), the protocol converts ALL IL into staker yield and Carnage burns. The pools shrink but token prices rise from deflation and stakers earn maximum SOL yield.

**The flywheel compounds:**
```
1. Correct prediction → pools grow → tokens more expensive
                                          ↓
2. More expensive tokens → more volume (bigger trades to move price)
                                          ↓
3. More volume → more tax revenue → more staker yield + Carnage burns
                                          ↓
4. Wrong prediction → arb fires → tax revenue from arb
                                          ↓
5. Carnage burns → supply decreases → tokens more expensive (even at same depth)
                                          ↓
6. Return to step 1 with deeper pools OR higher prices OR both
```

Every path loops back to either deeper pools, higher token prices, more staker yield, or all three simultaneously. The system has no terminal state — it compounds indefinitely as long as any trading volume exists.

**This is the economic link: the futarchy doesn't just steer capital allocation — it dynamically adjusts the protocol's IL exposure surface.** Correct predictions shrink the arb window (thin pool on extraction side). Wrong predictions widen it (thick pool on extraction side). But widened arb windows generate tax revenue that flows to stakers and Carnage. The protocol converts its own mistakes into yield.

---

## 5. IL Analysis: When Does the Protocol Actually Lose?

### 5.1 Reframing IL for Protocol-Owned Liquidity

Traditional IL: "LPs lose money compared to just holding the assets."

Dr. Fraudsworth IL: "The protocol's pools lose value compared to just holding SOL + tokens."

But this comparison is misleading because:
1. The protocol can't "just hold" — the pools ARE the product
2. Tax revenue from every trade (including arb) flows back to the protocol ecosystem
3. LP fees (1%) compound permanently into reserves
4. There is no external market to diverge against (transfer hooks)

### 5.2 IL in Dr. Fraudsworth: The Revenue-First Model

Traditional IL framing: "LPs lose money vs holding."
Dr. Fraudsworth framing: **"IL only occurs as a consequence of revenue-generating organic volume."**

This is the critical distinction. In Uniswap, IL happens because an external market moves and arbers correct the pool price — the LP had no say and received no revenue from the event that caused the IL. In Dr. Fraudsworth:

1. **Organic trading volume pushes prices apart** → protocol collects tax revenue (71/24/5 split) on every trade
2. **Tax flip creates arb window** → arber closes the spread
3. **Arber pays taxes on both legs of the arb** → protocol collects more tax revenue
4. **LP fees compound on both legs** → pools get deeper

**IL is the "cost" of having generated revenue.** It's not a loss from passivity — it's a controlled leak that only exists because the protocol was actively earning.

### 5.3 The Full Revenue vs IL Accounting

For a complete cycle (organic volume → flip → arb), the protocol's total accounting is:

```
REVENUE SIDE (collected before and during arb):
  + Tax revenue from organic buy volume (Epoch N)
  + Tax revenue from organic sell volume (Epoch N)
  + Tax revenue from arb buy leg (Epoch N+1)
  + Tax revenue from arb sell leg (Epoch N+1)
  + LP fees from organic volume (permanently in pools)
  + LP fees from arb volume (permanently in pools)

COST SIDE (value leaving pools via arb):
  - Arber's net profit (spread captured minus friction paid)

NET = All revenue - Arber's profit
```

**The arber's profit is always LESS than the spread.** The spread was created by organic volume that the protocol taxed. So the protocol is comparing:

```
Tax revenue from creating the spread + Tax revenue from closing the spread
vs
The portion of the spread the arber captures after paying taxes
```

Since the arber pays 4-10% friction on a spread that might be 8-15%, and the protocol collected taxes on all the organic volume that created that 8-15% spread in the first place, the protocol is very likely net positive across a full cycle.

### 5.4 When the Protocol Loses: Edge Cases

The protocol can be net negative on the IL accounting in specific scenarios:

**1. Whale single-trade divergence:** A single large organic trade pushes prices far apart in one transaction, paying one tax event. The subsequent arb across multiple smaller trades generates less total tax than the IL created. Mitigation: the 50% output floor limits how far a single trade can move the pool.

**2. Carnage-induced divergence:** Carnage buys are tax-exempt (swap_exempt). If Carnage buys CRIME from one pool, it pushes the CRIME price up without paying tax. This creates a "free" divergence that arbers can close while only paying post-flip friction. The protocol funded the Carnage from accumulated tax revenue (24% of all taxes), so this is effectively the protocol spending its own savings to create arb opportunities. Whether this is "IL" or "intended deflationary mechanics" is a framing question.

**3. Consecutive no-flip epochs with high organic volume:** If organic volume pushes prices far apart over multiple epochs (no flip for 3-4 consecutive epochs = ~0.4% chance), the accumulated spread when the flip finally occurs could be large. The arb profit on this large spread might exceed the cumulative tax revenue from the organic volume. This is the highest-risk IL scenario but it's statistically rare.

### 5.5 Net IL Over Time: The Critical Simulation Question

The question that needs simulation:

**Over N epochs with random VRF flips and realistic trading volume, does the protocol's TOTAL revenue (tax from organic + tax from arb + LP fees) exceed the TOTAL value extracted by arbers?**

This must account for the full cycle, not just individual arb events:

```
Per-cycle accounting:
  Revenue from organic volume that created the spread:
    = organic_volume × average_tax_rate × number_of_organic_trades

  Revenue from arb volume that closed the spread:
    = arb_volume × post_flip_tax_rate (both legs)

  LP fee revenue:
    = (organic_volume + arb_volume) × 1% (retained permanently)

  IL (value extracted):
    = arber_net_profit = spread × arb_volume - friction × arb_volume

  Net protocol position:
    = organic_tax_revenue + arb_tax_revenue + lp_fees - arber_net_profit
```

Variables:
- Organic volume per epoch (drives spread creation rate)
- VRF flip sequence (determines when arb windows open)
- Tax magnitude rolls (1-4% / 11-14% affects both organic revenue and arb friction)
- Carnage events (create tax-exempt divergence)
- Pool depths (deeper pools = less slippage per trade = smaller spreads per unit volume)
- SOL/USD volatility (irrelevant for two-pool cross-token arb, critical for four-pool cross-denomination)
- Futarchy accuracy (four-pool only, affects pool depth allocation)

---

## 6. Simulation Design Recommendations

### 6.1 Core Simulation Parameters

```
TIME:
  Epochs per simulation: 1,000-10,000 (30 min each = 21-208 days)
  SOL/USD price model: Geometric Brownian Motion with configurable vol

POOLS:
  Initial SOL reserves per pool: configurable (e.g., 500 SOL)
  Initial token reserves: derived from initial price
  Four-pool mode: configurable allocation ratio

TAXES:
  VRF simulation: random bytes, 75% flip, independent per token
  Tax magnitudes: uniform random from {1%, 2%, 3%, 4%} / {11%, 12%, 13%, 14%}

ARBERS:
  Strategy: rational (execute whenever profit > 0 after all costs)
  Latency: 0 (instant execution within epoch — conservative assumption)
  Capital: unlimited (can always execute profitable arbs)
  Route selection: optimal (always picks highest-profit route)

CARNAGE:
  Trigger: 4.3% per epoch
  Target: 50/50 CRIME/FRAUD
  Action: BuyOnly 100% / Burn 98% / Sell 2% (when holdings exist)
  Fund: accumulates from 24% of all taxes

FUTARCHY (four-pool only):
  Weekly allocation: 50-90% SOL (market-clearing from simulated predictions)
  Accuracy: configurable (50%, 55%, 65%, 75%, 80%)
  Rebalancing: delta-only, once per week
```

### 6.2 Metrics to Track

```
PER-EPOCH:
  - SOL/USD price
  - Tax regime state (CC/CE/EC/EE)
  - Each pool's reserves (before and after arb)
  - Arb routes executed (which, direction, size, profit)
  - Tax revenue generated (staking/carnage/treasury split)
  - LP fee revenue (per pool)
  - IL incurred (per pool, per arb event)
  - Carnage events (trigger, action, target, burn amount)
  - Net protocol P&L: (tax revenue + LP fees) - IL

PER-WEEK (four-pool):
  - Futarchy allocation decision
  - Actual SOL/USD direction
  - Correct/incorrect prediction
  - Rebalancing delta (size and slippage)
  - Cross-denomination arb volume
  - Pool depth change (USD terms)

CUMULATIVE:
  - Total protocol revenue (taxes + LP fees + prediction market fees)
  - Total IL incurred
  - Net protocol value change (revenue - IL, in USD terms)
  - CRIME/FRAUD supply (after Carnage burns)
  - Effective PROFIT supply (vault lockup from burns)
  - Pool depth trajectory (USD terms, per pool)
  - Revenue per unit of IL (efficiency ratio)
```

### 6.3 Key Scenarios to Simulate

```
1. BASELINE: SOL flat (±2%), moderate organic volume, two-pool
   → Establishes tax revenue baseline with minimal cross-denom arb

2. BULL RUN: SOL +5% per week for 20 weeks, two-pool
   → Measures IL from sustained directional movement

3. HIGH VOLATILITY: SOL ±10% weekly swings, two-pool
   → Stress-tests arb frequency and IL/revenue balance

4. FOUR-POOL FLAT: SOL flat, four-pool, 50/50 allocation
   → Baseline for four-pool without cross-denom arb

5. FOUR-POOL BULL + GOOD FUTARCHY: SOL trending up, 65% accuracy
   → Shows futarchy benefit over static allocation

6. FOUR-POOL BULL + BAD FUTARCHY: SOL trending up, 50% accuracy
   → Shows futarchy adds no harm (neutral at coin-flip accuracy)

7. FOUR-POOL CRASH + GOOD FUTARCHY: SOL -40% over 4 weeks, 75% accuracy
   → Shows bear market protection from correct USDC allocation

8. FOUR-POOL CRASH + BAD FUTARCHY: SOL -40%, 50% accuracy
   → Worst case: bad prediction + bear market

9. CARNAGE CASCADE: High volume → large Carnage fund → frequent burns
   → Measures Carnage amplification across four pools

10. REGIME FLIP STORM: Simulate 100 consecutive CE→EC flips
    → Tests pressure/release dynamics under extreme flip frequency
```

### 6.4 Hypothesis to Test

```
H1: Over 1000+ epochs, total protocol revenue (taxes + LP fees) exceeds
    total IL from all arb routes, at all realistic volatility levels.

H2: Four-pool architecture with 65% futarchy accuracy produces higher
    USD-denominated pool growth than two-pool architecture.

H3: Cross-denomination arb fires less than once per week at average
    SOL volatility (~5% weekly), but fires multiple times per week
    during high-vol periods (~15%+ weekly).

H4: The CE/EC tax states generate >3× the cross-token arb volume of
    CC/EE states, and the tax revenue from that increased volume more
    than compensates for the lower friction.

H5: Carnage events in four-pool create measurably more tax revenue via
    cascade arb than in two-pool (amplification factor).

H6: The emergent mean-reversion effect (automatic profit-taking on pumps,
    dip-buying on dumps) produces measurable USD-denominated pool value
    preservation vs a passive hold strategy.

H7: Net IL is negative (protocol GAINS from arb) in CC/EE states and
    positive (protocol loses to arb) in CE/EC states, but the time-weighted
    average across all states is net positive for the protocol.

H8: The protocol is net positive across ALL futarchy accuracy levels
    (50%-80%), with the mix shifting from growth-dominant (high accuracy)
    to yield-dominant (low accuracy) but never going negative.

H9: Pool depth ratio acts as an arb throttle — thin pools self-limit arb
    extraction via price impact. Correct futarchy allocation places the
    thin pool on the extraction side, measurably reducing IL per SOL move.

H10: With in-house arb bots capturing 100% of arb profit (no external
     leakage), the protocol's net position is positive under ALL conditions
     including worst-case futarchy accuracy and high SOL volatility.

H11: The "no-lose flywheel" compounds — each cycle produces either pool
     depth growth OR increased staker yield + Carnage burns, and the
     output of each cycle feeds into the next, producing geometric growth
     in total protocol value (pools + cumulative distributions).
```

---

## 7. Open Questions for Simulation Refinement

1. **Organic volume model:** What fraction of total volume is organic (user-driven) vs arb-driven? This affects the tax revenue baseline.

2. **Arber competition:** With multiple arbers competing, do they race to close gaps faster (reducing individual profits but generating same tax revenue)?

3. **Gas costs:** On Solana, gas is cheap but non-zero. At what trade size does gas become meaningful relative to arb profit?

4. **Pool depth sensitivity:** How does IL/revenue ratio change as pools grow from $15K to $150K to $1.5M? Deeper pools = less slippage per arb = less IL per event but potentially more arb volume.

5. **Carnage timing interaction:** Carnage events create price shocks. If Carnage fires in a CE state (low cross-token friction), does it trigger immediate arb that partially undoes the Carnage effect?

6. **Vault conversion volume:** Does heavy vault usage (CRIME → PROFIT → FRAUD) create measurable round-trip friction beyond the theoretical 0%? (Dust losses of `N % 100` lamports per conversion.)

7. **Futarchy rebalancing slippage:** The weekly USDC→SOL or SOL→USDC swap on Jupiter has its own slippage cost. At what protocol size does this become material?

8. **Tax magnitude variance:** Does it matter whether taxes are 1% or 4% on the low end, or 11% vs 14% on the high end? Or is the binary cheap/expensive distinction sufficient for simulation?

---

*Document created 2026-03-29. Status: analysis framework. All worked examples use illustrative numbers and require validation via simulation.*
