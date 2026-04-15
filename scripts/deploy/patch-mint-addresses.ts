/**
 * Patch Mint Addresses in Rust Constants
 *
 * Reads keypairs from mint-keypairs/ and keypairs/ directories, derives their
 * public keys, and patches the hardcoded Pubkey::from_str("...") values in
 * Rust constants.rs files.
 *
 * Three categories of patches:
 *   1. Vault mint addresses:  programs/conversion-vault/src/constants.rs
 *      - crime_mint(), fraud_mint(), profit_mint()
 *   2. Tax program cross-refs: programs/tax-program/src/constants.rs
 *      - epoch_program_id(), staking_program_id(), amm_program_id()
 *   3. Treasury wallet:        programs/tax-program/src/constants.rs
 *      - treasury_pubkey() (resolved from deployments/{cluster}.json .treasury — single
 *        source of truth; mirrors the canonical address table in
 *        .docs/standalone/hardcoded-address-sweep.md)
 *   4. Bonding curve refs:     programs/bonding_curve/src/constants.rs
 *      - crime_mint(), fraud_mint(), epoch_program_id()
 *
 * This script supports both devnet (auto-generated keypairs) and mainnet
 * (pre-placed vanity keypairs) workflows with the same logic.
 *
 * Usage: npx tsx scripts/deploy/patch-mint-addresses.ts
 * Called by: scripts/deploy/build.sh (before anchor build)
 */

import { Keypair } from "@solana/web3.js";
import * as fs from "fs";
import * as path from "path";

const PROJECT_ROOT = path.resolve(__dirname, "../..");

// ---------------------------------------------------------------------------
// Keypair Loading
// ---------------------------------------------------------------------------

function loadKeypair(filePath: string): Keypair {
  const resolved = path.resolve(PROJECT_ROOT, filePath);
  if (!fs.existsSync(resolved)) {
    throw new Error(`Keypair not found: ${resolved}`);
  }
  const secretKey = JSON.parse(fs.readFileSync(resolved, "utf8"));
  return Keypair.fromSecretKey(new Uint8Array(secretKey));
}

// ---------------------------------------------------------------------------
// Patching Logic
// ---------------------------------------------------------------------------

interface PatchSpec {
  /** Human-readable label for logging */
  label: string;
  /** Path to the Rust file (relative to project root) */
  file: string;
  /** The function name to find (e.g. "crime_mint") */
  functionName: string;
  /** The new address string to insert */
  newAddress: string;
}

/**
 * Patch a single Pubkey::from_str("...") call for the given function name.
 *
 * Strategy: Patches the cfg variant matching the active cluster mode ONLY —
 * devnet mode touches the devnet-gated variant (and non-gated fallbacks);
 * mainnet mode touches the `not(any(devnet,localnet))` variant (and non-gated
 * fallbacks). The other cluster's variant is left untouched.
 *
 * Rationale: the previous cfg-blind implementation iterated ALL prefix patterns
 * regardless of mode, which wrote the active cluster's value into EVERY variant
 * of a gated function. This silently contaminated committed source files
 * whenever someone built one cluster and committed without reverting — the
 * simplest mechanical hypothesis for the 846bbd4b treasury drift incident
 * (phase 122.1 root cause). See patch-script-sweep.md for the contamination
 * audit.
 *
 * Non-gated fallback uses a negative lookbehind to ensure it only matches
 * `pub fn` declarations that are NOT preceded by a `#[cfg(...)]` attribute
 * (within ~200 chars). This prevents the non-gated iteration from
 * re-contaminating a cfg-gated variant of the same function that we
 * intentionally skipped above.
 */
