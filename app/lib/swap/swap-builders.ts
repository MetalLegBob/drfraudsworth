/**
 * Swap Transaction Builders
 *
 * Builds complete Transaction objects for all swap types:
 * - SOL buy (SOL -> CRIME/FRAUD)          -- 20 named accounts + 4 hook, taxed
 * - SOL sell (CRIME/FRAUD -> SOL)         -- 21 named accounts + 4 hook, taxed
 * - Vault convert (CRIME/FRAUD <-> PROFIT) -- 9 named accounts + 8 hook, deterministic
 *
 * Each builder:
 * 1. Creates any necessary ATAs (WSOL or Token-2022)
 * 2. Resolves Transfer Hook remaining_accounts (all Token-2022 transfers)
 * 3. Resolves Transfer Hook remaining_accounts
 * 4. Assembles everything into a single Transaction with compute budget
 *
 * Source mapping:
 * - SOL buy struct: programs/tax-program/src/instructions/swap_sol_buy.rs
 * - SOL sell struct: programs/tax-program/src/instructions/swap_sol_sell.rs
 * - Vault convert: programs/conversion-vault/src/lib.rs
 * - E2E reference: scripts/e2e/lib/swap-flow.ts
 * - Hook resolution: app/lib/swap/hook-resolver.ts
 * - WSOL helpers: app/lib/swap/wsol.ts
 */

import {
  Connection,
  PublicKey,
  SystemProgram,
  Transaction,
  VersionedTransaction,
  TransactionMessage,
  ComputeBudgetProgram,
} from "@solana/web3.js";
import { fetchProtocolALT } from "./multi-hop-builder";
import {
  TOKEN_PROGRAM_ID,
  TOKEN_2022_PROGRAM_ID,
  NATIVE_MINT,
  getAssociatedTokenAddress,
  createAssociatedTokenAccountInstruction,
} from "@solana/spl-token";
import { BN } from "@coral-xyz/anchor";

import { getTaxProgram, getVaultProgram } from "@/lib/anchor";
import { buildWsolWrapInstructions, buildWsolUnwrapInstruction, getWsolAta } from "./wsol";
import { resolveHookAccounts } from "./hook-resolver";
import { VAULT_SEEDS } from "@dr-fraudsworth/shared";
import {
  MINTS,
  PROGRAM_IDS,
  DEVNET_POOL_CONFIGS,
  DEVNET_PDAS_EXTENDED,
  TREASURY_PUBKEY,
} from "@/lib/protocol-config";

// =============================================================================
// Parameter Interfaces
// =============================================================================

/** Parameters for building a SOL -> CRIME/FRAUD buy transaction */
export interface SolBuyParams {
  /** Solana RPC connection */
  connection: Connection;
  /** User's wallet public key (signer) */
  userPublicKey: PublicKey;
  /** SOL amount to spend in lamports (including tax) */
  amountInLamports: number;
  /** Minimum token output after slippage */
  minimumOutput: number;
  /** true = CRIME pool, false = FRAUD pool */
  isCrime: boolean;
  /** Buy tax rate in basis points (from EpochState). Used to wrap only
   *  the post-tax portion as WSOL, preventing wallet simulation failure
   *  when the user spends near their full SOL balance. */
  taxBps: number;
  /** Compute unit limit (default 200,000) */
  computeUnits?: number;
  /** Priority fee in microLamports per compute unit (default 0) */
  priorityFeeMicroLamports?: number;
}

/** Parameters for building a CRIME/FRAUD -> SOL sell transaction */
export interface SolSellParams {
  /** Solana RPC connection */
  connection: Connection;
  /** User's wallet public key (signer) */
  userPublicKey: PublicKey;
  /** Token amount to sell in base units (6 decimals) */
  amountInBaseUnits: number;
  /** Minimum SOL to receive AFTER tax in lamports */
  minimumOutput: number;
  /** true = CRIME pool, false = FRAUD pool */
  isCrime: boolean;
  /** Compute unit limit (default 200,000) */
  computeUnits?: number;
  /** Priority fee in microLamports per compute unit (default 0) */
  priorityFeeMicroLamports?: number;
}

