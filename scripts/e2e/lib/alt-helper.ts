/**
 * Protocol-wide Address Lookup Table (ALT) helper.
 *
 * Creates and manages a single ALT containing all static addresses used across
 * the protocol's transaction lifecycle (epoch transitions, Carnage execution,
 * swaps, staking). This compresses transactions from 32 bytes per account
 * down to 1 byte per account, enabling large instructions like
 * execute_carnage_atomic (23+ accounts + hook remaining_accounts) to fit
 * within Solana's 1232-byte transaction limit.
 *
 * ALTs are protocol infrastructure:
 * - One ALT per network (devnet, mainnet)
 * - Reusable by any caller (permissionless Carnage bots, frontend, etc.)
 * - Persisted to disk so we don't recreate on every run
 * - No impact on CPI depth or on-chain logic
 *
 * Usage:
 *   const alt = await getOrCreateProtocolALT(connection, payer, manifest);
 *   // Then use alt with TransactionMessage.compileToV0Message([alt])
 */

import {
  AddressLookupTableAccount,
  AddressLookupTableProgram,
  Connection,
  Keypair,
  PublicKey,
  SystemProgram,
  Transaction,
  TransactionMessage,
  VersionedTransaction,
  LAMPORTS_PER_SOL,
} from "@solana/web3.js";
import {
  TOKEN_2022_PROGRAM_ID,
  TOKEN_PROGRAM_ID,
  NATIVE_MINT,
} from "@solana/spl-token";
import { AnchorProvider } from "@coral-xyz/anchor";
import * as fs from "fs";
import * as path from "path";

import { PDAManifest } from "../devnet-e2e-validation";

// ---- Constants ----

/** Path to persist the ALT address for reuse across runs */
const ALT_CACHE_PATH = path.resolve(__dirname, "../../deploy/alt-address.json");

/** Rate limit delay between RPC calls (ms) */
const RPC_DELAY_MS = 300;

function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

// ---- Core Functions ----

/**
 * Collect all static protocol addresses that should be in the ALT.
 *
 * These are addresses that appear repeatedly in protocol transactions.
 * Signers are included too -- while they must stay in the static keys
 * section of v0 messages, having them in the ALT doesn't hurt (the
 * SDK handles this automatically).
 *
 * @param manifest - PDA manifest with all deployed addresses
 * @param carnageWsolPubkey - Carnage's WSOL token account pubkey
 * @returns Deduplicated array of PublicKeys to store in the ALT
 */
