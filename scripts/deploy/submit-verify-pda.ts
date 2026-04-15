/**
 * Submit OtterSec Verification PDA via Squads Multisig
 *
 * Takes a base58-encoded transaction from `solana-verify export-pda-tx`
 * and wraps it in a Squads vault transaction proposal.
 *
 * Usage:
 *   set -a && source .env.mainnet && set +a
 *   PDA_TX_BASE58="<base58 from export-pda-tx>" npx tsx scripts/deploy/submit-verify-pda.ts
 *
 * Source: Phase 109 OtterSec re-verification pattern, adapted for programmatic submission
 */

import * as multisig from "@sqds/multisig";
import {
  Connection,
  Keypair,
  PublicKey,
  Transaction,
  TransactionMessage,
} from "@solana/web3.js";
import * as bs58 from "bs58";
import * as fs from "fs";
import * as path from "path";

// =============================================================================
// Constants
// =============================================================================

const ROOT = path.resolve(__dirname, "../..");

const MULTISIG_PDA = new PublicKey(
  "F7axBNUgWQQ33ZYLdenCk5SV3wBrKyYz9R7MscdPJi1A"
);

const VAULT_PDA = new PublicKey(
  "4SMcPtixKvjgj3U5N7C4kcnHYcySudLZfFWc523NAvXJ"
);

const DEPLOYER_PATH = path.join(
  process.env.HOME || "~",
  "mainnet-keys/deployer.json"
);

const SIGNER_1_PATH = path.join(ROOT, "keypairs/squads-signer-1.json");

// =============================================================================
// Helpers
// =============================================================================

function loadKeypair(filePath: string): Keypair {
  const resolved = path.resolve(filePath);
  if (!fs.existsSync(resolved)) {
    throw new Error(`Keypair not found: ${resolved}`);
  }
  return Keypair.fromSecretKey(
    Uint8Array.from(JSON.parse(fs.readFileSync(resolved, "utf-8")))
  );
}

async function confirmOrThrow(
  connection: Connection,
  sig: string,
  label: string
): Promise<void> {
  const result = await connection.confirmTransaction(sig, "confirmed");
  if (result.value.err) {
    try {
      const tx = await connection.getTransaction(sig, {
        maxSupportedTransactionVersion: 0,
      });
      if (tx?.meta?.logMessages) {
        console.error(`  Logs for ${label}:`);
        for (const log of tx.meta.logMessages) {
          console.error(`    ${log}`);
        }
      }
    } catch {
      // Ignore log fetch errors
    }
    throw new Error(
      `${label} failed on-chain: ${JSON.stringify(result.value.err)}`
    );
  }
}

// =============================================================================
// Main
// =============================================================================