/** Parameters for building a vault conversion transaction (CRIME/FRAUD <-> PROFIT) */
export interface VaultConvertParams {
  /** Solana RPC connection */
  connection: Connection;
  /** User's wallet public key (signer) */
  userPublicKey: PublicKey;
  /** Input amount in base units (6 decimals) */
  amountInBaseUnits: number;
  /** Minimum output tokens to receive (slippage protection) */
  minimumOutput: number;
  /** Input mint (CRIME, FRAUD, or PROFIT) */
  inputMint: PublicKey;
  /** Output mint (PROFIT, CRIME, or FRAUD) */
  outputMint: PublicKey;
  /**
   * User's input token balance before the preceding swap step (default 0).
   * When > 0 and amountInBaseUnits == 0: delta mode — vault converts only
   * the tokens deposited since this balance (current_balance - preBalance).
   * When 0: convert-all mode (if amountInBaseUnits == 0) or ignored (if > 0).
   */
  preBalance?: number;
  /** Compute unit limit (default 200,000) */
  computeUnits?: number;
  /** Priority fee in microLamports per compute unit (default 0) */
  priorityFeeMicroLamports?: number;
}

// =============================================================================
// Helpers
// =============================================================================

/** Default compute unit limit for swap transactions */
const DEFAULT_COMPUTE_UNITS = 200_000;

/**
 * Get the Token-2022 ATA for a given owner and mint.
 * CRIME/FRAUD/PROFIT all use TOKEN_2022_PROGRAM_ID.
 */
async function getToken2022Ata(owner: PublicKey, mint: PublicKey): Promise<PublicKey> {
  return getAssociatedTokenAddress(
    mint,
    owner,
    false, // allowOwnerOffCurve = false
    TOKEN_2022_PROGRAM_ID,
  );
}

/**
 * Resolve pool config (vault addresses, mint) based on isCrime flag.
 */
function getPoolConfig(isCrime: boolean) {
  return isCrime ? DEVNET_POOL_CONFIGS.CRIME_SOL : DEVNET_POOL_CONFIGS.FRAUD_SOL;
}

/**
 * Get the token mint based on isCrime flag.
 */
function getTokenMint(isCrime: boolean): PublicKey {
  return isCrime ? MINTS.CRIME : MINTS.FRAUD;
}

/**
 * Derive vault token account PDA for a given mint seed.
 * Seeds: [mintSeed, vaultConfigPda]
 */
function deriveVaultTokenAccount(mintSeed: Buffer): PublicKey {
  const [vaultConfig] = PublicKey.findProgramAddressSync(
    [VAULT_SEEDS.CONFIG],
    PROGRAM_IDS.VAULT,
  );
  const [vaultAccount] = PublicKey.findProgramAddressSync(
    [mintSeed, vaultConfig.toBuffer()],
    PROGRAM_IDS.VAULT,
  );
  return vaultAccount;
}

/**
 * Get the vault seed for a given mint.
 */
function getVaultSeedForMint(mint: PublicKey): Buffer {
  if (mint.equals(MINTS.CRIME)) return VAULT_SEEDS.VAULT_CRIME;
  if (mint.equals(MINTS.FRAUD)) return VAULT_SEEDS.VAULT_FRAUD;
  if (mint.equals(MINTS.PROFIT)) return VAULT_SEEDS.VAULT_PROFIT;
  throw new Error(`Unknown mint for vault: ${mint.toBase58()}`);
}

// =============================================================================
// SOL Buy Transaction Builder
// =============================================================================