export function collectProtocolAddresses(
  manifest: PDAManifest,
  carnageWsolPubkey: PublicKey
): PublicKey[] {
  const addresses = new Set<string>();

  // Programs
  addresses.add(manifest.programs.AMM);
  addresses.add(manifest.programs.TransferHook);
  addresses.add(manifest.programs.TaxProgram);
  addresses.add(manifest.programs.EpochProgram);
  addresses.add(manifest.programs.Staking);
  if (manifest.programs.ConversionVault) {
    addresses.add(manifest.programs.ConversionVault);
  }

  // Well-known programs
  addresses.add(TOKEN_PROGRAM_ID.toBase58());
  addresses.add(TOKEN_2022_PROGRAM_ID.toBase58());
  addresses.add(SystemProgram.programId.toBase58());
  addresses.add(NATIVE_MINT.toBase58()); // WSOL mint

  // Mints
  addresses.add(manifest.mints.CRIME);
  addresses.add(manifest.mints.FRAUD);
  addresses.add(manifest.mints.PROFIT);

  // PDAs -- epoch, carnage, staking
  addresses.add(manifest.pdas.EpochState);
  addresses.add(manifest.pdas.CarnageFund);
  addresses.add(manifest.pdas.CarnageSolVault);
  addresses.add(manifest.pdas.CarnageCrimeVault);
  addresses.add(manifest.pdas.CarnageFraudVault);
  addresses.add(manifest.pdas.CarnageSigner);
  addresses.add(manifest.pdas.SwapAuthority);
  addresses.add(manifest.pdas.TaxAuthority);
  addresses.add(manifest.pdas.StakingAuthority);
  addresses.add(manifest.pdas.StakePool);
  addresses.add(manifest.pdas.EscrowVault);
  addresses.add(manifest.pdas.StakeVault);
  addresses.add(manifest.pdas.AdminConfig);
  addresses.add(manifest.pdas.WhitelistAuthority);

  // WSOL intermediary for sell tax extraction
  if (manifest.pdas.WsolIntermediary) {
    addresses.add(manifest.pdas.WsolIntermediary);
  }

  // Conversion Vault PDAs
  if (manifest.pdas.VaultConfig) addresses.add(manifest.pdas.VaultConfig);
  if (manifest.pdas.VaultCrime) addresses.add(manifest.pdas.VaultCrime);
  if (manifest.pdas.VaultFraud) addresses.add(manifest.pdas.VaultFraud);
  if (manifest.pdas.VaultProfit) addresses.add(manifest.pdas.VaultProfit);

  // Bonding Curve program + PDAs (CurveState, token vault, SOL vault, tax escrow x2 mints)
  if (manifest.programs.BondingCurve) {
    addresses.add(manifest.programs.BondingCurve);
  }
  for (const key of [
    "CurveState_CRIME", "CurveState_FRAUD",
    "CurveTokenVault_CRIME", "CurveTokenVault_FRAUD",
    "CurveSolVault_CRIME", "CurveSolVault_FRAUD",
    "CurveTaxEscrow_CRIME", "CurveTaxEscrow_FRAUD",
  ]) {
    if (manifest.pdas[key]) addresses.add(manifest.pdas[key]);
  }

  // Carnage WSOL account
  addresses.add(carnageWsolPubkey.toBase58());

  // ExtraAccountMetaLists (Transfer Hook PDAs for each mint)
  addresses.add(manifest.pdas.ExtraAccountMetaList_CRIME);
  addresses.add(manifest.pdas.ExtraAccountMetaList_FRAUD);
  addresses.add(manifest.pdas.ExtraAccountMetaList_PROFIT);

  // Pool accounts (CRIME/SOL and FRAUD/SOL -- used by Carnage)
  for (const poolName of ["CRIME/SOL", "FRAUD/SOL"]) {
    const pool = manifest.pools[poolName];
    if (!pool) continue;
    addresses.add(pool.pool);
    addresses.add(pool.vaultA);
    addresses.add(pool.vaultB);
  }

  // Whitelist PDAs for Transfer Hook remaining_accounts.
  // Seeds: ["whitelist", token_account_pubkey] on the hook program.
  // Pre-compute for all token accounts involved in Carnage swaps:
  // sell direction source/dest + buy direction source/dest.
  const hookProgramId = new PublicKey(manifest.programs.TransferHook);
  const whitelistAccounts = [
    manifest.pdas.CarnageCrimeVault,    // carnage CRIME vault (sell source / buy dest)
    manifest.pdas.CarnageFraudVault,    // carnage FRAUD vault (sell source / buy dest)
    manifest.pools["CRIME/SOL"]?.vaultB, // CRIME pool token vault (sell dest / buy source)
    manifest.pools["FRAUD/SOL"]?.vaultB, // FRAUD pool token vault (sell dest / buy source)
  ].filter(Boolean) as string[];

  for (const tokenAccount of whitelistAccounts) {
    const [whitelistPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("whitelist"), new PublicKey(tokenAccount).toBytes()],
      hookProgramId
    );
    addresses.add(whitelistPda.toBase58());
  }

  // Convert to PublicKey array
  return Array.from(addresses).map((addr) => new PublicKey(addr));
}

/**
 * Get or create the protocol-wide Address Lookup Table.
 *
 * 1. If ALT address is cached on disk and exists on-chain, load and return it
 * 2. Otherwise, create a new ALT, extend it with all protocol addresses,
 *    wait for activation, and cache the address
 *
 * @param provider - Anchor provider (has wallet + connection)
 * @param manifest - PDA manifest with all deployed addresses
 * @returns The AddressLookupTableAccount ready for use in v0 transactions
 */
