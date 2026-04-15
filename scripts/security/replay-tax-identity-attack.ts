/**
 * Tax/Pool Identity Mismatch — On-Chain Exploit Replay (Phase 122.1-02 Task 4)
 *
 * Submits the 4 adversarial swap_sol_* variants against the patched devnet Tax
 * Program and asserts that each transaction reverts with the expected error.
 *
 * This is EXPLOIT-ONLY. Honest swap verification is handled out-of-band by a
 * human operator driving the live devnet frontend (see 122.1-02 continuation
 * prompt). Do NOT add an honest case here.
 *
 * Exploit cases (all must revert):
 *   1. swap_sol_buy  — CRIME/SOL pool, mint_b = CRIME, is_crime = false
 *        -> expect TaxIdentityMismatch (6019 / 0x1783) from Tax Program
 *   2. swap_sol_sell — FRAUD/SOL pool, mint_b = FRAUD, is_crime = true
 *        -> expect TaxIdentityMismatch (6019 / 0x1783) from Tax Program
 *   3. swap_sol_buy  — CRIME/SOL pool, mint_b = FRAUD (wrong mint for pool),
 *                      is_crime = false (self-consistent with the mint passed)
 *        -> expect PoolMintMismatch (6020 / 0x1784) from Tax Program, OR a
 *           downstream AMM mint-mismatch error if the AMM's Anchor constraints
 *           fire before the Tax Program's check. Either outcome proves the
 *           attack is blocked (defense-in-depth working as designed per
 *           122.1-01 precedent).
 *   4. swap_sol_sell — CRIME/SOL pool, mint_b = random unknown mint
 *        -> expect UnknownTaxedMint (6021 / 0x1785) from Tax Program, OR a
 *           downstream AMM/Anchor error if it fires first.
 *
 * Error code reference (Anchor custom errors start at 6000 = 0x1770):
 *   6019 = 0x1783 — TaxIdentityMismatch
 *   6020 = 0x1784 — PoolMintMismatch
 *   6021 = 0x1785 — UnknownTaxedMint
 *
 * Idempotency: Each exploit attempts a tiny swap (0.001 SOL). If the exploit
 * were to succeed (which would be a CRITICAL fix-broken signal), it would
 * incur trivial tax. All exploits build instructions directly via the Tax
 * Program IDL — no frontend, no aggregators, no SDK sanitisation in the way.
 *
 * Run:
 *   source "$HOME/.cargo/env"
 *   export PATH="$HOME/.local/share/solana/install/active_release/bin:$PATH"
 *   export PATH="/opt/homebrew/bin:$PATH"
 *   CLUSTER_URL="https://api.devnet.solana.com" \
 *     WALLET="keypairs/devnet-wallet.json" \
 *     npx tsx scripts/security/replay-tax-identity-attack.ts
 */

import * as anchor from "@coral-xyz/anchor";
import { AnchorProvider } from "@coral-xyz/anchor";
import {
  Keypair,
  PublicKey,
  SystemProgram,
  Transaction,
  ComputeBudgetProgram,
  LAMPORTS_PER_SOL,
} from "@solana/web3.js";
import {
  TOKEN_2022_PROGRAM_ID,
  TOKEN_PROGRAM_ID,
  NATIVE_MINT,
} from "@solana/spl-token";

import * as fs from "fs";
import * as path from "path";

import { loadProvider } from "../deploy/lib/connection";
import { createE2EUser, E2EUser } from "../e2e/lib/user-setup";

// Cluster-aware Program loader — overrides the IDL's baked-in `address`
// field with the devnet Tax Program ID before constructing the Program.
// This is necessary because:
//   1. target/types/tax_program.ts / target/idl/tax_program.json were
//      generated with declare_id = mainnet value (43fZ…) after Task 3.
//   2. Running against devnet requires the program to be addressed at the
//      devnet Tax Program pid (FGgi…).
//   3. Anchor PDA constraints like `seeds = [...]` are derived from the
//      Program's programId — if it's wrong, every account constraint fails
//      with ConstraintSeeds (code 2006).
function loadTaxProgramForCluster(
  provider: AnchorProvider,
  devnetTaxPid: PublicKey
): anchor.Program {
  const idlPath = path.resolve(process.cwd(), "target/idl/tax_program.json");
  const idl = JSON.parse(fs.readFileSync(idlPath, "utf-8"));
  idl.address = devnetTaxPid.toBase58();
  // Anchor 0.32 constructor signature: new Program(idl, provider)
  // It reads address from idl.address when no explicit programId is given.
  return new anchor.Program(idl as any, provider);
}
import { resolveHookAccounts } from "../e2e/lib/swap-flow";
import { loadDeployment } from "../e2e/lib/load-deployment";
import { PDAManifest } from "../e2e/devnet-e2e-validation";