async function main() {
  console.log("=== Submit OtterSec Verification PDA via Squads ===\n");

  // Validate inputs
  const pdaTxBase58 = process.env.PDA_TX_BASE58;
  if (!pdaTxBase58) {
    throw new Error("PDA_TX_BASE58 env var required (from solana-verify export-pda-tx)");
  }

  const clusterUrl = process.env.CLUSTER_URL;
  if (!clusterUrl) {
    throw new Error("CLUSTER_URL env var required (set -a && source .env.mainnet && set +a)");
  }
  if (!clusterUrl.includes("mainnet")) {
    throw new Error(`CLUSTER_URL does not look like mainnet: ${clusterUrl}`);
  }

  const connection = new Connection(clusterUrl, "confirmed");

  // Load keypairs
  const deployer = loadKeypair(DEPLOYER_PATH);
  const signer1 = loadKeypair(SIGNER_1_PATH);

  console.log(`Multisig: ${MULTISIG_PDA.toBase58()}`);
  console.log(`Vault:    ${VAULT_PDA.toBase58()}`);
  console.log(`Deployer: ${deployer.publicKey.toBase58()} (fee payer)`);
  console.log(`Signer 1: ${signer1.publicKey.toBase58()} (file keypair)`);
  console.log("");

  // Step 1: Deserialize the PDA transaction to extract instructions
  console.log("[1/4] Deserializing PDA transaction...");
  const txBytes = bs58.default.decode(pdaTxBase58);
  const pdaTx = Transaction.from(txBytes);
  const instructions = pdaTx.instructions;
  console.log(`  Instructions found: ${instructions.length}`);
  for (let i = 0; i < instructions.length; i++) {
    console.log(`  IX ${i}: program=${instructions[i].programId.toBase58()}, keys=${instructions[i].keys.length}`);
  }

  // Step 2: Get next transaction index
  console.log("\n[2/4] Reading multisig state...");
  const msAccount = await multisig.accounts.Multisig.fromAccountAddress(
    connection,
    MULTISIG_PDA
  );
  const currentTxIndex = Number(msAccount.transactionIndex);
  const txIndex = BigInt(currentTxIndex + 1);
  console.log(`  Current TX index: ${currentTxIndex}`);
  console.log(`  Next TX index:    ${txIndex}`);
  console.log(`  Time lock:        ${msAccount.timeLock}s`);

  // Step 3: Create vault transaction wrapping the PDA instructions
  console.log("\n[3/4] Creating vault transaction (OtterSec PDA)...");
  const { blockhash } = await connection.getLatestBlockhash();
  const txMessage = new TransactionMessage({
    payerKey: VAULT_PDA,
    recentBlockhash: blockhash,
    instructions: instructions,
  });

  const vtSig = await multisig.rpc.vaultTransactionCreate({
    connection,
    feePayer: deployer,
    multisigPda: MULTISIG_PDA,
    transactionIndex: txIndex,
    creator: signer1.publicKey,
    vaultIndex: 0,
    ephemeralSigners: 0,
    transactionMessage: txMessage,
    memo: "OtterSec verification PDA: conversion vault delta mode upgrade",
    signers: [deployer, signer1],
    sendOptions: { skipPreflight: true },
  });
  console.log(`  Vault TX created: ${vtSig}`);
  await confirmOrThrow(connection, vtSig, "vault TX create");

  // Step 4: Create proposal + approve with signer 1
  console.log("\n[4/4] Creating proposal + signer 1 approval...");
  const propSig = await multisig.rpc.proposalCreate({
    connection,
    feePayer: deployer,
    creator: signer1,
    multisigPda: MULTISIG_PDA,
    transactionIndex: txIndex,
    sendOptions: { skipPreflight: true },
  });
  console.log(`  Proposal created: ${propSig}`);
  await confirmOrThrow(connection, propSig, "proposal create");

  const approveSig = await multisig.rpc.proposalApprove({
    connection,
    feePayer: deployer,
    member: signer1,
    multisigPda: MULTISIG_PDA,
    transactionIndex: txIndex,
    sendOptions: { skipPreflight: true },
  });
  console.log(`  Signer 1 approved: ${approveSig}`);
  await confirmOrThrow(connection, approveSig, "signer 1 approve");

  const timelockSeconds = Number(msAccount.timeLock);

  console.log("\n" + "=".repeat(60));
  console.log("  PDA PROPOSAL CREATED — APPROVAL REQUIRED");
  console.log("=".repeat(60));
  console.log("");
  console.log(`  Transaction index: ${txIndex}`);
  console.log(`  Signer 1 approved: ✓`);
  console.log(`  Signer 2 needed:   Squads UI`);
  console.log(`  Timelock:          ${timelockSeconds}s (${Math.round(timelockSeconds / 60)} min)`);
  console.log("");
  console.log("  After execution, verify at:");
  console.log("  https://verify.osec.io/status/5uawA6ehYTu69Ggvm3LSK84qFawPKxbWgfngwj15NRJ");
  console.log("");
}

main().catch((err) => {
  console.error("\nFATAL:", err.message || err);
  process.exit(1);
});