/**
 * Build a SOL -> CRIME/FRAUD swap transaction.
 *
 * Transaction structure:
 * 1. ComputeBudgetProgram.setComputeUnitLimit (always)
 * 2. ComputeBudgetProgram.setComputeUnitPrice (if priority > 0)
 * 3. WSOL wrap instructions (create ATA if needed + transfer + syncNative)
 * 4. Create output token ATA if needed (Token-2022)
 * 5. Tax Program swap_sol_buy instruction with 4 hook remaining_accounts
 *
 * Named accounts (20 total, matching SwapSolBuy struct):
 * user, epochState, swapAuthority, taxAuthority, pool, poolVaultA, poolVaultB,
 * mintA (NATIVE_MINT), mintB (token), userTokenA (WSOL ATA), userTokenB (token ATA),
 * stakePool, stakingEscrow, carnageVault, treasury,
 * ammProgram, tokenProgramA, tokenProgramB, systemProgram, stakingProgram
 *
 * @param params - SOL buy parameters
 * @returns Complete unsigned Transaction ready for signing
 */
export async function buildSolBuyTransaction(params: SolBuyParams): Promise<Transaction> {
  const {
    connection,
    userPublicKey,
    amountInLamports,
    minimumOutput,
    isCrime,
    taxBps,
    computeUnits = DEFAULT_COMPUTE_UNITS,
    priorityFeeMicroLamports = 0,
  } = params;

  const poolConfig = getPoolConfig(isCrime);
  const tokenMint = getTokenMint(isCrime);
  const tx = new Transaction();

  // 1. Compute budget (always set limit; only set price if priority > 0)
  tx.add(ComputeBudgetProgram.setComputeUnitLimit({ units: computeUnits }));
  if (priorityFeeMicroLamports > 0) {
    tx.add(ComputeBudgetProgram.setComputeUnitPrice({ microLamports: priorityFeeMicroLamports }));
  }

  // 2. WSOL wrap instructions (create ATA if needed + SOL transfer + syncNative)
  //    Wrap only the post-tax portion as WSOL. On-chain, the program transfers
  //    tax as native SOL via system_instruction::transfer, then sends sol_to_swap
  //    WSOL to the AMM. Wrapping the full amountInLamports causes a mid-TX
  //    double-dip: the user needs amountIn + taxAmount peak SOL, causing wallet
  //    simulation failure when spending near full balance (MAX button).
  //    Formula matches on-chain calculate_tax: floor(amount * bps / 10_000).
  const taxAmountLamports = Math.floor(amountInLamports * taxBps / 10_000);
  const solToSwapLamports = amountInLamports - taxAmountLamports;
  const wsolInstructions = await buildWsolWrapInstructions(
    connection,
    userPublicKey,
    solToSwapLamports,
  );
  for (const ix of wsolInstructions) {
    tx.add(ix);
  }

  // 3. Check if user's output token ATA exists; create if needed
  //    CRIME/FRAUD are Token-2022 mints, so use TOKEN_2022_PROGRAM_ID
  const userTokenB = await getToken2022Ata(userPublicKey, tokenMint);
  const userTokenBInfo = await connection.getAccountInfo(userTokenB);
  if (!userTokenBInfo) {
    tx.add(
      createAssociatedTokenAccountInstruction(
        userPublicKey, // payer
        userTokenB,    // ATA address
        userPublicKey, // owner
        tokenMint,     // mint
        TOKEN_2022_PROGRAM_ID,
      ),
    );
  }

  // 4. Resolve Transfer Hook accounts
  //    For buy: AMM transfers tokens FROM pool vaultB TO user (output is Token-2022)
  //    Deterministic PDA derivation -- no RPC needed
  const hookAccounts = resolveHookAccounts(
    poolConfig.vaultB, // source: pool vault sends tokens
    tokenMint,
    userTokenB,        // dest: user receives tokens
  );

  // 5. Build the Tax Program swap_sol_buy instruction
  const userWsolAta = await getWsolAta(userPublicKey);
  const taxProgram = getTaxProgram(connection);

  const swapIx = await taxProgram.methods
    .swapSolBuy(
      new BN(amountInLamports),
      new BN(minimumOutput),
      isCrime,
    )
    .accountsStrict({
      user: userPublicKey,
      epochState: DEVNET_PDAS_EXTENDED.EpochState,
      swapAuthority: DEVNET_PDAS_EXTENDED.SwapAuthority,
      taxAuthority: DEVNET_PDAS_EXTENDED.TaxAuthority,
      pool: poolConfig.pool,
      poolVaultA: poolConfig.vaultA,
      poolVaultB: poolConfig.vaultB,
      mintA: NATIVE_MINT,
      mintB: tokenMint,
      userTokenA: userWsolAta,
      userTokenB: userTokenB,
      stakePool: DEVNET_PDAS_EXTENDED.StakePool,
      stakingEscrow: DEVNET_PDAS_EXTENDED.EscrowVault,
      carnageVault: DEVNET_PDAS_EXTENDED.CarnageSolVault,
      treasury: TREASURY_PUBKEY,
      ammProgram: PROGRAM_IDS.AMM,
      tokenProgramA: TOKEN_PROGRAM_ID,
      tokenProgramB: TOKEN_2022_PROGRAM_ID,
      systemProgram: SystemProgram.programId,
      stakingProgram: PROGRAM_IDS.STAKING,
    })
    .remainingAccounts(hookAccounts)
    .instruction();

  tx.add(swapIx);

  // 6. Append WSOL unwrap instruction (close WSOL ATA, return leftover SOL to user)
  //    After the buy swap, the user's WSOL ATA may retain a small balance from
  //    rounding or slippage. Closing the account unwraps all remaining WSOL back
  //    to native SOL and recovers the rent-exempt lamports.
  const unwrapIx = await buildWsolUnwrapInstruction(userPublicKey);
  tx.add(unwrapIx);

  return tx;
}