// Load the raw devnet.json once to pull the treasury pubkey (not surfaced via
// PDAManifest). Matches the devnet treasury pinned in tax-program constants.rs
// under #[cfg(feature = "devnet")] — must match or every honest swap reverts
// with InvalidTreasury. We don't read it inside the handler; only using it to
// populate the `treasury` account in the exploit transactions.
function loadDevnetTreasury(): PublicKey {
  const p = path.resolve(process.cwd(), "deployments/devnet.json");
  const d = JSON.parse(fs.readFileSync(p, "utf-8"));
  if (!d.treasury) {
    throw new Error("deployments/devnet.json is missing `treasury` field");
  }
  return new PublicKey(d.treasury);
}

// ---------------------------------------------------------------------------
// Error-code matcher
// ---------------------------------------------------------------------------
//
// Anchor custom errors are surfaced in logs as:
//   "custom program error: 0x1783"           (hex form)
//   "Error Number: 6019"                     (decimal, from Anchor's simulate)
//   "Error Code: TaxIdentityMismatch"        (IDL-named, only if IDL is fresh)
//
// The devnet IDL may be stale (regenerated only on `anchor build`, not
// `cargo build-sbf`), so the named form is unreliable. We match on all three
// forms and call it a pass if ANY is present.

const TAX_ERROR_HEX = {
  TaxIdentityMismatch: "0x1783",
  PoolMintMismatch: "0x1784",
  UnknownTaxedMint: "0x1785",
} as const;

const TAX_ERROR_DEC = {
  TaxIdentityMismatch: 6019,
  PoolMintMismatch: 6020,
  UnknownTaxedMint: 6021,
} as const;

type TaxErrorName = keyof typeof TAX_ERROR_HEX;

const VERBOSE = process.env.VERBOSE === "1";

/**
 * Extract every piece of diagnostic information from a thrown error.
 *
 * Solana's SendTransactionError swallows the meaningful program logs unless
 * you explicitly read .logs / .getLogs() — the default toString() just says
 * "Simulation failed." which is useless for matching error codes.
 *
 * We aggressively flatten .logs, .message, .stack, and any nested fields
 * into a single string the matcher can regex over.
 */
function errToFullString(err: unknown): string {
  const parts: string[] = [];
  parts.push(String(err));
  if (err && typeof err === "object") {
    const anyErr = err as any;
    if (anyErr.message) parts.push(`message: ${anyErr.message}`);
    if (Array.isArray(anyErr.logs)) {
      parts.push("logs:\n" + anyErr.logs.join("\n"));
    }
    // SendTransactionError.getLogs() (may require connection)
    if (typeof anyErr.getLogs === "function") {
      try {
        const maybe = anyErr.getLogs();
        if (Array.isArray(maybe)) parts.push("getLogs:\n" + maybe.join("\n"));
      } catch {
        /* ignore */
      }
    }
    if (anyErr.signature) parts.push(`signature: ${anyErr.signature}`);
    // Anchor AnchorError
    if (anyErr.error?.errorCode) {
      parts.push(
        `anchorErrorCode: ${anyErr.error.errorCode.code}/${anyErr.error.errorCode.number}`
      );
    }
    if (anyErr.error?.errorMessage) {
      parts.push(`anchorErrorMessage: ${anyErr.error.errorMessage}`);
    }
    if (anyErr.transactionMessage) {
      parts.push(`txMessage: ${anyErr.transactionMessage}`);
    }
  }
  return parts.join("\n");
}

function matchesTaxError(errStr: string, name: TaxErrorName): boolean {
  return (
    errStr.includes(TAX_ERROR_HEX[name]) ||
    errStr.includes(String(TAX_ERROR_DEC[name])) ||
    errStr.includes(name)
  );
}

