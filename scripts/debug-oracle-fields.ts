/**
 * Diagnostic: Dump ALL fields from one oracle to find TEE key expiry fields.
 */
import { Connection, PublicKey } from "@solana/web3.js";
import * as anchor from "@coral-xyz/anchor";
import { Program, AnchorProvider, Wallet } from "@coral-xyz/anchor";
import * as sb from "@switchboard-xyz/on-demand";
import * as fs from "fs";
import * as path from "path";

async function main() {
  // Construct mainnet RPC from HELIUS_API_KEY
  const envContent = fs.readFileSync(path.join(__dirname, "../.env"), "utf8");
  const match = envContent.match(/HELIUS_API_KEY=(.+)/);
  const apiKey = match![1].trim();
  const rpcUrl = `https://mainnet.helius-rpc.com/?api-key=${apiKey}`;

  const connection = new Connection(rpcUrl, "confirmed");
  const dummyKp = anchor.web3.Keypair.generate();
  const provider = new AnchorProvider(connection, new Wallet(dummyKp), {});
  anchor.setProvider(provider);

  const sbProgramId = await sb.getProgramId(connection);
  const sbIdl = await Program.fetchIdl(sbProgramId, provider);
  if (!sbIdl) throw new Error("Failed to fetch Switchboard IDL");
  const sbProgram = new Program(sbIdl, provider);

  // Check first oracle
  const oracle = new sb.Oracle(sbProgram as any, new PublicKey("3Nv1DJdf7163FcB5dFEQGKbw6dUK4HqtwuUcyUf3DWni"));
  const data = await oracle.loadData();

  // Dump all keys and types
  console.log("=== Oracle Data Fields ===\n");
  for (const [key, val] of Object.entries(data as any)) {
    if (val === null || val === undefined) {
      console.log(`${key}: null`);
    } else if (typeof val === 'object' && 'toNumber' in val) {
      const n = val.toNumber();
      // Try to identify timestamps
      if (n > 1600000000 && n < 2000000000) {
        console.log(`${key}: ${n} (${new Date(n * 1000).toISOString()})`);
      } else {
        console.log(`${key}: ${n}`);
      }
    } else if (typeof val === 'object' && 'toBase58' in val) {
      console.log(`${key}: ${(val as PublicKey).toBase58()}`);
    } else if (Array.isArray(val)) {
      if (val.length <= 10) {
        console.log(`${key}: [${val.join(', ')}]`);
      } else {
        // Check if it's a buffer (uint8 array)
        const isBytes = val.every((v: any) => typeof v === 'number' && v >= 0 && v <= 255);
        if (isBytes) {
          const nonZero = val.filter((v: any) => v !== 0);
          if (nonZero.length > 0) {
            const str = String.fromCharCode(...val).replace(/\0+$/g, '');
            if (str.length > 0 && /^[\x20-\x7E]+$/.test(str)) {
              console.log(`${key}: "${str}" (${val.length} bytes)`);
            } else {
              console.log(`${key}: [${val.length} bytes, ${nonZero.length} non-zero]`);
            }
          } else {
            console.log(`${key}: [${val.length} zero bytes]`);
          }
        } else {
          console.log(`${key}: [array of ${val.length}]`);
        }
      }
    } else if (typeof val === 'object' && val !== null) {
      console.log(`${key}: ${JSON.stringify(val)}`);
    } else {
      console.log(`${key}: ${val} (${typeof val})`);
    }
  }

  // Also check: try to read the raw account and look for the on-chain Switchboard error
  // by attempting a commit with this oracle
  console.log("\n=== Checking randomness commit simulation ===\n");

  const queueAccount = await sb.getDefaultQueue(connection.rpcEndpoint);

  // Create a randomness account
  const rngKp = anchor.web3.Keypair.generate();
  const [randomness, createIx] = await sb.Randomness.create(
    sbProgram as any, rngKp, queueAccount.pubkey
  );

  // Simulate the create
  const createTx = new anchor.web3.Transaction().add(createIx);
  createTx.feePayer = dummyKp.publicKey;
  createTx.recentBlockhash = (await connection.getLatestBlockhash()).blockhash;

  console.log("Randomness created (not sent). Checking commitIx oracle selection...");

  // Call commitIx to see which oracle it selects
  try {
    const commitIx = await randomness.commitIx(queueAccount.pubkey);
    // Parse the oracle from the instruction accounts
    const oracleIdx = commitIx.keys.findIndex((k: any) => {
      // Oracle should be the 3rd account (randomness, queue, oracle, slothashes, authority)
      return true; // we'll just print all
    });
    console.log("Commit instruction accounts:");
    for (const key of commitIx.keys) {
      console.log(`  ${key.pubkey.toBase58()} (${key.isSigner ? 'signer' : 'readonly'}${key.isWritable ? ',writable' : ''})`);
    }
  } catch (e) {
    console.log(`commitIx failed: ${e}`);
  }
}

main().catch(e => {
  console.error("Fatal:", e);
  process.exit(1);
});