export async function getOrCreateProtocolALT(
  provider: AnchorProvider,
  manifest: PDAManifest,
  carnageWsolPubkeyOverride?: PublicKey
): Promise<AddressLookupTableAccount> {
  const connection = provider.connection;

  // Load carnage-wsol pubkey: prefer override (Railway env var), fall back to file
  let carnageWsolPubkey: PublicKey;
  if (carnageWsolPubkeyOverride) {
    carnageWsolPubkey = carnageWsolPubkeyOverride;
  } else {
    const carnageWsolKeypairPath = path.resolve(
      __dirname,
      "../../../keypairs/carnage-wsol.json"
    );
    const carnageWsolSecretKey = JSON.parse(
      fs.readFileSync(carnageWsolKeypairPath, "utf-8")
    );
    carnageWsolPubkey = Keypair.fromSecretKey(
      Uint8Array.from(carnageWsolSecretKey)
    ).publicKey;
  }

  const allAddresses = collectProtocolAddresses(manifest, carnageWsolPubkey);
  console.log(`  ALT: ${allAddresses.length} protocol addresses collected`);

  // Priority 1: ALT_ADDRESS env var (Railway / CI — avoids creating new ALTs on every deploy)
  const altAddressEnv = process.env.ALT_ADDRESS;
  if (altAddressEnv) {
    try {
      const altAddress = new PublicKey(altAddressEnv);
      const altAccount = await connection.getAddressLookupTable(altAddress);
      await sleep(RPC_DELAY_MS);

      if (altAccount.value) {
        console.log(
          `  ALT: Loaded from ALT_ADDRESS env var ${altAddress.toBase58().slice(0, 12)}... (${altAccount.value.state.addresses.length} addresses)`
        );
        return altAccount.value;
      }
      console.log(
        `  ALT: ALT_ADDRESS env var set but account not found on-chain. Falling through...`
      );
    } catch (err) {
      console.log(
        `  ALT: ALT_ADDRESS env var invalid: ${String(err).slice(0, 100)}. Falling through...`
      );
    }
  }

  // Priority 2: Cached ALT address on disk
  if (fs.existsSync(ALT_CACHE_PATH)) {
    try {
      const cached = JSON.parse(fs.readFileSync(ALT_CACHE_PATH, "utf-8"));
      const altAddress = new PublicKey(cached.altAddress);

      const altAccount = await connection.getAddressLookupTable(altAddress);
      await sleep(RPC_DELAY_MS);

      if (altAccount.value) {
        // Verify the ALT has all needed addresses
        const altKeys = new Set(
          altAccount.value.state.addresses.map((a) => a.toBase58())
        );
        const missing = allAddresses.filter(
          (addr) => !altKeys.has(addr.toBase58())
        );

        if (missing.length === 0) {
          console.log(
            `  ALT: Loaded existing ALT ${altAddress.toBase58().slice(0, 12)}... (${altAccount.value.state.addresses.length} addresses)`
          );
          return altAccount.value;
        }

        // Extend with missing addresses
        console.log(
          `  ALT: Extending existing ALT with ${missing.length} missing addresses`
        );
        await extendALT(provider, altAddress, missing);
        await sleep(RPC_DELAY_MS);

        // Re-fetch after extension
        const updated = await connection.getAddressLookupTable(altAddress);
        await sleep(RPC_DELAY_MS);
        if (updated.value) {
          console.log(
            `  ALT: Extended to ${updated.value.state.addresses.length} addresses`
          );
          return updated.value;
        }
      }
    } catch (err) {
      console.log(
        `  ALT: Cached ALT invalid, creating new one: ${String(err).slice(0, 100)}`
      );
    }
  }

  // Create new ALT
  console.log("  ALT: Creating new Address Lookup Table...");
  const altAddress = await createALT(provider);
  await sleep(RPC_DELAY_MS);

  // Extend with all addresses (max 30 per extend call)
  const BATCH_SIZE = 30;
  for (let i = 0; i < allAddresses.length; i += BATCH_SIZE) {
    const batch = allAddresses.slice(i, i + BATCH_SIZE);
    console.log(
      `  ALT: Extending batch ${Math.floor(i / BATCH_SIZE) + 1}/${Math.ceil(allAddresses.length / BATCH_SIZE)} (${batch.length} addresses)`
    );
    await extendALT(provider, altAddress, batch);
    await sleep(RPC_DELAY_MS);
  }

  // Wait for ALT activation (needs 1 slot after last write)
  console.log("  ALT: Waiting for activation (1 slot)...");
  await waitForALTActivation(connection, altAddress);

  // Cache the ALT address
  fs.writeFileSync(
    ALT_CACHE_PATH,
    JSON.stringify({
      altAddress: altAddress.toBase58(),
      createdAt: new Date().toISOString(),
      addressCount: allAddresses.length,
      network: "devnet",
    }),
    "utf-8"
  );
  console.log(
    `  ALT: Created and cached ${altAddress.toBase58().slice(0, 12)}... (${allAddresses.length} addresses)`
  );

  // Fetch and return the active ALT
  const altAccount = await connection.getAddressLookupTable(altAddress);
  if (!altAccount.value) {
    throw new Error("ALT created but not fetchable after activation wait");
  }
  return altAccount.value;
}

/**
 * Create a new Address Lookup Table.
 *
 * @returns The ALT address
 */