// =============================================================================
// SOL Sell Transaction Builder
// =============================================================================

/**
 * Build a CRIME/FRAUD -> SOL sell swap transaction.
 *
 * Transaction structure:
 * 1. ComputeBudgetProgram.setComputeUnitLimit (always)
 * 2. ComputeBudgetProgram.setComputeUnitPrice (if priority > 0)
 * 3. Create WSOL ATA if needed (receives SOL output)
 * 4. Tax Program swap_sol_sell instruction with 4 hook remaining_accounts
 * 5. WSOL unwrap instruction (close WSOL ATA, recover SOL)
 *
 * Named accounts (21 total, matching SwapSolSell struct):
 * Same as SwapSolBuy + wsolIntermediary (PDA for sell tax extraction).
 *
 * CRITICAL: Hook resolution direction is different for sell.
 * - Buy: AMM transfers tokens FROM poolVaultB TO user (output = Token-2022)
 * - Sell: AMM transfers tokens FROM user TO poolVaultB (input = Token-2022)
 * Hook source/dest must match the transfer direction.
 *
 * @param params - SOL sell parameters
 * @returns Complete unsigned Transaction ready for signing
 */
export async function buildSolSellTransaction(params: SolSellParams): Promise<Transaction> {
  const {
    connection,
    userPublicKey,
    amountInBaseUnits,
    minimumOutput,
    isCrime,
    computeUnits = 250_000,  // Sell needs more CU for transfer-close-distribute-reinit
    priorityFeeMicroLamports = 0,
  } = params;

  const poolConfig = getPoolConfig(isCrime);
  const tokenMint = getTokenMint(isCrime);
  const tx = new Transaction();

  // 1. Compute budget
  tx.add(ComputeBudgetProgram.setComputeUnitLimit({ units: computeUnits }));
  if (priorityFeeMicroLamports > 0) {
    tx.add(ComputeBudgetProgram.setComputeUnitPrice({ microLamports: priorityFeeMicroLamports }));
  }

  // 2. Check if user's WSOL ATA exists; create if needed (receives SOL output)
  const userWsolAta = await getWsolAta(userPublicKey);
  const wsolAtaInfo = await connection.getAccountInfo(userWsolAta);
  if (!wsolAtaInfo) {
    tx.add(
      createAssociatedTokenAccountInstruction(
        userPublicKey, // payer
        userWsolAta,   // ATA address
        userPublicKey, // owner
        NATIVE_MINT,   // mint
        TOKEN_PROGRAM_ID, // WSOL uses original Token Program
      ),
    );
  }

  // 3. Resolve Transfer Hook accounts
  //    For sell: AMM transfers tokens FROM user TO poolVaultB (input = Token-2022)
  //    Deterministic PDA derivation -- no RPC needed
  const userTokenB = await getToken2022Ata(userPublicKey, tokenMint);
  const hookAccounts = resolveHookAccounts(
    userTokenB,        // source: user sends tokens
    tokenMint,
    poolConfig.vaultB, // dest: pool vault receives tokens
  );

  // 4. Build the Tax Program swap_sol_sell instruction
  const taxProgram = getTaxProgram(connection);

  const swapIx = await taxProgram.methods
    .swapSolSell(
      new BN(amountInBaseUnits),
      new BN(minimumOutput),
      isCrime,
    )
    .accountsStrict({
      user: userPublicKey,
      epochState: DEVNET_PDAS_EXTENDED.EpochState,
      swapAuthority: DEVNET_PDAS_EXTENDED.SwapAuthority,
      taxAuthority: DEVNET_PDAS_EXTENDED.TaxAuthority,
      pool: poolConfig.pool,
      poolVaultA: poolConfig.vaultA,
      poolVaultB: poolConfig.vaultB,
      mintA: NATIVE_MINT,
      mintB: tokenMint,
      userTokenA: userWsolAta,
      userTokenB: userTokenB,
      stakePool: DEVNET_PDAS_EXTENDED.StakePool,
      stakingEscrow: DEVNET_PDAS_EXTENDED.EscrowVault,
      carnageVault: DEVNET_PDAS_EXTENDED.CarnageSolVault,
      treasury: TREASURY_PUBKEY,
      ammProgram: PROGRAM_IDS.AMM,
      tokenProgramA: TOKEN_PROGRAM_ID,
      tokenProgramB: TOKEN_2022_PROGRAM_ID,
      systemProgram: SystemProgram.programId,
      stakingProgram: PROGRAM_IDS.STAKING,
      wsolIntermediary: DEVNET_PDAS_EXTENDED.WsolIntermediary,
    })
    .remainingAccounts(hookAccounts)
    .instruction();

  tx.add(swapIx);

  // 5. Append WSOL unwrap instruction (close WSOL ATA, return SOL to user)
  const unwrapIx = await buildWsolUnwrapInstruction(userPublicKey);
  tx.add(unwrapIx);

  return tx;
}

