/**
 * Mainnet Tax Program Upgrade via Squads
 *
 * Creates a Squads upgrade proposal for the Tax Program on mainnet.
 * Buffer must already be written and authority transferred to vault PDA.
 *
 * Adapted from Phase 109's mainnet-upgrade-vault.ts with:
 * - Tax Program target (43fZ...)
 * - Proposal PDA pre-funding (per memory/feedback_squads_proposal_prefund.md)
 * - Manual BPFLoaderUpgradeable encoder (per memory/project_squads_encoding_bugs.md)
 *
 * Usage:
 *   set -a && source .env.mainnet && set +a
 *   BUFFER_ADDRESS=3zSBHUAxT6KiqhgzoukQtHFpZtBSPNzFuFiyJ6MTTFyj npx tsx scripts/deploy/mainnet-upgrade-tax.ts
 *
 * Required env:
 *   CLUSTER_URL     - Mainnet RPC URL (from .env.mainnet)
 *   BUFFER_ADDRESS  - Buffer account address (from write-buffer step)
 *
 * Source: Phase 122.1-03 Plan
 */

import * as multisig from "@sqds/multisig";
import {
  Connection,
  Keypair,
  PublicKey,
  SystemProgram,
  Transaction,
  TransactionMessage,
  TransactionInstruction,
  sendAndConfirmTransaction,
} from "@solana/web3.js";
import * as fs from "fs";
import * as path from "path";

// =============================================================================
// Constants — all from deployments/mainnet.json
// =============================================================================

const ROOT = path.resolve(__dirname, "../..");

