/**
 * verify-jupiter-devnet.ts
 *
 * Tests Jupiter SOL/USDC routing availability on devnet and mainnet.
 * Three probe steps:
 *   1. Mainnet API with devnet USDC mint (expected to fail -- proves mainnet-only)
 *   2. Mainnet API with mainnet USDC mint (baseline -- proves API is functional)
 *   3. Devnet endpoint probe (tests whether devnet.jup.ag exposes a working API)
 *
 * Usage: npx tsx scripts/verify/verify-jupiter-devnet.ts
 */

const SOL_MINT = "So11111111111111111111111111111111111111112";
const USDC_DEVNET = "4zMMC9srt5Ri5X14GAgXhaHii3GnPAEERYPJgZJDncDU";
const USDC_MAINNET = "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v";
const TEST_AMOUNT = "10000000"; // 0.01 SOL in lamports

interface ProbeResult {
  step: string;
  url: string;
  status: number | string;
  routeFound: boolean;
  details: string;
}

const results: ProbeResult[] = [];

async function probe(step: string, url: string): Promise<ProbeResult> {
  console.log(`\n--- ${step} ---`);
  console.log(`URL: ${url}`);

  try {
    const response = await fetch(url);
    const text = await response.text();
    const truncated = text.length > 500 ? text.slice(0, 500) + "..." : text;

    console.log(`HTTP Status: ${response.status}`);
    console.log(`Response: ${truncated}`);

    let routeFound = false;
    if (response.ok) {
      try {
        const json = JSON.parse(text);
        // Jupiter v1 returns outAmount or routePlan
        routeFound = !!(json.outAmount || json.routePlan?.length);
        if (routeFound) {
          console.log(
            `Route found! outAmount: ${json.outAmount}, hops: ${json.routePlan?.length ?? "N/A"}`
          );
        }
      } catch {
        // Not JSON
      }
    }

    const result: ProbeResult = {
      step,
      url,
      status: response.status,
      routeFound,
      details: truncated,
    };
    results.push(result);
    return result;
  } catch (err: unknown) {
    const errorMsg =
      err instanceof Error ? err.message : String(err);
    console.log(`Network error: ${errorMsg}`);

    const result: ProbeResult = {
      step,
      url,
      status: `ERROR: ${errorMsg}`,
      routeFound: false,
      details: errorMsg,
    };
    results.push(result);
    return result;
  }
}

async function main() {
  console.log("=== Jupiter Devnet SOL/USDC Routing Verification ===");
  console.log(`SOL mint: ${SOL_MINT}`);
  console.log(`USDC devnet: ${USDC_DEVNET}`);
  console.log(`USDC mainnet: ${USDC_MAINNET}`);
  console.log(`Test amount: ${TEST_AMOUNT} lamports (0.01 SOL)`);

  // Step 1: Mainnet API with devnet USDC mint
  await probe(
    "Step 1: Mainnet API + devnet USDC mint",
    `https://api.jup.ag/swap/v1/quote?inputMint=${SOL_MINT}&outputMint=${USDC_DEVNET}&amount=${TEST_AMOUNT}&slippageBps=50`
  );

  // Step 2: Mainnet API with mainnet USDC mint (baseline)
  await probe(
    "Step 2: Mainnet API + mainnet USDC mint (baseline)",
    `https://api.jup.ag/swap/v1/quote?inputMint=${SOL_MINT}&outputMint=${USDC_MAINNET}&amount=${TEST_AMOUNT}&slippageBps=50`
  );

  // Step 3a: Devnet endpoint (legacy path)
  await probe(
    "Step 3a: devnet.jup.ag legacy API",
    `https://devnet.jup.ag/api/quote?inputMint=${SOL_MINT}&outputMint=${USDC_DEVNET}&amount=${TEST_AMOUNT}&slippageBps=50`
  );

  // Step 3b: Devnet endpoint (v1 path)
  await probe(
    "Step 3b: devnet.jup.ag v1 API",
    `https://devnet.jup.ag/swap/v1/quote?inputMint=${SOL_MINT}&outputMint=${USDC_DEVNET}&amount=${TEST_AMOUNT}&slippageBps=50`
  );

  // Summary
  console.log("\n=== Summary ===");
  console.log("| Step | Status | Route Found |");
  console.log("|------|--------|-------------|");
  for (const r of results) {
    console.log(
      `| ${r.step} | ${r.status} | ${r.routeFound ? "YES" : "NO"} |`
    );
  }

  const devnetAvailable = results.some(
    (r) =>
      (r.step.includes("Step 3") || r.step.includes("devnet")) &&
      r.routeFound
  );
  const mainnetAvailable = results.some(
    (r) => r.step.includes("Step 2") && r.routeFound
  );

  console.log("");
  if (devnetAvailable) {
    console.log("Jupiter devnet routing: AVAILABLE");
  } else if (mainnetAvailable) {
    console.log("Jupiter devnet routing: UNAVAILABLE");
    console.log(
      "Jupiter mainnet routing: AVAILABLE (baseline confirmed)"
    );
    console.log("");
    console.log("Fallback strategy:");
    console.log(
      "  - Anchor tests: Mock Jupiter CPI (program-level mock or test helper)"
    );
    console.log(
      "  - Mainnet: Real Jupiter routing via api.jup.ag (confirmed working)"
    );
    console.log(
      "  - E2E testing: Use mainnet-fork or mainnet directly for Jupiter integration"
    );
  } else {
    console.log("Jupiter routing: UNAVAILABLE (both devnet and mainnet failed)");
    console.log(
      "WARNING: This is unexpected. Check network connectivity and Jupiter API status."
    );
  }
}

main().catch((err) => {
  console.error("Fatal error:", err);
  process.exit(1);
});