function patchFile(
  content: string,
  spec: PatchSpec,
  isDevnet: boolean,
): { content: string; patched: boolean } {
  // Negative lookbehind: the `pub fn` must not be immediately preceded by any
  // `#[cfg(...)]` attribute. Used as the "non-gated" prefix so we only match
  // functions declared without a cfg attribute (e.g. amm_program_id, which has
  // no cfg variants at all).
  const NON_GATED = `(?<!#\\[cfg\\([^\\n]{0,200}\\)\\]\\s{0,10})`;

  // Mainnet prefix must match BOTH gating forms used across the codebase:
  //   - `#[cfg(not(feature = "devnet"))]`                              (tax-program crime_mint/fraud_mint)
  //   - `#[cfg(not(any(feature = "devnet", feature = "localnet")))]`   (tax-program treasury_pubkey, conversion-vault mints)
  // The `localnet` tail is optional via `(?:...)?`. Without this unified form,
  // the tax-program taxed mints would silently fail to patch under mainnet mode.
  const MAINNET_PREFIX =
    `#\\[cfg\\(not\\((?:any\\()?feature\\s*=\\s*"devnet"(?:,\\s*feature\\s*=\\s*"localnet"\\))?\\)\\)\\]\\s*`;

  // Select prefixes for the active cluster only. Never iterate the OTHER
  // cluster's prefix — that is exactly the cross-contamination we are fixing.
  const prefixes = isDevnet
    ? [
        `#\\[cfg\\(feature\\s*=\\s*"devnet"\\)\\]\\s*`, // devnet-gated
        NON_GATED,                                        // non-gated fallback
      ]
    : [
        MAINNET_PREFIX, // mainnet-gated (handles both `not(...)` forms)
        NON_GATED,      // non-gated fallback
      ];

  let anyPatched = false;
  let result = content;

  for (const prefix of prefixes) {
    // Try Pubkey::from_str("...") pattern
    const fnRegex = new RegExp(
      `(${prefix}pub\\s+fn\\s+${escapeRegex(spec.functionName)}\\s*\\(\\)\\s*->\\s*Pubkey\\s*\\{[^}]*?)` +
      `Pubkey::from_str\\("([A-Za-z0-9]+)"\\)`,
      "s"
    );

    const match = result.match(fnRegex);
    if (match) {
      const oldAddress = match[2];
      if (oldAddress !== spec.newAddress) {
        result = result.replace(fnRegex, `$1Pubkey::from_str("${spec.newAddress}")`);
        anyPatched = true;
      }
      continue;
    }

    // Try compile_error!(...) placeholder (mainnet cfg blocks before first mainnet build)
    const compileErrorRegex = new RegExp(
      `(${prefix}pub\\s+fn\\s+${escapeRegex(spec.functionName)}\\s*\\(\\)\\s*->\\s*Pubkey\\s*\\{\\s*)` +
      `compile_error!\\([^)]*\\);?\\s*`,
      "s"
    );

    const compileErrorMatch = result.match(compileErrorRegex);
    if (compileErrorMatch) {
      result = result.replace(compileErrorRegex, `$1Pubkey::from_str("${spec.newAddress}").unwrap()\n`);
      anyPatched = true;
      continue;
    }

    // Try Pubkey::default() placeholder
    const defaultRegex = new RegExp(
      `(${prefix}pub\\s+fn\\s+${escapeRegex(spec.functionName)}\\s*\\(\\)\\s*->\\s*Pubkey\\s*\\{[^}]*?)` +
      `Pubkey::default\\(\\)`,
      "s"
    );

    const defaultMatch = result.match(defaultRegex);
    if (defaultMatch) {
      result = result.replace(defaultRegex, `$1Pubkey::from_str("${spec.newAddress}").unwrap()`);
      anyPatched = true;
    }
  }

  if (!anyPatched) {
    // Check if already correct in all variants
    const alreadyCorrect = new RegExp(
      `pub\\s+fn\\s+${escapeRegex(spec.functionName)}\\s*\\(\\)\\s*->\\s*Pubkey\\s*\\{[^}]*?` +
      `Pubkey::from_str\\("${escapeRegex(spec.newAddress)}"\\)`,
      "s"
    );
    if (result.match(alreadyCorrect)) {
      return { content: result, patched: false }; // Already correct
    }
    console.warn(`  WARNING: Could not find ${spec.functionName}() in ${spec.file}`);
  }

  return { content: result, patched: anyPatched };
}