// AMM / Anchor account-constraint errors that legitimately catch some
// exploit variants before the Tax Program's own checks fire. Treated as
// "defense-in-depth working" per 122.1-01 precedent — still a PASS.
function matchesDownstreamRejection(errStr: string): boolean {
  return (
    errStr.includes("ConstraintAddress") ||
    errStr.includes("ConstraintSeeds") ||
    errStr.includes("ConstraintTokenMint") ||
    errStr.includes("ConstraintRaw") ||
    errStr.includes("InvalidMint") ||
    errStr.includes("AccountOwnedByWrongProgram") ||
    errStr.includes("AccountNotInitialized") ||
    // Anchor account constraint codes: 2000-2999 range
    /0x7d[0-9a-f]/i.test(errStr) ||
    /\b(200[0-9]|201[0-9]|202[0-9])\b/.test(errStr)
  );
}

// ---------------------------------------------------------------------------
// Result tracking
// ---------------------------------------------------------------------------

interface ExploitResult {
  caseNum: number;
  name: string;
  expectedError: string;
  actualError: string;
  actualCode: string | null;
  txSignature: string | null;
  solscanUrl: string | null;
  verdict: "REJECTED_TAX" | "REJECTED_DOWNSTREAM" | "UNEXPECTED_SUCCESS" | "UNEXPECTED_ERROR";
}

const results: ExploitResult[] = [];

function solscanTx(sig: string): string {
  return `https://solscan.io/tx/${sig}?cluster=devnet`;
}

function extractFirstLine(errStr: string, maxLen = 200): string {
  const firstLine = errStr.split("\n")[0].trim();
  return firstLine.length > maxLen
    ? firstLine.slice(0, maxLen) + "…"
    : firstLine;
}

function extractErrorCode(errStr: string): string | null {
  // Prefer hex code, fall back to decimal "Error Number: NNNN"
  const hexMatch = errStr.match(/0x1[78][0-9a-f][0-9a-f]/i);
  if (hexMatch) return hexMatch[0];
  const decMatch = errStr.match(/Error Number:\s*(\d+)/);
  if (decMatch) return `${decMatch[1]}`;
  const customMatch = errStr.match(/custom program error:\s*(0x[0-9a-f]+)/i);
  if (customMatch) return customMatch[1];
  return null;
}

// ---------------------------------------------------------------------------
// Account builder helpers
// ---------------------------------------------------------------------------

interface SwapContext {
  provider: AnchorProvider;
  taxProgram: anchor.Program;
  manifest: PDAManifest;
  user: E2EUser;
  treasury: PublicKey;
}

/**
 * Build the accounts struct for swap_sol_buy against the CRIME/SOL pool.
 * Overrides allow each exploit case to substitute specific fields (mintB,
 * userTokenB, hookAccounts) to construct the adversarial variant.
 */
function buildCrimeBuyAccounts(ctx: SwapContext, overrides: {
  mintB?: PublicKey;
  userTokenB?: PublicKey;
}) {
  const { manifest, user, taxProgram } = ctx;
  const pool = manifest.pools["CRIME/SOL"];
  const crimeMint = new PublicKey(manifest.mints.CRIME);

  const [swapAuthorityPda] = PublicKey.findProgramAddressSync(
    [Buffer.from("swap_authority")],
    taxProgram.programId
  );

  return {
    user: user.keypair.publicKey,
    epochState: new PublicKey(manifest.pdas.EpochState),
    swapAuthority: swapAuthorityPda,
    taxAuthority: new PublicKey(manifest.pdas.TaxAuthority),
    pool: new PublicKey(pool.pool),
    poolVaultA: new PublicKey(pool.vaultA),
    poolVaultB: new PublicKey(pool.vaultB),
    mintA: NATIVE_MINT,
    mintB: overrides.mintB ?? crimeMint,
    userTokenA: user.wsolAccount,
    userTokenB: overrides.userTokenB ?? user.crimeAccount,
    stakePool: new PublicKey(manifest.pdas.StakePool),
    stakingEscrow: new PublicKey(manifest.pdas.EscrowVault),
    carnageVault: new PublicKey(manifest.pdas.CarnageSolVault),
    treasury: ctx.treasury,
    ammProgram: new PublicKey(manifest.programs.AMM),
    tokenProgramA: TOKEN_PROGRAM_ID,
    tokenProgramB: TOKEN_2022_PROGRAM_ID,
    systemProgram: SystemProgram.programId,
    stakingProgram: new PublicKey(manifest.programs.Staking),
  };
}