async function createALT(provider: AnchorProvider): Promise<PublicKey> {
  // Use "finalized" for getSlot to ensure the slot is recognized by all validators.
  // Skip preflight simulation to avoid stale-slot race conditions on mainnet
  // (simulation can see a different slot than what we queried).
  const slot = await provider.connection.getSlot("finalized");

  const [createIx, altAddress] = AddressLookupTableProgram.createLookupTable({
    authority: provider.wallet.publicKey,
    payer: provider.wallet.publicKey,
    recentSlot: slot,
  });

  const tx = new Transaction().add(createIx);
  tx.recentBlockhash = (await provider.connection.getLatestBlockhash("finalized")).blockhash;
  tx.feePayer = provider.wallet.publicKey;
  const signed = await provider.wallet.signTransaction(tx);
  const sig = await provider.connection.sendRawTransaction(signed.serialize(), {
    skipPreflight: true,
  });
  await provider.connection.confirmTransaction(sig, "finalized");

  console.log(`  ALT: Created at ${altAddress.toBase58()}`);
  return altAddress;
}

/**
 * Extend an ALT with additional addresses.
 *
 * @param provider - Anchor provider
 * @param altAddress - The ALT to extend
 * @param addresses - Addresses to add (max 30 per call)
 */
async function extendALT(
  provider: AnchorProvider,
  altAddress: PublicKey,
  addresses: PublicKey[]
): Promise<void> {
  const extendIx = AddressLookupTableProgram.extendLookupTable({
    lookupTable: altAddress,
    authority: provider.wallet.publicKey,
    payer: provider.wallet.publicKey,
    addresses,
  });

  const tx = new Transaction().add(extendIx);
  await provider.sendAndConfirm(tx);
}

/**
 * Wait for an ALT to become active (needs 1+ slot after creation/extension).
 *
 * Polls every 500ms for up to 30 seconds.
 */
async function waitForALTActivation(
  connection: Connection,
  altAddress: PublicKey
): Promise<void> {
  const maxWaitMs = 30_000;
  const pollIntervalMs = 500;
  const startMs = Date.now();

  while (Date.now() - startMs < maxWaitMs) {
    const altAccount = await connection.getAddressLookupTable(altAddress);
    if (altAccount.value && altAccount.value.isActive()) {
      return;
    }
    await sleep(pollIntervalMs);
  }

  throw new Error(
    `ALT ${altAddress.toBase58()} did not activate within ${maxWaitMs / 1000}s`
  );
}

/**
 * Build and send a VersionedTransaction (v0) using the protocol ALT.
 *
 * This is the core helper that replaces legacy Transaction usage.
 * It compiles instructions into a v0 message referencing the ALT,
 * signs with the provided signers, and sends.
 *
 * @param connection - Solana connection
 * @param payer - Transaction fee payer
 * @param instructions - Instructions to include
 * @param signers - Keypairs that need to sign (payer + any additional)
 * @param alt - The Address Lookup Table account
 * @returns Transaction signature
 */
export async function sendV0Transaction(
  connection: Connection,
  payer: PublicKey,
  instructions: import("@solana/web3.js").TransactionInstruction[],
  signers: Keypair[],
  alt: AddressLookupTableAccount
): Promise<string> {
  const { blockhash, lastValidBlockHeight } =
    await connection.getLatestBlockhash("confirmed");

  const messageV0 = new TransactionMessage({
    payerKey: payer,
    recentBlockhash: blockhash,
    instructions,
  }).compileToV0Message([alt]);

  const vtx = new VersionedTransaction(messageV0);
  vtx.sign(signers);

  const txSig = await connection.sendTransaction(vtx, {
    skipPreflight: true,
    maxRetries: 3,
  });

  const confirmation = await connection.confirmTransaction(
    { signature: txSig, blockhash, lastValidBlockHeight },
    "confirmed"
  );

  // With skipPreflight, failed TXs are still "confirmed" on Solana.
  // We must check confirmation.value.err to detect on-chain failures.
  if (confirmation.value.err) {
    // Fetch TX logs for debugging
    let logs = "";
    try {
      const txInfo = await connection.getTransaction(txSig, {
        maxSupportedTransactionVersion: 0,
        commitment: "confirmed",
      });
      logs = txInfo?.meta?.logMessages?.join("\n") || "(no logs)";
    } catch {
      logs = "(failed to fetch logs)";
    }
    const err = new Error(
      `Transaction confirmed but failed on-chain: ${JSON.stringify(confirmation.value.err)}\nTX: ${txSig}\nLogs:\n${logs}`
    );
    (err as any).txSig = txSig;
    throw err;
  }

  return txSig;
}