function escapeRegex(s: string): string {
  return s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

function main() {
  const isDevnet = process.argv.includes("--devnet");

  console.log("Patch Mint Addresses");
  console.log(`====================  [${isDevnet ? "DEVNET" : "MAINNET"}]\n`);

  let crimeMintAddress: string;
  let fraudMintAddress: string;
  let profitMintAddress: string;
  let epochProgramAddress: string;
  let stakingProgramAddress: string;
  let ammProgramAddress: string;
  let treasuryAddress: string;

  if (isDevnet) {
    // Devnet: read addresses from deployments/devnet.json (source of truth for devnet).
    // keypairs/ contains mainnet keypairs — cannot be used for devnet builds.
    const devnetJsonPath = path.resolve(PROJECT_ROOT, "deployments/devnet.json");
    if (!fs.existsSync(devnetJsonPath)) {
      console.error("ERROR: deployments/devnet.json not found.");
      console.error("This file is required for devnet builds to resolve correct program IDs.");
      process.exit(1);
    }
    const devnet = JSON.parse(fs.readFileSync(devnetJsonPath, "utf8"));
    crimeMintAddress = devnet.mints.crime;
    fraudMintAddress = devnet.mints.fraud;
    profitMintAddress = devnet.mints.profit;
    epochProgramAddress = devnet.programs.epochProgram;
    stakingProgramAddress = devnet.programs.staking;
    ammProgramAddress = devnet.programs.amm;
    treasuryAddress = devnet.treasury;
    console.log("  Source: deployments/devnet.json\n");
  } else {
    // Mainnet: derive mint + program addresses from keypairs (existing behavior).
    // Treasury is read from deployments/mainnet.json (single source of truth), mirroring
    // the devnet branch above. NO env var fallback, NO hardcoded default — this is the
    // class of bug that caused the 846bbd4b treasury drift incident (see
    // .docs/standalone/hardcoded-address-sweep.md History 2026-04-07). Phase 122.1-02
    // removed the `process.env.TREASURY_PUBKEY || "8kPzh..."` footgun inline because
    // it sat on the critical path of the hotfix rebuild.
    const mintKeypairsDir = path.resolve(PROJECT_ROOT, "scripts/deploy/mint-keypairs");
    if (!fs.existsSync(mintKeypairsDir)) {
      console.error("ERROR: scripts/deploy/mint-keypairs/ directory not found.");
      console.error("This directory must contain crime-mint.json, fraud-mint.json, profit-mint.json.");
      process.exit(1);
    }

    const mainnetJsonPath = path.resolve(PROJECT_ROOT, "deployments/mainnet.json");
    if (!fs.existsSync(mainnetJsonPath)) {
      console.error("ERROR: deployments/mainnet.json not found.");
      console.error("This file is required for mainnet builds to resolve the canonical treasury address.");
      process.exit(1);
    }
    const mainnet = JSON.parse(fs.readFileSync(mainnetJsonPath, "utf8"));
    if (!mainnet.treasury || typeof mainnet.treasury !== "string") {
      console.error("ERROR: deployments/mainnet.json is missing the .treasury field (or it is not a string).");
      console.error("Refusing to build with an unknown treasury address.");
      process.exit(1);
    }

    const crimeMint = loadKeypair("scripts/deploy/mint-keypairs/crime-mint.json");
    const fraudMint = loadKeypair("scripts/deploy/mint-keypairs/fraud-mint.json");
    const profitMint = loadKeypair("scripts/deploy/mint-keypairs/profit-mint.json");
    const epochProgram = loadKeypair("keypairs/epoch-program.json");
    const stakingProgram = loadKeypair("keypairs/staking-keypair.json");
    const ammProgram = loadKeypair("keypairs/amm-keypair.json");

    crimeMintAddress = crimeMint.publicKey.toBase58();
    fraudMintAddress = fraudMint.publicKey.toBase58();
    profitMintAddress = profitMint.publicKey.toBase58();
    epochProgramAddress = epochProgram.publicKey.toBase58();
    stakingProgramAddress = stakingProgram.publicKey.toBase58();
    ammProgramAddress = ammProgram.publicKey.toBase58();
    treasuryAddress = mainnet.treasury;
    console.log("  Source: deployments/mainnet.json + keypairs/\n");
  }

  // Build patch specs
  const patches: PatchSpec[] = [
    // Category 1: Vault mint addresses
    {
      label: "Vault CRIME mint",
      file: "programs/conversion-vault/src/constants.rs",
      functionName: "crime_mint",
      newAddress: crimeMintAddress,
    },
    {
      label: "Vault FRAUD mint",
      file: "programs/conversion-vault/src/constants.rs",
      functionName: "fraud_mint",
      newAddress: fraudMintAddress,
    },
    {
      label: "Vault PROFIT mint",
      file: "programs/conversion-vault/src/constants.rs",
      functionName: "profit_mint",
      newAddress: profitMintAddress,
    },
    // Category 2: Tax program cross-refs
    {
      label: "Tax epoch_program_id",
      file: "programs/tax-program/src/constants.rs",
      functionName: "epoch_program_id",
      newAddress: epochProgramAddress,
    },
    {
      label: "Tax staking_program_id",
      file: "programs/tax-program/src/constants.rs",
      functionName: "staking_program_id",
      newAddress: stakingProgramAddress,
    },
    {
      label: "Tax amm_program_id",
      file: "programs/tax-program/src/constants.rs",
      functionName: "amm_program_id",
      newAddress: ammProgramAddress,
    },
    // Category 3: Treasury wallet
    {
      label: "Tax treasury_pubkey",
      file: "programs/tax-program/src/constants.rs",
      functionName: "treasury_pubkey",
      newAddress: treasuryAddress,
    },
    // Category 4: Bonding curve mint addresses + cross-program ref
    {
      label: "Curve CRIME mint",
      file: "programs/bonding_curve/src/constants.rs",
      functionName: "crime_mint",
      newAddress: crimeMintAddress,
    },
    {
      label: "Curve FRAUD mint",
      file: "programs/bonding_curve/src/constants.rs",
      functionName: "fraud_mint",
      newAddress: fraudMintAddress,
    },
    {
      label: "Curve epoch_program_id",
      file: "programs/bonding_curve/src/constants.rs",
      functionName: "epoch_program_id",
      newAddress: epochProgramAddress,
    },
  ];

  // Apply patches grouped by file
  const fileContents = new Map<string, string>();
  let totalPatched = 0;
  let totalSkipped = 0;

  for (const spec of patches) {
    const filePath = path.resolve(PROJECT_ROOT, spec.file);

    // Load file content (cache across patches to same file)
    if (!fileContents.has(spec.file)) {
      if (!fs.existsSync(filePath)) {
        console.error(`ERROR: File not found: ${spec.file}`);
        process.exit(1);
      }
      fileContents.set(spec.file, fs.readFileSync(filePath, "utf8"));
    }

    const content = fileContents.get(spec.file)!;
    const { content: updated, patched } = patchFile(content, spec, isDevnet);
    fileContents.set(spec.file, updated);

    if (patched) {
      console.log(`  PATCHED: ${spec.label} -> ${spec.newAddress}`);
      totalPatched++;
    } else {
      console.log(`  SKIP:    ${spec.label} (already correct)`);
      totalSkipped++;
    }
  }

  // Write modified files back
  for (const [relPath, content] of fileContents) {
    const filePath = path.resolve(PROJECT_ROOT, relPath);
    fs.writeFileSync(filePath, content, { mode: 0o600 });
  }

  console.log(`\nSummary: ${totalPatched} patched, ${totalSkipped} skipped`);
  console.log("Done.\n");
}

main();
