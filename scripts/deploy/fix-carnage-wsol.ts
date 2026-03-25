/**
 * Fix Carnage WSOL Account — One-Time Repair Script
 *
 * Root cause: Phase 95 clean deploy changed the Epoch Program ID, which changed
 * the CarnageSigner PDA. The old Carnage WSOL token account's owner field still
 * points to the old CarnageSigner PDA. The on-chain constraint
 * `carnage_wsol.owner == carnage_signer.key()` correctly rejects this.
 *
 * This script:
 *   1. Derives the current CarnageSigner PDA from the current Epoch Program ID
 *   2. Generates a new WSOL keypair (saves to keypairs/carnage-wsol.json)
 *   3. Creates a new WSOL token account with the correct CarnageSigner PDA as owner
 *   4. Extends the protocol ALT with the new address
 *   5. Prints the new CARNAGE_WSOL_PUBKEY to update on Railway
 *
 * Usage:
 *   source "$HOME/.cargo/env" && export PATH="$HOME/.local/share/solana/install/active_release/bin:$PATH"
 *   set -a && source .env && set +a
 *   npx tsx scripts/deploy/fix-carnage-wsol.ts
 */

import {
  Keypair,
  PublicKey,
  Connection,
} from "@solana/web3.js";
import {
  TOKEN_PROGRAM_ID,
  NATIVE_MINT,
  createWrappedNativeAccount,
  getAccount,
} from "@solana/spl-token";
import * as fs from "fs";
import * as path from "path";
import { loadProvider } from "./lib/connection";

async function main() {
  console.log("=== Fix Carnage WSOL Account ===\n");

  const provider = loadProvider();
  const connection = provider.connection;
  const authority = (provider.wallet as any).payer as Keypair;

  // Load manifest to get Epoch Program ID
  const manifestPath = path.resolve(__dirname, "pda-manifest.json");
  const manifest = JSON.parse(fs.readFileSync(manifestPath, "utf-8"));
  const epochProgramId = new PublicKey(manifest.programs.EpochProgram);

  console.log(`Epoch Program ID: ${epochProgramId.toBase58()}`);

  // Derive current CarnageSigner PDA
  const [carnageSignerPda] = PublicKey.findProgramAddressSync(
    [Buffer.from("carnage_signer")],
    epochProgramId
  );
  console.log(`CarnageSigner PDA: ${carnageSignerPda.toBase58()}`);

  // Check if the manifest's CarnageSigner matches
  if (manifest.pdas.CarnageSigner !== carnageSignerPda.toBase58()) {
    console.error(
      `MISMATCH: Manifest says CarnageSigner = ${manifest.pdas.CarnageSigner}\n` +
      `  but derived PDA = ${carnageSignerPda.toBase58()}`
    );
    process.exit(1);
  }

  // Check existing WSOL account (if any)
  const carnageWsolKeypairPath = path.resolve(__dirname, "../../keypairs/carnage-wsol.json");
  if (fs.existsSync(carnageWsolKeypairPath)) {
    const oldSecretKey = JSON.parse(fs.readFileSync(carnageWsolKeypairPath, "utf-8"));
    const oldKeypair = Keypair.fromSecretKey(new Uint8Array(oldSecretKey));
    console.log(`\nExisting WSOL keypair: ${oldKeypair.publicKey.toBase58()}`);

    try {
      const oldAccount = await getAccount(
        connection,
        oldKeypair.publicKey,
        "confirmed",
        TOKEN_PROGRAM_ID
      );
      console.log(`  Token owner: ${oldAccount.owner.toBase58()}`);
      console.log(`  Expected:    ${carnageSignerPda.toBase58()}`);

      if (oldAccount.owner.equals(carnageSignerPda)) {
        console.log("\n  Owner already matches! No fix needed.");
        console.log(`  CARNAGE_WSOL_PUBKEY=${oldKeypair.publicKey.toBase58()}`);
        return;
      }
      console.log("  MISMATCH -- creating new account.");
    } catch (e) {
      console.log(`  Could not decode existing account: ${e}`);
    }

    // Backup old keypair
    const backupPath = carnageWsolKeypairPath + ".bak";
    fs.copyFileSync(carnageWsolKeypairPath, backupPath);
    console.log(`  Backed up old keypair to ${backupPath}`);
  }

  // Generate new keypair
  const newKeypair = Keypair.generate();
  fs.writeFileSync(
    carnageWsolKeypairPath,
    JSON.stringify(Array.from(newKeypair.secretKey)),
    { mode: 0o600 }
  );
  console.log(`\nNew WSOL keypair: ${newKeypair.publicKey.toBase58()}`);

  // Create the WSOL account with CarnageSigner PDA as owner
  console.log("Creating WSOL account with correct owner...");
  const newWsolAccount = await createWrappedNativeAccount(
    connection,
    authority,                 // payer
    carnageSignerPda,          // owner (CarnageSigner PDA)
    0,                         // 0 lamports initial
    newKeypair,                // explicit keypair
    { commitment: "confirmed" },
    TOKEN_PROGRAM_ID           // WSOL uses SPL Token, not Token-2022
  );
  console.log(`Created: ${newWsolAccount.toBase58()}`);

  // Verify the new account
  const verifyAccount = await getAccount(
    connection,
    newWsolAccount,
    "confirmed",
    TOKEN_PROGRAM_ID
  );
  console.log(`\nVerification:`);
  console.log(`  Address: ${newWsolAccount.toBase58()}`);
  console.log(`  Token owner: ${verifyAccount.owner.toBase58()}`);
  console.log(`  Expected:    ${carnageSignerPda.toBase58()}`);
  console.log(`  Mint: ${verifyAccount.mint.toBase58()}`);
  console.log(`  Match: ${verifyAccount.owner.equals(carnageSignerPda)}`);

  if (!verifyAccount.owner.equals(carnageSignerPda)) {
    console.error("\nFATAL: Owner still doesn't match after creation!");
    process.exit(1);
  }

  console.log("\n=== SUCCESS ===");
  console.log(`\nUpdate Railway env var:`);
  console.log(`  CARNAGE_WSOL_PUBKEY=${newWsolAccount.toBase58()}`);
  console.log(`\nAlso update ALT to include the new address.`);
  console.log(`Run: npx tsx scripts/deploy/create-alt.ts`);
}

main().catch((err) => {
  console.error(`FATAL: ${err}`);
  process.exit(1);
});
