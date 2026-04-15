/**
 * Manual one-shot trigger for `executeCarnageAtomic` on mainnet.
 *
 * Use case: when atomic bundling in consume_randomness failed to land,
 * `carnage_pending` stays true on EpochState. This script lets a permissionless
 * caller execute the pending carnage as a standalone TX.
 *
 * Pre-flight: run `check-mainnet-carnage-state.ts` first to confirm the state
 * is actually pending (otherwise this is a wasted no-op TX).
 *
 * Signer: defaults to ~/mainnet-keys/crank.json (set CRANK_KEYPAIR to override).
 *
 * Usage:
 *   set -a && source .env.mainnet && set +a
 *   npx tsx scripts/crank/manual-execute-carnage.ts [--dry-run]
 */

import { AnchorProvider, Program, Wallet } from "@coral-xyz/anchor";
import {
  AddressLookupTableAccount,
  ComputeBudgetProgram,
  Connection,
  Keypair,
  PublicKey,
} from "@solana/web3.js";
import * as fs from "fs";
import * as os from "os";
import * as path from "path";

import { buildExecuteCarnageAtomicIx } from "../e2e/lib/carnage-flow";
import { sendV0Transaction } from "../e2e/lib/alt-helper";
import { VRFAccounts } from "../vrf/lib/vrf-flow";
import { NATIVE_MINT } from "@solana/spl-token";

const DRY_RUN = process.argv.includes("--dry-run");

function loadKeypair(filePath: string): Keypair {
  const expanded = filePath.startsWith("~")
    ? path.join(os.homedir(), filePath.slice(1))
    : filePath;
  const raw = JSON.parse(fs.readFileSync(expanded, "utf-8"));
  return Keypair.fromSecretKey(Uint8Array.from(raw));
}