function buildFraudSellAccounts(ctx: SwapContext, overrides: {
  mintB?: PublicKey;
}) {
  const { manifest, user, taxProgram } = ctx;
  const pool = manifest.pools["FRAUD/SOL"];
  const fraudMint = new PublicKey(manifest.mints.FRAUD);

  const [swapAuthorityPda] = PublicKey.findProgramAddressSync(
    [Buffer.from("swap_authority")],
    taxProgram.programId
  );

  return {
    user: user.keypair.publicKey,
    epochState: new PublicKey(manifest.pdas.EpochState),
    swapAuthority: swapAuthorityPda,
    taxAuthority: new PublicKey(manifest.pdas.TaxAuthority),
    pool: new PublicKey(pool.pool),
    poolVaultA: new PublicKey(pool.vaultA),
    poolVaultB: new PublicKey(pool.vaultB),
    mintA: NATIVE_MINT,
    mintB: overrides.mintB ?? fraudMint,
    userTokenA: user.wsolAccount,
    userTokenB: user.fraudAccount,
    stakePool: new PublicKey(manifest.pdas.StakePool),
    stakingEscrow: new PublicKey(manifest.pdas.EscrowVault),
    carnageVault: new PublicKey(manifest.pdas.CarnageSolVault),
    treasury: ctx.treasury,
    wsolIntermediary: new PublicKey(manifest.pdas.WsolIntermediary),
    ammProgram: new PublicKey(manifest.programs.AMM),
    tokenProgramA: TOKEN_PROGRAM_ID,
    tokenProgramB: TOKEN_2022_PROGRAM_ID,
    systemProgram: SystemProgram.programId,
    stakingProgram: new PublicKey(manifest.programs.Staking),
  };
}

// ---------------------------------------------------------------------------
// Exploit cases
// ---------------------------------------------------------------------------

async function runCase1CrimeBuyWrongFlag(ctx: SwapContext): Promise<void> {
  const caseNum = 1;
  const name = "swap_sol_buy CRIME pool + mint_b=CRIME + is_crime=false";
  const expected = "TaxIdentityMismatch (6019 / 0x1783)";
  console.log(`\n[Case ${caseNum}] ${name}`);
  console.log(`  Expecting: ${expected}`);

  try {
    const accounts = buildCrimeBuyAccounts(ctx, {});
    const hook = await resolveHookAccounts(
      ctx.provider.connection,
      accounts.poolVaultB,
      accounts.mintB,
      accounts.userTokenB,
      accounts.swapAuthority,
      BigInt(0)
    );

    const swapIx = await ctx.taxProgram.methods
      .swapSolBuy(
        new anchor.BN(1_000_000), // 0.001 SOL
        new anchor.BN(1),
        false // WRONG: pool is CRIME, caller lies that it's FRAUD
      )
      .accountsStrict(accounts)
      .remainingAccounts(hook)
      .instruction();

    const tx = new Transaction().add(
      ComputeBudgetProgram.setComputeUnitLimit({ units: 400_000 }),
      swapIx
    );

    const sig = await ctx.provider.sendAndConfirm(tx, [ctx.user.keypair]);
    // If we reach here the exploit SUCCEEDED — this is a CRITICAL failure.
    results.push({
      caseNum, name, expectedError: expected,
      actualError: "UNEXPECTED_SUCCESS — exploit reached chain without revert",
      actualCode: null,
      txSignature: sig,
      solscanUrl: solscanTx(sig),
      verdict: "UNEXPECTED_SUCCESS",
    });
    console.error(`  CRITICAL: exploit succeeded! TX ${sig}`);
  } catch (err) {
    const errStr = errToFullString(err);
    if (VERBOSE) {
      console.log("  --- full error dump ---");
      console.log(errStr);
      console.log("  --- end dump ---");
    }
    const code = extractErrorCode(errStr);
    if (matchesTaxError(errStr, "TaxIdentityMismatch")) {
      results.push({
        caseNum, name, expectedError: expected,
        actualError: extractFirstLine(errStr),
        actualCode: code,
        txSignature: (err as any)?.signature ?? null,
        solscanUrl: (err as any)?.signature ? solscanTx((err as any).signature) : null,
        verdict: "REJECTED_TAX",
      });
      console.log(`  PASS — rejected with TaxIdentityMismatch (${code})`);
    } else if (matchesDownstreamRejection(errStr)) {
      results.push({
        caseNum, name, expectedError: expected,
        actualError: extractFirstLine(errStr),
        actualCode: code,
        txSignature: (err as any)?.signature ?? null,
        solscanUrl: (err as any)?.signature ? solscanTx((err as any).signature) : null,
        verdict: "REJECTED_DOWNSTREAM",
      });
      console.log(`  PASS — rejected downstream (${code}): ${extractFirstLine(errStr)}`);
    } else {
      results.push({
        caseNum, name, expectedError: expected,
        actualError: extractFirstLine(errStr, 300),
        actualCode: code,
        txSignature: null,
        solscanUrl: null,
        verdict: "UNEXPECTED_ERROR",
      });
      console.warn(`  UNEXPECTED ERROR (${code}): ${extractFirstLine(errStr, 300)}`);
    }
  }
}

