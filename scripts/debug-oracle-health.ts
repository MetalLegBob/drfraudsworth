/**
 * Diagnostic: Check Switchboard queue oracle health on mainnet.
 * Lists all oracles on the queue and checks their TEE key expiry.
 */
import { Connection, PublicKey } from "@solana/web3.js";
import * as anchor from "@coral-xyz/anchor";
import { Program, AnchorProvider, Wallet } from "@coral-xyz/anchor";
import * as sb from "@switchboard-xyz/on-demand";
import * as fs from "fs";
import * as path from "path";

async function main() {
  // Load mainnet RPC: use CLUSTER_URL env, or construct from HELIUS_API_KEY
  let rpcUrl = process.env.CLUSTER_URL;

  if (!rpcUrl) {
    // Try to get HELIUS_API_KEY from .env and construct mainnet URL
    try {
      const envContent = fs.readFileSync(path.join(__dirname, "../.env"), "utf8");
      const match = envContent.match(/HELIUS_API_KEY=(.+)/);
      if (match) {
        const apiKey = match[1].trim();
        rpcUrl = `https://mainnet.helius-rpc.com/?api-key=${apiKey}`;
      }
    } catch {}
  }

  if (!rpcUrl) {
    console.error("No RPC URL found. Set CLUSTER_URL env var.");
    process.exit(1);
  }

  console.log(`RPC: ${rpcUrl.slice(0, 40)}...`);
  const connection = new Connection(rpcUrl, "confirmed");

  // Create a dummy provider (we only need read access)
  const dummyKp = anchor.web3.Keypair.generate();
  const dummyWallet = new Wallet(dummyKp);
  const provider = new AnchorProvider(connection, dummyWallet, {});
  anchor.setProvider(provider);

  // Get Switchboard program
  const sbProgramId = await sb.getProgramId(connection);
  console.log(`Switchboard Program: ${sbProgramId.toBase58()}`);

  const sbIdl = await Program.fetchIdl(sbProgramId, provider);
  if (!sbIdl) throw new Error("Failed to fetch Switchboard IDL");
  const sbProgram = new Program(sbIdl, provider);

  // Get the default queue
  const queueAccount = await sb.getDefaultQueue(connection.rpcEndpoint);
  console.log(`Queue: ${queueAccount.pubkey.toBase58()}`);

  // Fetch oracle keys from queue
  const queueData = await queueAccount.loadData();
  const oracleKeys: PublicKey[] = (queueData as any).oracleKeys.slice(0, (queueData as any).oracleKeysLen);
  console.log(`\nOracles on queue: ${oracleKeys.length}`);

  // Check each oracle
  const now = Math.floor(Date.now() / 1000);
  console.log(`Current unix timestamp: ${now}\n`);

  for (let i = 0; i < oracleKeys.length; i++) {
    const oracleKey = oracleKeys[i];
    console.log(`--- Oracle ${i + 1}: ${oracleKey.toBase58()} ---`);

    try {
      const oracle = new sb.Oracle(sbProgram as any, oracleKey);
      const oracleData = await oracle.loadData();

      // Check key fields
      const gatewayUri = String.fromCharCode(...(oracleData as any).gatewayUri).replace(/\0+$/g, '');
      console.log(`  Gateway: ${gatewayUri}`);

      // Check enclave/TEE validity
      if ((oracleData as any).enclaveValidUntil) {
        const validUntil = Number((oracleData as any).enclaveValidUntil);
        const expired = validUntil < now;
        const delta = validUntil - now;
        console.log(`  Enclave valid until: ${new Date(validUntil * 1000).toISOString()} (${expired ? 'EXPIRED' : 'valid'}, ${Math.abs(delta)}s ${expired ? 'ago' : 'remaining'})`);
      }

      // Check last heartbeat
      if ((oracleData as any).lastHeartbeat) {
        const lastHb = Number((oracleData as any).lastHeartbeat);
        const hbAge = now - lastHb;
        console.log(`  Last heartbeat: ${new Date(lastHb * 1000).toISOString()} (${hbAge}s ago)`);
      }

      // Check secp authority (signing key)
      if ((oracleData as any).secpAuthority) {
        console.log(`  Secp authority: ${Buffer.from((oracleData as any).secpAuthority).toString('hex').slice(0, 40)}...`);
      }

      // Print any other useful timestamps
      if ((oracleData as any).validUntil) {
        const vu = Number((oracleData as any).validUntil);
        console.log(`  validUntil: ${new Date(vu * 1000).toISOString()} (${vu < now ? 'EXPIRED' : 'valid'})`);
      }

      // Dump all numeric fields that look like timestamps
      for (const [key, val] of Object.entries(oracleData as any)) {
        if (typeof val === 'object' && val !== null && 'toNumber' in val) {
          const n = val.toNumber();
          // Looks like a unix timestamp if > 1600000000 and < 2000000000
          if (n > 1600000000 && n < 2000000000) {
            if (!['enclaveValidUntil', 'lastHeartbeat', 'validUntil'].includes(key)) {
              console.log(`  ${key}: ${new Date(n * 1000).toISOString()} (${n < now ? 'PAST' : 'FUTURE'})`);
            }
          }
        } else if (typeof val === 'number' && val > 1600000000 && val < 2000000000) {
          if (!['enclaveValidUntil', 'lastHeartbeat', 'validUntil'].includes(key)) {
            console.log(`  ${key}: ${new Date(val * 1000).toISOString()} (${val < now ? 'PAST' : 'FUTURE'})`);
          }
        }
      }
    } catch (e) {
      console.log(`  ERROR loading oracle data: ${String(e).slice(0, 200)}`);
    }
    console.log();
  }
}

main().catch(e => {
  console.error("Fatal:", e);
  process.exit(1);
});