async function main() {
  // ─── Load manifest ──────────────────────────────────────────────────────
  const manifest = JSON.parse(
    fs.readFileSync(path.resolve("deployments/mainnet.json"), "utf-8")
  );

  // ─── RPC + provider ─────────────────────────────────────────────────────
  const apiKey = process.env.HELIUS_API_KEY;
  if (!apiKey) {
    throw new Error("HELIUS_API_KEY not set (source .env.mainnet first)");
  }
  const rpcUrl = `https://mainnet.helius-rpc.com/?api-key=${apiKey}`;
  const connection = new Connection(rpcUrl, "confirmed");

  // ─── Signer ─────────────────────────────────────────────────────────────
  const keypairPath = process.env.CRANK_KEYPAIR ?? "~/mainnet-keys/crank.json";
  const signer = loadKeypair(keypairPath);
  const wallet = new Wallet(signer);
  console.log(`Signer:        ${signer.publicKey.toBase58()}`);
  console.log(`Keypair file:  ${keypairPath}`);

  const balance = await connection.getBalance(signer.publicKey);
  console.log(`Balance:       ${(balance / 1e9).toFixed(6)} SOL`);
  if (balance < 5_000_000) {
    throw new Error("Signer balance < 0.005 SOL — insufficient for fee");
  }

  const provider = new AnchorProvider(connection, wallet, {
    commitment: "confirmed",
  });

  // ─── Load Epoch Program ─────────────────────────────────────────────────
  const idl = JSON.parse(
    fs.readFileSync(path.resolve("target/idl/epoch_program.json"), "utf-8")
  );
  idl.address = manifest.programs.epochProgram;
  const epochProgram = new Program(idl, provider);

  // ─── Pre-flight: confirm carnage_pending ────────────────────────────────
  const epochStatePda = new PublicKey(manifest.pdas.epochState);
  const epochStateBefore = await (epochProgram.account as any).epochState.fetch(
    epochStatePda
  );
  console.log("\n--- Pre-flight EpochState ---");
  console.log(`  currentEpoch:     ${epochStateBefore.currentEpoch}`);
  console.log(`  carnagePending:   ${epochStateBefore.carnagePending}`);
  console.log(`  lastCarnageEpoch: ${epochStateBefore.lastCarnageEpoch}`);
  console.log(`  carnageAction:    ${JSON.stringify(epochStateBefore.carnageAction)} (0=Sell, 1=Burn)`);
  console.log(`  carnageTarget:    ${JSON.stringify(epochStateBefore.carnageTarget)} (0=Crime, 1=Fraud)`);

  if (!epochStateBefore.carnagePending) {
    console.log("\n❌ carnage_pending is FALSE. Nothing to execute. Aborting.");
    process.exit(1);
  }

  // ─── Build VRFAccounts from mainnet manifest ────────────────────────────
  const carnageWsol = process.env.CARNAGE_WSOL_PUBKEY;
  if (!carnageWsol) {
    throw new Error("CARNAGE_WSOL_PUBKEY env var not set (source .env.mainnet)");
  }

  const accounts: VRFAccounts = {
    epochStatePda,
    treasuryPda: signer.publicKey, // unused by carnage builder
    stakingAuthorityPda: new PublicKey(manifest.pdas.stakingAuthority),
    stakePoolPda: new PublicKey(manifest.pdas.stakePool),
    stakingProgramId: new PublicKey(manifest.programs.staking),
    carnageFundPda: new PublicKey(manifest.pdas.carnageFund),
    carnageAccounts: {
      carnageSignerPda: new PublicKey(manifest.pdas.carnageSigner),
      carnageSolVault: new PublicKey(manifest.pdas.carnageSolVault),
      carnageWsol: new PublicKey(carnageWsol),
      carnageCrimeVault: new PublicKey(manifest.pdas.carnageCrimeVault),
      carnageFraudVault: new PublicKey(manifest.pdas.carnageFraudVault),
      crimePool: new PublicKey(manifest.pools.crimeSol.pool),
      crimePoolVaultA: new PublicKey(manifest.pools.crimeSol.vaultA),
      crimePoolVaultB: new PublicKey(manifest.pools.crimeSol.vaultB),
      fraudPool: new PublicKey(manifest.pools.fraudSol.pool),
      fraudPoolVaultA: new PublicKey(manifest.pools.fraudSol.vaultA),
      fraudPoolVaultB: new PublicKey(manifest.pools.fraudSol.vaultB),
      mintA: NATIVE_MINT,
      crimeMint: new PublicKey(manifest.mints.crime),
      fraudMint: new PublicKey(manifest.mints.fraud),
      taxProgram: new PublicKey(manifest.programs.taxProgram),
      ammProgram: new PublicKey(manifest.programs.amm),
      swapAuthority: new PublicKey(manifest.pdas.swapAuthority),
    },
  };

  // ─── Build IX (handles 23 named accounts + Transfer Hook resolution) ─────
  console.log("\n--- Building executeCarnageAtomic IX ---");
  const carnageIx = await buildExecuteCarnageAtomicIx(
    epochProgram,
    accounts,
    signer.publicKey,
    connection
  );
  console.log(`  Named + remaining accounts: ${carnageIx.keys.length}`);

  // ─── Load protocol ALT ──────────────────────────────────────────────────
  const altPubkey = new PublicKey(manifest.alt);
  console.log(`  ALT: ${altPubkey.toBase58()}`);
  const altResp = await connection.getAddressLookupTable(altPubkey);
  if (!altResp.value) {
    throw new Error(`Failed to fetch ALT ${altPubkey.toBase58()}`);
  }
  const alt: AddressLookupTableAccount = altResp.value;
  console.log(`  ALT addresses: ${alt.state.addresses.length}`);

  const instructions = [
    ComputeBudgetProgram.setComputeUnitLimit({ units: 400_000 }),
    ComputeBudgetProgram.setComputeUnitPrice({ microLamports: 50_000 }), // ~0.00002 SOL priority
    carnageIx,
  ];

  if (DRY_RUN) {
    console.log("\n[--dry-run] Not sending. Instruction built successfully.");
    return;
  }

  // ─── Send v0 TX with ALT ────────────────────────────────────────────────
  console.log("\n--- Sending v0 TX (skipPreflight=true, ALT-compressed) ---");
  let txSig: string;
  try {
    txSig = await sendV0Transaction(
      connection,
      signer.publicKey,
      instructions,
      [signer],
      alt
    );
  } catch (err: any) {
    console.error("\n❌ TX FAILED");
    console.error(err.message ?? String(err));
    if (err.txSig) {
      console.error(`Solscan: https://solscan.io/tx/${err.txSig}`);
    }
    process.exit(1);
  }
  console.log(`\n✅ TX landed: ${txSig}`);
  console.log(`   Solscan: https://solscan.io/tx/${txSig}`);

  // ─── Verify state flipped ───────────────────────────────────────────────
  console.log("\n--- Waiting 2s for RPC propagation ---");
  await new Promise((r) => setTimeout(r, 2000));

  const epochStateAfter = await (epochProgram.account as any).epochState.fetch(
    epochStatePda
  );
  console.log("\n--- Post-TX EpochState ---");
  console.log(`  currentEpoch:     ${epochStateAfter.currentEpoch}`);
  console.log(`  carnagePending:   ${epochStateAfter.carnagePending}`);
  console.log(`  lastCarnageEpoch: ${epochStateAfter.lastCarnageEpoch}`);

  if (
    !epochStateAfter.carnagePending &&
    epochStateAfter.lastCarnageEpoch === epochStateBefore.currentEpoch
  ) {
    console.log("\n✅ SUCCESS — carnage_pending cleared, lastCarnageEpoch advanced.");
  } else if (!epochStateAfter.carnagePending) {
    console.log(
      "\n⚠️  carnage_pending=false but lastCarnageEpoch did not advance. Investigate."
    );
  } else {
    console.log(
      "\n⚠️  carnage_pending still TRUE. TX landed but may have hit no-op or partial path."
    );
  }
}

main().catch((err) => {
  console.error("FATAL:", err);
  process.exit(1);
});