async function runCase2FraudSellWrongFlag(ctx: SwapContext): Promise<void> {
  const caseNum = 2;
  const name = "swap_sol_sell FRAUD pool + mint_b=FRAUD + is_crime=true";
  const expected = "TaxIdentityMismatch (6019 / 0x1783)";
  console.log(`\n[Case ${caseNum}] ${name}`);
  console.log(`  Expecting: ${expected}`);

  try {
    const accounts = buildFraudSellAccounts(ctx, {});
    // Sell needs tokens to sell; without any FRAUD in the user ATA, the AMM
    // will short-circuit on insufficient balance. But our check fires BEFORE
    // the AMM CPI, so a zero-balance account is still a valid exploit probe.
    const hook = await resolveHookAccounts(
      ctx.provider.connection,
      accounts.poolVaultB,
      accounts.mintB,
      ctx.user.fraudAccount,
      accounts.swapAuthority,
      BigInt(0)
    );

    const swapIx = await ctx.taxProgram.methods
      .swapSolSell(
        new anchor.BN(1_000), // 1000 base units (tiny)
        new anchor.BN(1),
        true // WRONG: pool is FRAUD, caller lies that it's CRIME
      )
      .accountsStrict(accounts)
      .remainingAccounts(hook)
      .instruction();

    const tx = new Transaction().add(
      ComputeBudgetProgram.setComputeUnitLimit({ units: 400_000 }),
      swapIx
    );

    const sig = await ctx.provider.sendAndConfirm(tx, [ctx.user.keypair]);
    results.push({
      caseNum, name, expectedError: expected,
      actualError: "UNEXPECTED_SUCCESS",
      actualCode: null,
      txSignature: sig,
      solscanUrl: solscanTx(sig),
      verdict: "UNEXPECTED_SUCCESS",
    });
    console.error(`  CRITICAL: exploit succeeded! TX ${sig}`);
  } catch (err) {
    const errStr = errToFullString(err);
    if (VERBOSE) {
      console.log("  --- full error dump ---");
      console.log(errStr);
      console.log("  --- end dump ---");
    }
    const code = extractErrorCode(errStr);
    if (matchesTaxError(errStr, "TaxIdentityMismatch")) {
      results.push({
        caseNum, name, expectedError: expected,
        actualError: extractFirstLine(errStr),
        actualCode: code,
        txSignature: (err as any)?.signature ?? null,
        solscanUrl: (err as any)?.signature ? solscanTx((err as any).signature) : null,
        verdict: "REJECTED_TAX",
      });
      console.log(`  PASS — rejected with TaxIdentityMismatch (${code})`);
    } else if (matchesDownstreamRejection(errStr)) {
      results.push({
        caseNum, name, expectedError: expected,
        actualError: extractFirstLine(errStr),
        actualCode: code,
        txSignature: (err as any)?.signature ?? null,
        solscanUrl: (err as any)?.signature ? solscanTx((err as any).signature) : null,
        verdict: "REJECTED_DOWNSTREAM",
      });
      console.log(`  PASS — rejected downstream (${code}): ${extractFirstLine(errStr)}`);
    } else {
      results.push({
        caseNum, name, expectedError: expected,
        actualError: extractFirstLine(errStr, 300),
        actualCode: code,
        txSignature: null,
        solscanUrl: null,
        verdict: "UNEXPECTED_ERROR",
      });
      console.warn(`  UNEXPECTED ERROR (${code}): ${extractFirstLine(errStr, 300)}`);
    }
  }
}