// =============================================================================
// v0 Wrapper — Converts legacy Transaction to VersionedTransaction with ALT
// =============================================================================

/**
 * Compile a legacy Transaction to a v0 VersionedTransaction using the protocol ALT.
 *
 * Phantom's simulator can't handle legacy transactions with 24+ accounts (taxed swaps).
 * v0 + ALT compresses the account list and allows Phantom to simulate correctly,
 * avoiding the "This dApp could be malicious" red warning.
 *
 * Used by the single-hop swap path in useSwap.ts. Multi-hop already builds v0 natively.
 */
export async function compileToVersionedTransaction(
  tx: Transaction,
  connection: Connection,
  payerKey: PublicKey,
): Promise<{ transaction: VersionedTransaction; lastValidBlockHeight: number }> {
  const [alt, { blockhash, lastValidBlockHeight }] = await Promise.all([
    fetchProtocolALT(connection),
    connection.getLatestBlockhash("confirmed"),
  ]);

  const messageV0 = new TransactionMessage({
    payerKey,
    recentBlockhash: blockhash,
    instructions: tx.instructions,
  }).compileToV0Message([alt]);

  return {
    transaction: new VersionedTransaction(messageV0),
    lastValidBlockHeight,
  };
}

// =============================================================================
// Vault Convert Transaction Builder
// =============================================================================

