/**
 * Mainnet Conversion Vault Upgrade via Squads
 *
 * Creates a Squads upgrade proposal for the conversion vault on mainnet.
 * Buffer must already be written and authority transferred to vault PDA.
 *
 * This script automates signer 1 (file keypair) approval only.
 * Signer 2 (Ledger) must approve manually via Squads UI to reach 2-of-3 threshold.
 *
 * Usage:
 *   set -a && source .env.mainnet && set +a
 *   BUFFER_ADDRESS=<address> npx tsx scripts/deploy/mainnet-upgrade-vault.ts
 *
 * Required env:
 *   CLUSTER_URL     - Mainnet RPC URL (from .env.mainnet)
 *   BUFFER_ADDRESS  - Buffer account address (from write-buffer step)
 *
 * Source: Phase 109-02 Plan
 */

import * as multisig from "@sqds/multisig";
import {
  Connection,
  Keypair,
  PublicKey,
  TransactionMessage,
  TransactionInstruction,
} from "@solana/web3.js";
import * as fs from "fs";
import * as path from "path";

// =============================================================================
// Constants — all from deployments/mainnet.json
// =============================================================================

const ROOT = path.resolve(__dirname, "../..");

const CONVERSION_VAULT_PROGRAM_ID = new PublicKey(
  "5uawA6ehYTu69Ggvm3LSK84qFawPKxbWgfngwj15NRJ"
);

const MULTISIG_PDA = new PublicKey(
  "F7axBNUgWQQ33ZYLdenCk5SV3wBrKyYz9R7MscdPJi1A"
);

const VAULT_PDA = new PublicKey(
  "4SMcPtixKvjgj3U5N7C4kcnHYcySudLZfFWc523NAvXJ"
);

const BPF_LOADER_UPGRADEABLE = new PublicKey(
  "BPFLoaderUpgradeab1e11111111111111111111111"
);

const SYSVAR_RENT = new PublicKey(
  "SysvarRent111111111111111111111111111111111"
);

const SYSVAR_CLOCK = new PublicKey(
  "SysvarC1ock11111111111111111111111111111111"
);

// Deployer keypair (fee payer, NOT an upgrade authority)
const DEPLOYER_PATH = path.join(
  process.env.HOME || "~",
  "mainnet-keys/deployer.json"
);

// Signer 1 (file keypair — the only automatable signer)
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

async function getProgramDataInfo(
  connection: Connection,
  programId: PublicKey
): Promise<{ programData: PublicKey; lastDeploySlot: bigint }> {
  const programInfo = await connection.getAccountInfo(programId);
  if (!programInfo || programInfo.data.length < 36) {
    throw new Error("Could not read program account");
  }

  const programData = new PublicKey(programInfo.data.slice(4, 36));
  const programDataInfo = await connection.getAccountInfo(programData);
  if (!programDataInfo) {
    throw new Error("Could not read ProgramData account");
  }

  const lastDeploySlot = programDataInfo.data.readBigUInt64LE(4);
  return { programData, lastDeploySlot };
}

/**
 * BPFLoaderUpgradeable::Upgrade instruction.
 * Discriminator: 3 (u32 LE)
 * Accounts: [programData(w), program(w), buffer(w), spill(w), rent(r), clock(r), authority(s)]
 */
function makeUpgradeIx(
  programId: PublicKey,
  programData: PublicKey,
  bufferAddress: PublicKey,
  spillAddress: PublicKey,
  authority: PublicKey
): TransactionInstruction {
  const data = Buffer.alloc(4);
  data.writeUInt32LE(3, 0);
  return new TransactionInstruction({
    keys: [
      { pubkey: programData, isSigner: false, isWritable: true },
      { pubkey: programId, isSigner: false, isWritable: true },
      { pubkey: bufferAddress, isSigner: false, isWritable: true },
      { pubkey: spillAddress, isSigner: false, isWritable: true },
      { pubkey: SYSVAR_RENT, isSigner: false, isWritable: false },
      { pubkey: SYSVAR_CLOCK, isSigner: false, isWritable: false },
      { pubkey: authority, isSigner: true, isWritable: false },
    ],
    programId: BPF_LOADER_UPGRADEABLE,
    data,
  });
}

// =============================================================================
// Main
// =============================================================================