async function runCase3CrimePoolFraudMint(ctx: SwapContext): Promise<void> {
  const caseNum = 3;
  const name = "swap_sol_buy CRIME pool + mint_b=FRAUD (wrong mint) + is_crime=false";
  const expected = "PoolMintMismatch (6020 / 0x1784) or downstream AMM rejection";
  console.log(`\n[Case ${caseNum}] ${name}`);
  console.log(`  Expecting: ${expected}`);

  try {
    const fraudMint = new PublicKey(ctx.manifest.mints.FRAUD);
    const accounts = buildCrimeBuyAccounts(ctx, {
      mintB: fraudMint,
      userTokenB: ctx.user.fraudAccount,
    });
    const hook = await resolveHookAccounts(
      ctx.provider.connection,
      accounts.poolVaultB, // CRIME pool vault (wrong mint for the hook we're resolving)
      fraudMint,
      ctx.user.fraudAccount,
      accounts.swapAuthority,
      BigInt(0)
    );

    const swapIx = await ctx.taxProgram.methods
      .swapSolBuy(
        new anchor.BN(1_000_000),
        new anchor.BN(1),
        false // self-consistent with mint_b=FRAUD (so Option B check passes)
              // forcing Option C (pool.mint_b vs mint_b account) to be the trip wire
      )
      .accountsStrict(accounts)
      .remainingAccounts(hook)
      .instruction();

    const tx = new Transaction().add(
      ComputeBudgetProgram.setComputeUnitLimit({ units: 400_000 }),
      swapIx
    );

    const sig = await ctx.provider.sendAndConfirm(tx, [ctx.user.keypair]);
    results.push({
      caseNum, name, expectedError: expected,
      actualError: "UNEXPECTED_SUCCESS",
      actualCode: null,
      txSignature: sig,
      solscanUrl: solscanTx(sig),
      verdict: "UNEXPECTED_SUCCESS",
    });
    console.error(`  CRITICAL: exploit succeeded! TX ${sig}`);
  } catch (err) {
    const errStr = errToFullString(err);
    if (VERBOSE) {
      console.log("  --- full error dump ---");
      console.log(errStr);
      console.log("  --- end dump ---");
    }
    const code = extractErrorCode(errStr);
    if (matchesTaxError(errStr, "PoolMintMismatch")) {
      results.push({
        caseNum, name, expectedError: expected,
        actualError: extractFirstLine(errStr),
        actualCode: code,
        txSignature: (err as any)?.signature ?? null,
        solscanUrl: (err as any)?.signature ? solscanTx((err as any).signature) : null,
        verdict: "REJECTED_TAX",
      });
      console.log(`  PASS — rejected with PoolMintMismatch (${code})`);
    } else if (matchesDownstreamRejection(errStr)) {
      results.push({
        caseNum, name, expectedError: expected,
        actualError: extractFirstLine(errStr),
        actualCode: code,
        txSignature: (err as any)?.signature ?? null,
        solscanUrl: (err as any)?.signature ? solscanTx((err as any).signature) : null,
        verdict: "REJECTED_DOWNSTREAM",
      });
      console.log(`  PASS — rejected downstream (${code}): ${extractFirstLine(errStr)} — defense-in-depth`);
    } else {
      results.push({
        caseNum, name, expectedError: expected,
        actualError: extractFirstLine(errStr, 300),
        actualCode: code,
        txSignature: null,
        solscanUrl: null,
        verdict: "UNEXPECTED_ERROR",
      });
      console.warn(`  UNEXPECTED ERROR (${code}): ${extractFirstLine(errStr, 300)}`);
    }
  }
}

