/**
 * Check enclave.validUntil for ALL oracles on the queue.
 */
import { Connection, PublicKey } from "@solana/web3.js";
import * as anchor from "@coral-xyz/anchor";
import { Program, AnchorProvider, Wallet } from "@coral-xyz/anchor";
import * as sb from "@switchboard-xyz/on-demand";
import * as fs from "fs";
import * as path from "path";

async function main() {
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

  const queueAccount = await sb.getDefaultQueue(connection.rpcEndpoint);
  const queueData = await queueAccount.loadData();
  const oracleKeys: PublicKey[] = (queueData as any).oracleKeys.slice(0, (queueData as any).oracleKeysLen);

  const now = Math.floor(Date.now() / 1000);
  console.log(`Current time: ${new Date(now * 1000).toISOString()} (${now})`);
  console.log(`Oracles: ${oracleKeys.length}\n`);

  for (let i = 0; i < oracleKeys.length; i++) {
    const oracleKey = oracleKeys[i];
    try {
      const oracle = new sb.Oracle(sbProgram as any, oracleKey);
      const data = await oracle.loadData();
      const enclave = (data as any).enclave;

      // Parse validUntil — it might be a hex string, BN, or number
      let validUntil: number;
      const rawVU = enclave.validUntil;
      if (typeof rawVU === 'string') {
        validUntil = parseInt(rawVU, 16);
      } else if (typeof rawVU === 'object' && rawVU.toNumber) {
        validUntil = rawVU.toNumber();
      } else {
        validUntil = Number(rawVU);
      }

      const expired = validUntil < now;
      const delta = validUntil - now;
      const lastHb = Number((data as any).lastHeartbeat);
      const hbAge = now - lastHb;

      const gateway = String.fromCharCode(...(data as any).gatewayUri).replace(/\0+$/g, '');
      const ip = gateway.match(/\/\/([\d.]+)/)?.[1] || gateway;

      console.log(`Oracle ${i + 1}: ${oracleKey.toBase58().slice(0, 12)}... (${ip})`);
      console.log(`  validUntil: ${new Date(validUntil * 1000).toISOString()} ${expired ? '*** EXPIRED ***' : `(${Math.round(delta / 60)}min remaining)`}`);
      console.log(`  heartbeat:  ${hbAge}s ago`);
      console.log(`  status:     ${(enclave as any).verificationStatus}`);
      console.log();
    } catch (e) {
      console.log(`Oracle ${i + 1}: ${oracleKey.toBase58().slice(0, 12)}... ERROR: ${String(e).slice(0, 100)}`);
      console.log();
    }
  }
}

main().catch(e => { console.error("Fatal:", e); process.exit(1); });