/**
 * Build a vault conversion transaction (CRIME/FRAUD <-> PROFIT).
 *
 * Fixed-rate 100:1 conversion. No tax, no slippage, no AMM.
 *
 * Transaction structure:
 * 1. ComputeBudgetProgram.setComputeUnitLimit (always)
 * 2. ComputeBudgetProgram.setComputeUnitPrice (if priority > 0)
 * 3. Create output token ATA if needed (Token-2022)
 * 4. Vault convert instruction (9 named + 8 hook remaining_accounts)
 *
 * Named accounts (matching Convert struct in conversion_vault IDL):
 * user, vaultConfig, userInputAccount, userOutputAccount,
 * inputMint, outputMint, vaultInput, vaultOutput, tokenProgram
 *
 * remaining_accounts layout: [input_hooks(4), output_hooks(4)]
 * Each set of 4 = ExtraAccountMetaList + source WL + dest WL + hook program
 *
 * @param params - Vault convert parameters
 * @returns Complete unsigned Transaction ready for signing
 */
export async function buildVaultConvertTransaction(params: VaultConvertParams): Promise<Transaction> {
  const {
    connection,
    userPublicKey,
    amountInBaseUnits,
    minimumOutput,
    inputMint,
    outputMint,
    preBalance = 0,
    computeUnits = DEFAULT_COMPUTE_UNITS,
    priorityFeeMicroLamports = 0,
  } = params;

  const tx = new Transaction();

  // 1. Compute budget
  tx.add(ComputeBudgetProgram.setComputeUnitLimit({ units: computeUnits }));
  if (priorityFeeMicroLamports > 0) {
    tx.add(ComputeBudgetProgram.setComputeUnitPrice({ microLamports: priorityFeeMicroLamports }));
  }

  // 2. Derive PDAs
  const [vaultConfig] = PublicKey.findProgramAddressSync(
    [VAULT_SEEDS.CONFIG],
    PROGRAM_IDS.VAULT,
  );
  const vaultInput = deriveVaultTokenAccount(getVaultSeedForMint(inputMint));
  const vaultOutput = deriveVaultTokenAccount(getVaultSeedForMint(outputMint));

  // 3. Resolve user ATAs (all Token-2022)
  const userInputAccount = await getToken2022Ata(userPublicKey, inputMint);
  const userOutputAccount = await getToken2022Ata(userPublicKey, outputMint);

  // 4. Create output ATA if needed
  const outputAtaInfo = await connection.getAccountInfo(userOutputAccount);
  if (!outputAtaInfo) {
    tx.add(
      createAssociatedTokenAccountInstruction(
        userPublicKey,
        userOutputAccount,
        userPublicKey,
        outputMint,
        TOKEN_2022_PROGRAM_ID,
      ),
    );
  }

  // 5. Resolve Transfer Hook accounts for both conversion legs
  //    The vault program does two Token-2022 transfer_checked calls:
  //    - Input: user -> vault (user-signed)
  //    - Output: vault -> user (PDA-signed)
  //    Each requires 4 hook accounts (meta list, source WL, dest WL, hook program).
  //    Layout: [input_hooks(4), output_hooks(4)] — same as AMM pattern.
  const inputHooks = resolveHookAccounts(userInputAccount, inputMint, vaultInput);
  const outputHooks = resolveHookAccounts(vaultOutput, outputMint, userOutputAccount);

  // 6. Build vault convert instruction via Anchor
  const vaultProgram = getVaultProgram(connection);

  const convertIx = await vaultProgram.methods
    .convertV2(new BN(amountInBaseUnits), new BN(minimumOutput), new BN(preBalance))
    .accountsStrict({
      user: userPublicKey,
      vaultConfig,
      userInputAccount,
      userOutputAccount,
      inputMint,
      outputMint,
      vaultInput,
      vaultOutput,
      tokenProgram: TOKEN_2022_PROGRAM_ID,
    })
    .remainingAccounts([...inputHooks, ...outputHooks])
    .instruction();

  tx.add(convertIx);

  return tx;
}