async function runCase4UnknownMint(ctx: SwapContext): Promise<void> {
  const caseNum = 4;
  const name = "swap_sol_sell CRIME pool + mint_b=<random unknown mint>";
  const expected = "UnknownTaxedMint (6021 / 0x1785) or downstream rejection";
  console.log(`\n[Case ${caseNum}] ${name}`);
  console.log(`  Expecting: ${expected}`);

  try {
    const randomMint = Keypair.generate().publicKey;
    // Reuse the CRIME-sell shape but substitute a bogus mint for mint_b.
    // The Anchor InterfaceAccount<Mint> deserialization will fail because
    // a random pubkey is not an initialised Mint account — we expect
    // UnknownTaxedMint IF the account somehow deserializes, or a downstream
    // AccountNotInitialized/ConstraintOwner error as the realistic outcome.
    const { manifest, user, taxProgram } = ctx;
    const pool = manifest.pools["CRIME/SOL"];
    const [swapAuthorityPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("swap_authority")],
      taxProgram.programId
    );

    const accounts = {
      user: user.keypair.publicKey,
      epochState: new PublicKey(manifest.pdas.EpochState),
      swapAuthority: swapAuthorityPda,
      taxAuthority: new PublicKey(manifest.pdas.TaxAuthority),
      pool: new PublicKey(pool.pool),
      poolVaultA: new PublicKey(pool.vaultA),
      poolVaultB: new PublicKey(pool.vaultB),
      mintA: NATIVE_MINT,
      mintB: randomMint, // bogus
      userTokenA: user.wsolAccount,
      userTokenB: user.crimeAccount,
      stakePool: new PublicKey(manifest.pdas.StakePool),
      stakingEscrow: new PublicKey(manifest.pdas.EscrowVault),
      carnageVault: new PublicKey(manifest.pdas.CarnageSolVault),
      treasury: ctx.treasury,
      wsolIntermediary: new PublicKey(manifest.pdas.WsolIntermediary),
      ammProgram: new PublicKey(manifest.programs.AMM),
      tokenProgramA: TOKEN_PROGRAM_ID,
      tokenProgramB: TOKEN_2022_PROGRAM_ID,
      systemProgram: SystemProgram.programId,
      stakingProgram: new PublicKey(manifest.programs.Staking),
    };

    const swapIx = await ctx.taxProgram.methods
      .swapSolSell(new anchor.BN(1_000), new anchor.BN(1), true)
      .accountsStrict(accounts)
      .instruction();

    const tx = new Transaction().add(
      ComputeBudgetProgram.setComputeUnitLimit({ units: 400_000 }),
      swapIx
    );

    const sig = await ctx.provider.sendAndConfirm(tx, [ctx.user.keypair]);
    results.push({
      caseNum, name, expectedError: expected,
      actualError: "UNEXPECTED_SUCCESS",
      actualCode: null,
      txSignature: sig,
      solscanUrl: solscanTx(sig),
      verdict: "UNEXPECTED_SUCCESS",
    });
    console.error(`  CRITICAL: exploit succeeded! TX ${sig}`);
  } catch (err) {
    const errStr = errToFullString(err);
    if (VERBOSE) {
      console.log("  --- full error dump ---");
      console.log(errStr);
      console.log("  --- end dump ---");
    }
    const code = extractErrorCode(errStr);
    if (matchesTaxError(errStr, "UnknownTaxedMint")) {
      results.push({
        caseNum, name, expectedError: expected,
        actualError: extractFirstLine(errStr),
        actualCode: code,
        txSignature: (err as any)?.signature ?? null,
        solscanUrl: (err as any)?.signature ? solscanTx((err as any).signature) : null,
        verdict: "REJECTED_TAX",
      });
      console.log(`  PASS — rejected with UnknownTaxedMint (${code})`);
    } else if (matchesDownstreamRejection(errStr)) {
      results.push({
        caseNum, name, expectedError: expected,
        actualError: extractFirstLine(errStr),
        actualCode: code,
        txSignature: (err as any)?.signature ?? null,
        solscanUrl: (err as any)?.signature ? solscanTx((err as any).signature) : null,
        verdict: "REJECTED_DOWNSTREAM",
      });
      console.log(`  PASS — rejected downstream (${code}): ${extractFirstLine(errStr)} — defense-in-depth`);
    } else {
      results.push({
        caseNum, name, expectedError: expected,
        actualError: extractFirstLine(errStr, 300),
        actualCode: code,
        txSignature: null,
        solscanUrl: null,
        verdict: "UNEXPECTED_ERROR",
      });
      console.warn(`  UNEXPECTED ERROR (${code}): ${extractFirstLine(errStr, 300)}`);
    }
  }
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

