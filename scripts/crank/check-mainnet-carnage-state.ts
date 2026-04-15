/**
 * Read-only check of mainnet EpochState + CarnageFundState.
 *
 * Confirms whether `carnage_pending` is still true so we know whether the
 * manual `executeCarnageAtomic` trigger is still viable.
 *
 * Usage: npx tsx scripts/crank/check-mainnet-carnage-state.ts
 *
 * SAFETY: Pure read. No transactions sent. No keypair needed.
 */

import { AnchorProvider, Program } from "@coral-xyz/anchor";
import { Connection, Keypair, PublicKey } from "@solana/web3.js";
import { Wallet } from "@coral-xyz/anchor";
import * as fs from "fs";
import * as path from "path";

async function main() {
  const manifest = JSON.parse(
    fs.readFileSync(path.resolve("deployments/mainnet.json"), "utf-8")
  );

  const apiKey = process.env.HELIUS_API_KEY;
  if (!apiKey) {
    throw new Error("HELIUS_API_KEY not set (source .env.mainnet first)");
  }
  const rpcUrl = `https://mainnet.helius-rpc.com/?api-key=${apiKey}`;

  const connection = new Connection(rpcUrl, "confirmed");

  // Dummy wallet — we never sign anything
  const dummyWallet = new Wallet(Keypair.generate());
  const provider = new AnchorProvider(connection, dummyWallet, {
    commitment: "confirmed",
  });

  // Load epoch program IDL and instantiate
  const idlPath = path.resolve("target/idl/epoch_program.json");
  const idl = JSON.parse(fs.readFileSync(idlPath, "utf-8"));
  // Override IDL address with mainnet program ID
  idl.address = manifest.programs.epochProgram;
  const epochProgram = new Program(idl, provider);

  const epochStatePda = new PublicKey(manifest.pdas.epochState);
  const carnageFundPda = new PublicKey(manifest.pdas.carnageFund);

  console.log("=== Mainnet Carnage State Check ===\n");
  console.log(`Epoch Program: ${manifest.programs.epochProgram}`);
  console.log(`EpochState PDA: ${epochStatePda.toBase58()}`);
  console.log(`CarnageFund PDA: ${carnageFundPda.toBase58()}\n`);

  // Current slot for context
  const currentSlot = await connection.getSlot("confirmed");
  console.log(`Current slot: ${currentSlot}\n`);

  // Read EpochState
  const epochState = await (epochProgram.account as any).epochState.fetch(
    epochStatePda
  );

  console.log("--- EpochState ---");
  console.log(`  currentEpoch:           ${epochState.currentEpoch}`);
  console.log(`  vrfPending:             ${epochState.vrfPending}`);
  console.log(`  taxesConfirmed:         ${epochState.taxesConfirmed}`);
  console.log(`  carnagePending:         ${epochState.carnagePending}  ${epochState.carnagePending ? "<-- ACTIONABLE" : ""}`);
  console.log(`  lastCarnageEpoch:       ${epochState.lastCarnageEpoch}`);

  // carnage_action / carnage_target may not be in IDL camelCase form — try both
  const action =
    epochState.carnageAction ?? epochState.carnage_action ?? "(field missing)";
  const target =
    epochState.carnageTarget ?? epochState.carnage_target ?? "(field missing)";
  const deadline =
    epochState.carnageDeadlineSlot ??
    epochState.carnage_deadline_slot ??
    "(field missing)";

  console.log(`  carnageAction:          ${JSON.stringify(action)} (0=Sell, 1=Burn)`);
  console.log(`  carnageTarget:          ${JSON.stringify(target)} (0=Crime, 1=Fraud)`);
  const deadlineNum = typeof deadline === "object" && deadline?.toNumber
    ? deadline.toNumber()
    : Number(deadline);
  console.log(`  carnageDeadlineSlot:    ${deadlineNum}`);
  if (Number.isFinite(deadlineNum) && deadlineNum > 0) {
    const slotsAgo = currentSlot - deadlineNum;
    console.log(`    -> ${slotsAgo > 0 ? "PASSED" : "in"} ${Math.abs(slotsAgo)} slots ${slotsAgo > 0 ? "ago" : "from now"} (${(Math.abs(slotsAgo) * 0.4).toFixed(0)}s)`);
  }

  // Read CarnageFundState
  console.log("\n--- CarnageFundState ---");
  const carnageState = await (epochProgram.account as any).carnageFundState.fetch(
    carnageFundPda
  );
  const heldAmount = carnageState.heldAmount?.toNumber
    ? carnageState.heldAmount.toNumber()
    : Number(carnageState.heldAmount);
  const heldToken = carnageState.heldToken;
  console.log(`  heldAmount:             ${heldAmount}`);
  console.log(`  heldToken:              ${heldToken} (0=None, 1=Crime, 2=Fraud)`);
  console.log(`  initialized:            ${carnageState.initialized}`);

  // Verdict
  console.log("\n--- Verdict ---");
  if (epochState.carnagePending) {
    console.log("  ✅ carnage_pending=TRUE — manual executeCarnageAtomic will execute the carnage.");
    console.log("     The deadline does NOT gate this path; only carnage_pending matters.");
  } else {
    console.log("  ❌ carnage_pending=FALSE — nothing to execute. Either the bundled");
    console.log("     atomic call already ran, or the fallback path cleared it.");
    console.log(`     lastCarnageEpoch=${epochState.lastCarnageEpoch} vs currentEpoch=${epochState.currentEpoch}`);
  }
}

main().catch((err) => {
  console.error("ERROR:", err);
  process.exit(1);
});