const TAX_PROGRAM_ID = new PublicKey(
  "43fZGRtmEsP7ExnJE1dbTbNjaP1ncvVmMPusSeksWGEj"
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

// Pre-fund amount for proposal PDA (lamports)
const PROPOSAL_PREFUND_LAMPORTS = 10_000_000; // 0.01 SOL

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
 *
 * CRITICAL: This uses the manual bincode-style encoder, NOT BorshCoder.
 * BorshCoder has previously burned authorities in this project.
 * See memory/project_squads_encoding_bugs.md.
 */
function makeUpgradeIx(
  programId: PublicKey,
  programData: PublicKey,
  bufferAddress: PublicKey,
  spillAddress: PublicKey,
  authority: PublicKey
): TransactionInstruction {
  const data = Buffer.alloc(4);
  data.writeUInt32LE(3, 0); // discriminator 3 = Upgrade
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
  console.log("=== Mainnet Tax Program Upgrade (Phase 122.1-03) ===\n");

  // Validate inputs
  const bufferAddress = process.env.BUFFER_ADDRESS;
  if (!bufferAddress) {
    throw new Error("BUFFER_ADDRESS env var required");
  }
  const bufferPubkey = new PublicKey(bufferAddress);

  const clusterUrl = process.env.CLUSTER_URL;
  if (!clusterUrl) {
    throw new Error(
      "CLUSTER_URL env var required (set -a && source .env.mainnet && set +a)"
    );
  }
  if (!clusterUrl.includes("mainnet")) {
    throw new Error(`CLUSTER_URL does not look like mainnet: ${clusterUrl}`);
  }

  const connection = new Connection(clusterUrl, "confirmed");

  // Load keypairs
  const deployer = loadKeypair(DEPLOYER_PATH);
  const signer1 = loadKeypair(SIGNER_1_PATH);

  console.log(`Program:  ${TAX_PROGRAM_ID.toBase58()}`);
  console.log(`Buffer:   ${bufferAddress}`);
  console.log(`Multisig: ${MULTISIG_PDA.toBase58()}`);
  console.log(`Vault:    ${VAULT_PDA.toBase58()}`);
  console.log(`Deployer: ${deployer.publicKey.toBase58()} (fee payer)`);
  console.log(`Signer 1: ${signer1.publicKey.toBase58()} (file keypair)`);
  console.log("");

  // Step 1: Read current program state
  console.log("[1/7] Reading current program state...");
  const pdInfo = await getProgramDataInfo(connection, TAX_PROGRAM_ID);
  console.log(`  ProgramData:      ${pdInfo.programData.toBase58()}`);
  console.log(`  last_deploy_slot: ${pdInfo.lastDeploySlot}`);

  // Step 2: Get next transaction index
  console.log("\n[2/7] Reading multisig state...");
  const msAccount = await multisig.accounts.Multisig.fromAccountAddress(
    connection,
    MULTISIG_PDA
  );
  const currentTxIndex = Number(msAccount.transactionIndex);
  const txIndex = BigInt(currentTxIndex + 1);
  console.log(`  Current TX index: ${currentTxIndex}`);
  console.log(`  Next TX index:    ${txIndex}`);
  console.log(`  Time lock:        ${msAccount.timeLock}s`);

  // Step 3: Pre-fund the vault transaction PDA AND proposal PDA
  // The vault transaction account stores the serialized IX and can be large enough
  // to fail rent-exemption during vaultTransactionCreate. The proposal PDA can
  // similarly fail during proposalCreate. Pre-fund both with 0.01 SOL each.
  console.log("\n[3/7] Pre-funding vault transaction PDA + proposal PDA...");

  const [vaultTransactionPda] = multisig.getTransactionPda({
    multisigPda: MULTISIG_PDA,
    index: txIndex,
  });
  console.log(`  Vault TX PDA:  ${vaultTransactionPda.toBase58()}`);

  const [proposalPda] = multisig.getProposalPda({
    multisigPda: MULTISIG_PDA,
    transactionIndex: txIndex,
  });
  console.log(`  Proposal PDA:  ${proposalPda.toBase58()}`);

  const prefundTx = new Transaction().add(
    SystemProgram.transfer({
      fromPubkey: deployer.publicKey,
      toPubkey: vaultTransactionPda,
      lamports: PROPOSAL_PREFUND_LAMPORTS,
    }),
    SystemProgram.transfer({
      fromPubkey: deployer.publicKey,
      toPubkey: proposalPda,
      lamports: PROPOSAL_PREFUND_LAMPORTS,
    })
  );
  const prefundSig = await sendAndConfirmTransaction(
    connection,
    prefundTx,
    [deployer],
    { commitment: "confirmed" }
  );
  console.log(`  Pre-funded both with 0.01 SOL each: ${prefundSig}`);

  // Step 4: Create vault transaction
  console.log("\n[4/7] Creating vault transaction (Upgrade IX)...");

  // Safety check: verify the upgrade IX targets the correct program
  console.log(`  Target program:  ${TAX_PROGRAM_ID.toBase58()}`);
  console.log(`  ProgramData:     ${pdInfo.programData.toBase58()}`);
  console.log(`  Buffer:          ${bufferPubkey.toBase58()}`);
  console.log(`  Authority:       ${VAULT_PDA.toBase58()} (Squads vault PDA)`);
  console.log(`  Spill:           ${VAULT_PDA.toBase58()} (rent refund to vault)`);

  const upgradeIx = makeUpgradeIx(
    TAX_PROGRAM_ID,
    pdInfo.programData,
    bufferPubkey,
    VAULT_PDA, // spill address — rent goes back to vault
    VAULT_PDA // authority — vault PDA is the upgrade authority
  );

  // Manual review of encoded IX before submission
  console.log("\n  --- ENCODED IX REVIEW ---");
  console.log(`  IX program:       ${upgradeIx.programId.toBase58()}`);
  console.log(`  IX data (hex):    ${upgradeIx.data.toString("hex")}`);
  console.log(`  IX data (u32 LE): ${upgradeIx.data.readUInt32LE(0)} (expected: 3 = Upgrade)`);
  console.log(`  IX accounts[0]:   ${upgradeIx.keys[0].pubkey.toBase58()} (programData, w=${upgradeIx.keys[0].isWritable}, s=${upgradeIx.keys[0].isSigner})`);
  console.log(`  IX accounts[1]:   ${upgradeIx.keys[1].pubkey.toBase58()} (program, w=${upgradeIx.keys[1].isWritable}, s=${upgradeIx.keys[1].isSigner})`);
  console.log(`  IX accounts[2]:   ${upgradeIx.keys[2].pubkey.toBase58()} (buffer, w=${upgradeIx.keys[2].isWritable}, s=${upgradeIx.keys[2].isSigner})`);
  console.log(`  IX accounts[3]:   ${upgradeIx.keys[3].pubkey.toBase58()} (spill, w=${upgradeIx.keys[3].isWritable}, s=${upgradeIx.keys[3].isSigner})`);
  console.log(`  IX accounts[4]:   ${upgradeIx.keys[4].pubkey.toBase58()} (rent, w=${upgradeIx.keys[4].isWritable}, s=${upgradeIx.keys[4].isSigner})`);
  console.log(`  IX accounts[5]:   ${upgradeIx.keys[5].pubkey.toBase58()} (clock, w=${upgradeIx.keys[5].isWritable}, s=${upgradeIx.keys[5].isSigner})`);
  console.log(`  IX accounts[6]:   ${upgradeIx.keys[6].pubkey.toBase58()} (authority, w=${upgradeIx.keys[6].isWritable}, s=${upgradeIx.keys[6].isSigner})`);
  console.log("  --- END IX REVIEW ---\n");

  // Sanity checks before submission
  if (upgradeIx.data.readUInt32LE(0) !== 3) {
    throw new Error("ABORT: IX discriminator is not 3 (Upgrade)");
  }
  if (!upgradeIx.keys[6].pubkey.equals(VAULT_PDA)) {
    throw new Error("ABORT: Authority account is not Squads vault PDA");
  }
  if (!upgradeIx.keys[6].isSigner) {
    throw new Error("ABORT: Authority account is not marked as signer");
  }
  if (upgradeIx.keys[6].isWritable) {
    throw new Error("ABORT: Authority account should NOT be writable");
  }

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
    memo: "Upgrade Tax Program: pool identity binding fix (Phase 122.1)",
    signers: [deployer, signer1],
    sendOptions: { skipPreflight: true },
  });
  console.log(`  Vault TX created: ${vtSig}`);
  await confirmOrThrow(connection, vtSig, "vault TX create");

  // Step 5: Create proposal
  console.log("\n[5/7] Creating proposal...");
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

  // Step 6: Approve with signer 1
  console.log("\n[6/7] Approving with signer 1...");
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

  // Step 7: Print summary and next steps
  const timelockSeconds = Number(msAccount.timeLock);

  console.log("\n" + "=".repeat(60));
  console.log("  PROPOSAL CREATED -- LEDGER APPROVAL REQUIRED");
  console.log("=".repeat(60));
  console.log("");
  console.log(`  Transaction index: ${txIndex}`);
  console.log(`  Proposal PDA:      ${proposalPda.toBase58()}`);
  console.log(`  Signer 1 approved: yes`);
  console.log(`  Signer 2 needed:   Connect Ledger -> Squads UI`);
  console.log(
    `  Timelock:          ${timelockSeconds}s (${Math.round(timelockSeconds / 60)} min) after 2nd approval`
  );
  console.log("");
  console.log("  Next steps:");
  console.log("  1. Open https://app.squads.so/");
  console.log(`  2. Connect Ledger wallet (signer 2: Dw69LQtm...)`);
  console.log(`  3. Navigate to the pending proposal (TX #${txIndex})`);
  console.log("  4. Click 'Approve' -- Ledger will prompt for confirmation");
  console.log(
    `  5. Wait ${timelockSeconds}s (${Math.round(timelockSeconds / 60)} min) for timelock to expire`
  );
  console.log("  6. Execute the upgrade");
  console.log("");
  console.log("  Pre-upgrade state (for post-deploy verification):");
  console.log(`  Program:           ${TAX_PROGRAM_ID.toBase58()}`);
  console.log(`  last_deploy_slot:  ${pdInfo.lastDeploySlot}`);
  console.log(
    `  Binary sha256:     0dd98c85aff9dd88856dc15a8d3a8ee8f712df05dca62bd3388db10c68f46ad0`
  );
  console.log(`  Buffer:            ${bufferPubkey.toBase58()}`);
  console.log("");
}

main().catch((err) => {
  console.error("\nFATAL:", err.message || err);
  process.exit(1);
});