async function main(): Promise<void> {
  console.log("=".repeat(72));
  console.log("Tax/Pool Identity Mismatch — On-Chain Exploit Replay");
  console.log("Phase 122.1-02 Task 4 — scripted exploit-only cases (1-4)");
  console.log("=".repeat(72));

  const provider = loadProvider();
  anchor.setProvider(provider);
  const manifest = loadDeployment();
  const devnetTaxPid = new PublicKey(manifest.programs.TaxProgram);
  const taxProgram = loadTaxProgramForCluster(provider, devnetTaxPid);

  console.log(`\nCluster        : ${provider.connection.rpcEndpoint}`);
  console.log(`Tax Program    : ${taxProgram.programId.toBase58()}`);
  console.log(`Funder wallet  : ${provider.wallet.publicKey.toBase58()}`);
  console.log(`CRIME mint     : ${manifest.mints.CRIME}`);
  console.log(`FRAUD mint     : ${manifest.mints.FRAUD}`);
  console.log(`CRIME/SOL pool : ${manifest.pools["CRIME/SOL"].pool}`);
  console.log(`FRAUD/SOL pool : ${manifest.pools["FRAUD/SOL"].pool}`);

  // Create a fresh throwaway user for the exploit attempts. Pre-funds SOL +
  // empty Token-2022 accounts for CRIME/FRAUD and a WSOL ATA. Small budget
  // (0.05 SOL) is more than enough — exploits revert before any meaningful
  // state change.
  console.log("\nProvisioning exploit user…");
  // createE2EUser(provider, mints, solAmountLamports)
  // 0.02 SOL wrapped as WSOL is plenty — exploits revert before any transfer.
  const user = await createE2EUser(
    provider,
    manifest.mints,
    Math.round(0.02 * LAMPORTS_PER_SOL)
  );
  console.log(`  user           : ${user.keypair.publicKey.toBase58()}`);
  console.log(`  user wsol      : ${user.wsolAccount.toBase58()}`);
  console.log(`  user crime ata : ${user.crimeAccount.toBase58()}`);
  console.log(`  user fraud ata : ${user.fraudAccount.toBase58()}`);

  const treasury = loadDevnetTreasury();
  console.log(`  treasury       : ${treasury.toBase58()}`);

  const ctx: SwapContext = { provider, taxProgram, manifest, user, treasury };

  await runCase1CrimeBuyWrongFlag(ctx);
  await runCase2FraudSellWrongFlag(ctx);
  await runCase3CrimePoolFraudMint(ctx);
  await runCase4UnknownMint(ctx);

  // ------ Summary ------
  console.log("\n" + "=".repeat(72));
  console.log("RESULTS");
  console.log("=".repeat(72));

  let allBlocked = true;
  for (const r of results) {
    const blocked =
      r.verdict === "REJECTED_TAX" || r.verdict === "REJECTED_DOWNSTREAM";
    if (!blocked) allBlocked = false;
    console.log(
      `\nCase ${r.caseNum}: ${r.verdict}${r.actualCode ? ` (${r.actualCode})` : ""}`
    );
    console.log(`  ${r.name}`);
    console.log(`  expected: ${r.expectedError}`);
    console.log(`  actual  : ${r.actualError}`);
    if (r.solscanUrl) console.log(`  solscan : ${r.solscanUrl}`);
  }

  console.log("\n" + "=".repeat(72));
  if (allBlocked) {
    console.log("ALL 4 EXPLOITS BLOCKED — fix verified on devnet");
    console.log("=".repeat(72));
    process.exit(0);
  } else {
    console.error("ONE OR MORE EXPLOITS NOT BLOCKED — FIX BROKEN OR TEST BUG");
    console.log("=".repeat(72));
    process.exit(1);
  }
}

main().catch((err) => {
  console.error("FATAL:", err);
  process.exit(2);
});