async function main() {
  console.log("=== Mainnet Conversion Vault Upgrade ===\n");

  // Validate inputs
  const bufferAddress = process.env.BUFFER_ADDRESS;
  if (!bufferAddress) {
    throw new Error("BUFFER_ADDRESS env var required");
  }
  const bufferPubkey = new PublicKey(bufferAddress);

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

  console.log(`Program:  ${CONVERSION_VAULT_PROGRAM_ID.toBase58()}`);
  console.log(`Buffer:   ${bufferAddress}`);
  console.log(`Multisig: ${MULTISIG_PDA.toBase58()}`);
  console.log(`Vault:    ${VAULT_PDA.toBase58()}`);
  console.log(`Deployer: ${deployer.publicKey.toBase58()} (fee payer)`);
  console.log(`Signer 1: ${signer1.publicKey.toBase58()} (file keypair)`);
  console.log("");

  // Step 1: Read current program state
  console.log("[1/5] Reading current program state...");
  const pdInfo = await getProgramDataInfo(connection, CONVERSION_VAULT_PROGRAM_ID);
  console.log(`  ProgramData:     ${pdInfo.programData.toBase58()}`);
  console.log(`  last_deploy_slot: ${pdInfo.lastDeploySlot}`);

  // Step 2: Get next transaction index
  console.log("\n[2/5] Reading multisig state...");
  const msAccount = await multisig.accounts.Multisig.fromAccountAddress(
    connection,
    MULTISIG_PDA
  );
  const currentTxIndex = Number(msAccount.transactionIndex);
  const txIndex = BigInt(currentTxIndex + 1);
  console.log(`  Current TX index: ${currentTxIndex}`);
  console.log(`  Next TX index:    ${txIndex}`);
  console.log(`  Time lock:        ${msAccount.timeLock}s`);

  // Step 3: Create vault transaction
  console.log("\n[3/5] Creating vault transaction (Upgrade IX)...");
  const upgradeIx = makeUpgradeIx(
    CONVERSION_VAULT_PROGRAM_ID,
    pdInfo.programData,
    bufferPubkey,
    VAULT_PDA, // spill address — rent goes back to vault
    VAULT_PDA  // authority — vault PDA is the upgrade authority
  );

  const { blockhash } = await connection.getLatestBlockhash();
  const txMessage = new TransactionMessage({
    payerKey: VAULT_PDA,
    recentBlockhash: blockhash,
    instructions: [upgradeIx],
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
    memo: "Upgrade conversion vault: convert_v2 delta mode (pre_balance param)",
    signers: [deployer, signer1],
    sendOptions: { skipPreflight: true },
  });
  console.log(`  Vault TX created: ${vtSig}`);
  await confirmOrThrow(connection, vtSig, "vault TX create");

  // Step 4: Create proposal
  console.log("\n[4/5] Creating proposal...");
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

  // Step 5: Approve with signer 1
  console.log("\n[5/5] Approving with signer 1...");
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

  // Done — print instructions for Ledger approval
  const timelockSeconds = Number(msAccount.timeLock);

  console.log("\n" + "=".repeat(60));
  console.log("  PROPOSAL CREATED — LEDGER APPROVAL REQUIRED");
  console.log("=".repeat(60));
  console.log("");
  console.log(`  Transaction index: ${txIndex}`);
  console.log(`  Signer 1 approved: ✓`);
  console.log(`  Signer 2 needed:   Connect Ledger → Squads UI`);
  console.log(`  Timelock:          ${timelockSeconds}s (${Math.round(timelockSeconds / 60)} min) after 2nd approval`);
  console.log("");
  console.log("  Next steps:");
  console.log("  1. Open https://app.squads.so/");
  console.log(`  2. Connect Ledger wallet (signer 2)`);
  console.log(`  3. Navigate to the pending proposal (TX #${txIndex})`);
  console.log("  4. Click 'Approve' — Ledger will prompt for confirmation");
  console.log(`  5. Wait ${timelockSeconds}s (${Math.round(timelockSeconds / 60)} min) for timelock to expire`);
  console.log("  6. Execute the upgrade (Plan 109-03)");
  console.log("");
  console.log("  Pre-upgrade state (for verification):");
  console.log(`  last_deploy_slot: ${pdInfo.lastDeploySlot}`);
  console.log(`  binary hash:      ${process.env.BINARY_HASH || "(set BINARY_HASH env for logging)"}`);
  console.log("");
}

main().catch((err) => {
  console.error("\nFATAL:", err.message || err);
  process.exit(1);
});
