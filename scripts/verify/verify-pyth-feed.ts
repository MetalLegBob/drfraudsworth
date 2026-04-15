/**
 * verify-pyth-feed.ts
 *
 * Verifies Pyth SOL/USD push feed accounts on-chain for both devnet and mainnet,
 * and fetches latest price from Hermes API for off-chain confirmation.
 *
 * Usage: npx tsx scripts/verify/verify-pyth-feed.ts
 */

import { Connection, PublicKey } from "@solana/web3.js";
import { HermesClient } from "@pythnetwork/hermes-client";

// --- Constants ---
const SOL_USD_FEED_ID =
  "0xef0d8b6fda2ceba41da15d4095d1da392a0d2f8ed0c6c7bc0f4cfac8c280b56d";

const DEVNET_FEED = new PublicKey(
  "7UVimffxr9ow1uXYxsr4LHAcV58mLzhmwaeKvJ1pjLiE"
);
const MAINNET_FEED = new PublicKey(
  "H6ARHf6YXhGYeQfUzQNGk6rDNnLBQKrenN712K4AQJEG"
);

const PYTH_RECEIVER_PROGRAM = new PublicKey(
  "rec5EKMGg6MxZYaMdyBfgwp4d5rB9T1VQH5pJv5LtFJ"
);

const DEVNET_RPC =
  process.env.DEVNET_RPC || "https://api.devnet.solana.com";
const MAINNET_RPC =
  process.env.MAINNET_RPC || "https://api.mainnet-beta.solana.com";

// --- On-chain verification ---
interface FeedResult {
  cluster: string;
  address: string;
  exists: boolean;
  ownerCorrect: boolean | null;
  owner: string | null;
  dataLength: number | null;
  lamports: number | null;
}

async function verifyFeedOnChain(
  rpcUrl: string,
  feedPubkey: PublicKey,
  cluster: string
): Promise<FeedResult> {
  console.log(`\n--- ${cluster}: On-chain Feed Verification ---`);
  console.log(`  Feed address: ${feedPubkey.toBase58()}`);
  console.log(`  RPC: ${rpcUrl}`);

  const connection = new Connection(rpcUrl, "confirmed");

  try {
    const accountInfo = await connection.getAccountInfo(feedPubkey);

    if (!accountInfo) {
      console.log(`  FEED NOT FOUND on ${cluster}`);
      return {
        cluster,
        address: feedPubkey.toBase58(),
        exists: false,
        ownerCorrect: null,
        owner: null,
        dataLength: null,
        lamports: null,
      };
    }

    const ownerCorrect = accountInfo.owner.equals(PYTH_RECEIVER_PROGRAM);

    console.log(`  Account exists: YES`);
    console.log(`  Owner: ${accountInfo.owner.toBase58()}`);
    console.log(
      `  Owner correct (Pyth Receiver): ${ownerCorrect ? "YES" : "NO -- expected " + PYTH_RECEIVER_PROGRAM.toBase58()}`
    );
    console.log(`  Data length: ${accountInfo.data.length} bytes`);
    console.log(
      `  Lamports: ${accountInfo.lamports} (${(accountInfo.lamports / 1e9).toFixed(6)} SOL)`
    );

    return {
      cluster,
      address: feedPubkey.toBase58(),
      exists: true,
      ownerCorrect,
      owner: accountInfo.owner.toBase58(),
      dataLength: accountInfo.data.length,
      lamports: accountInfo.lamports,
    };
  } catch (err: unknown) {
    const msg = err instanceof Error ? err.message : String(err);
    console.log(`  RPC error: ${msg}`);
    return {
      cluster,
      address: feedPubkey.toBase58(),
      exists: false,
      ownerCorrect: null,
      owner: null,
      dataLength: null,
      lamports: null,
    };
  }
}

// --- Off-chain Hermes verification ---
async function verifyHermes(): Promise<void> {
  console.log("\n--- Hermes Off-chain Price Verification ---");
  console.log(`  Feed ID: ${SOL_USD_FEED_ID}`);
  console.log(`  Endpoint: https://hermes.pyth.network`);

  try {
    const client = new HermesClient("https://hermes.pyth.network");
    const priceUpdates = await client.getLatestPriceUpdates([
      SOL_USD_FEED_ID,
    ]);

    if (
      !priceUpdates ||
      !priceUpdates.parsed ||
      priceUpdates.parsed.length === 0
    ) {
      console.log("  No price updates returned from Hermes");
      return;
    }

    for (const update of priceUpdates.parsed) {
      const price = update.price;
      const displayPrice =
        Number(price.price) * Math.pow(10, price.expo);
      const confidence =
        Number(price.conf) * Math.pow(10, price.expo);
      const publishTime = new Date(
        Number(price.publish_time) * 1000
      );
      const ageSeconds = Math.floor(
        (Date.now() - publishTime.getTime()) / 1000
      );

      console.log(`  SOL/USD Price: $${displayPrice.toFixed(4)}`);
      console.log(`  Confidence: +/- $${confidence.toFixed(4)}`);
      console.log(`  Exponent: ${price.expo}`);
      console.log(`  Publish time: ${publishTime.toISOString()}`);
      console.log(`  Age: ${ageSeconds} seconds`);

      if (ageSeconds > 300) {
        console.log(
          `  WARNING: Price is ${ageSeconds}s old (>5 min). Feed may be stale.`
        );
      } else {
        console.log(`  Feed is FRESH (within 5 minutes).`);
      }
    }
  } catch (err: unknown) {
    const msg = err instanceof Error ? err.message : String(err);
    console.log(`  Hermes error: ${msg}`);
  }
}

// --- Main ---
async function main() {
  console.log("=== Pyth SOL/USD Push Feed Verification ===");
  console.log(`Pyth Receiver Program: ${PYTH_RECEIVER_PROGRAM.toBase58()}`);

  const devnetResult = await verifyFeedOnChain(
    DEVNET_RPC,
    DEVNET_FEED,
    "Devnet"
  );
  const mainnetResult = await verifyFeedOnChain(
    MAINNET_RPC,
    MAINNET_FEED,
    "Mainnet"
  );

  await verifyHermes();

  // Summary
  console.log("\n=== Summary ===");
  console.log("| Cluster | Address | Status | Owner Correct |");
  console.log("|---------|---------|--------|---------------|");

  for (const r of [devnetResult, mainnetResult]) {
    const status = r.exists ? "VERIFIED" : "NOT FOUND";
    const owner = r.ownerCorrect === true ? "YES" : r.ownerCorrect === false ? "NO" : "N/A";
    console.log(
      `| ${r.cluster} | ${r.address} | ${status} | ${owner} |`
    );
  }

  if (!devnetResult.exists || !mainnetResult.exists) {
    console.log(
      "\nWARNING: One or more feed accounts were not found. This is a critical finding."
    );
    console.log(
      "The Pyth SOL/USD push feed addresses may need to be re-derived or verified manually."
    );
  }
}

main().catch((err) => {
  console.error("Fatal error:", err);
  process.exit(1);
});
