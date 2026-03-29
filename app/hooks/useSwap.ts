"use client";

/**
 * useSwap -- Orchestrates the entire swap lifecycle
 *
 * State machine: idle -> quoting -> building -> signing -> sending -> confirming -> confirmed/failed
 *
 * Manages:
 * - Token pair selection with VALID_PAIRS enforcement
 * - Debounced quoting (300ms) in both directions (input->output and output->input)
 * - Transaction building via swap-builders.ts
 * - Wallet signing + sending via useProtocolWallet
 * - Transaction submission and confirmation
 * - Error parsing via error-map.ts
 * - Auto-reset 10 seconds after confirmed state
 *
 * This hook is consumed by SwapForm.tsx (the sole hook consumer for the swap UI,
 * following the SwapForm pattern of props-only child components).
 */

import { useState, useCallback, useRef, useEffect } from "react";
import { useProtocolWallet } from "./useProtocolWallet";
import { useSettings } from "./useSettings";
import { usePoolPrices } from "./usePoolPrices";
import { useEpochState } from "./useEpochState";
import { useTokenBalances } from "./useTokenBalances";
import { useRoutes } from "./useRoutes";
import { getConnection } from "@/lib/connection";
import {
  buildSolBuyTransaction,
  buildSolSellTransaction,
  buildVaultConvertTransaction,
  compileToVersionedTransaction,
} from "@/lib/swap/swap-builders";
import {
  buildAtomicRoute,
  executeAtomicRoute,
} from "@/lib/swap/multi-hop-builder";
import {
  quoteSolBuy,
  quoteSolSell,
  quoteVaultConvert,
  reverseQuoteSolBuy,
  reverseQuoteSolSell,
  reverseQuoteVaultConvert,
} from "@/lib/swap/quote-engine";
import { parseSwapError } from "@/lib/swap/error-map";
import { Transaction } from "@solana/web3.js";
import { pollTransactionConfirmation } from "@/lib/confirm-transaction";
import type { Route } from "@/lib/swap/route-types";
import { resolvePool, resolveRoute } from "@/lib/protocol-config";
import {
  type TokenSymbol,
  VALID_PAIRS,
  TOKEN_DECIMALS,
  SOL_POOL_FEE_BPS,
  VAULT_CONVERSION_RATE,
} from "@dr-fraudsworth/shared";

// =============================================================================
// BigInt conversion helpers (shared constants are `number`, quote-engine needs `bigint`)
// =============================================================================

const SOL_POOL_FEE_BPS_BI = BigInt(SOL_POOL_FEE_BPS);
const VAULT_CONVERSION_RATE_BI = BigInt(VAULT_CONVERSION_RATE);

// =============================================================================
// Types
// =============================================================================

/** Transaction lifecycle states */
export type SwapStatus =
  | "idle"
  | "quoting"
  | "building"
  | "signing"
  | "sending"
  | "confirming"
  | "confirmed"
  | "failed";

/** Price quote result displayed in the fee breakdown */
export interface SwapQuote {
  /** Output amount in base units */
  outputAmount: number;
  /** LP fee in base units */
  lpFee: number;
  /** Tax amount in base units (0 for vault conversions) */
  taxAmount: number;
  /** Price impact in basis points */
  priceImpactBps: number;
  /** Minimum output after slippage deduction */
  minimumOutput: number;
  /** Total fee percentage string for display (e.g. "3.5%") */
  totalFeePct: string;
}

/** Priority fee preset -- canonical definition in SettingsProvider */
export type { PriorityFeePreset } from "@/providers/SettingsProvider";
import type { PriorityFeePreset } from "@/providers/SettingsProvider";

/** microLamports per compute unit for each preset */
const PRIORITY_FEE_MAP: Record<PriorityFeePreset, number> = {
  none: 0,
  low: 1_000,
  medium: 10_000,
  high: 100_000,
  turbo: 1_000_000,
};

/** SOL decimals (9) for lamport conversions */
const SOL_DECIMALS = 9;

/** Debounce delay for quoting in milliseconds */
const QUOTE_DEBOUNCE_MS = 300;

/** Auto-dismiss delay after confirmed state */
const AUTO_RESET_MS = 10_000;

// =============================================================================
// Hook Return Interface
// =============================================================================

export interface UseSwapReturn {
  // Form state
  inputToken: TokenSymbol;
  outputToken: TokenSymbol;
  inputAmount: string;
  outputAmount: string;
  setInputToken: (token: TokenSymbol) => void;
  setOutputToken: (token: TokenSymbol) => void;
  setInputAmount: (amount: string) => void;
  setOutputAmount: (amount: string) => void;
  flipTokens: () => void;

  // Quote data
  quote: SwapQuote | null;
  quoteLoading: boolean;

  // Swap config
  slippageBps: number;
  setSlippageBps: (bps: number) => void;
  priorityFeePreset: PriorityFeePreset;
  setPriorityFeePreset: (preset: PriorityFeePreset) => void;

  // Execution
  executeSwap: () => Promise<void>;
  executeRoute: () => Promise<void>;
  status: SwapStatus;
  txSignature: string | null;
  errorMessage: string | null;

  // Smart routing
  smartRouting: boolean;
  setSmartRouting: (enabled: boolean) => void;
  routes: Route[];
  selectedRoute: Route | null;
  selectRoute: (route: Route) => void;
  routesLoading: boolean;
  refreshCountdown: number;

  // Wallet
  connected: boolean;

  // Balances
  balances: { sol: number; crime: number; fraud: number; profit: number };
  balancesLoading: boolean;

  // Reset
  resetForm: () => void;
}

// =============================================================================
// Hook Implementation
// =============================================================================

export function useSwap(): UseSwapReturn {
  // --- Token pair state ---
  const [inputToken, setInputTokenRaw] = useState<TokenSymbol>("SOL");
  const [outputToken, setOutputTokenRaw] = useState<TokenSymbol>("PROFIT");

  // --- Amount strings (user-entered, displayed in fields) ---
  const [inputAmount, setInputAmountRaw] = useState("");
  const [outputAmount, setOutputAmountRaw] = useState("");

  // --- Which field the user is currently editing ---
  // "input" means user typed in input field, output is computed
  // "output" means user typed in output field, input is computed
  const [editingField, setEditingField] = useState<"input" | "output">("input");

  // --- Quote ---
  const [quote, setQuote] = useState<SwapQuote | null>(null);
  const [quoteLoading, setQuoteLoading] = useState(false);

  // --- Config (from shared SettingsProvider -- single source of truth) ---
  const { settings, setSlippageBps, setPriorityFeePreset } = useSettings();
  const slippageBps = settings.slippageBps;
  const priorityFeePreset = settings.priorityFeePreset;

  // --- Execution state ---
  const [status, setStatus] = useState<SwapStatus>("idle");
  const [txSignature, setTxSignature] = useState<string | null>(null);
  const [errorMessage, setErrorMessage] = useState<string | null>(null);

  // --- Smart routing state ---
  const [smartRouting, setSmartRouting] = useState(true); // default ON

  // --- Data hooks ---
  const wallet = useProtocolWallet();
  const { pools } = usePoolPrices();
  const { epochState } = useEpochState();
  const { sol, crime, fraud, profit, loading: balancesLoading, refresh: refreshBalances } =
    useTokenBalances(wallet.publicKey);

  // --- Refs for debouncing ---
  const quoteTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const autoResetTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  // When the user starts editing after a completed/failed swap, clear the
  // terminal status so the next button click executes instead of resetting.
  const clearTerminalStatus = useCallback(() => {
    if (status === 'confirmed' || status === 'failed') {
      setStatus('idle');
      setTxSignature(null);
      setErrorMessage(null);
      if (autoResetTimerRef.current) {
        clearTimeout(autoResetTimerRef.current);
        autoResetTimerRef.current = null;
      }
    }
  }, [status]);

  // --- Smart routing: compute input in base units for useRoutes ---
  // Inline conversion since toBaseUnits callback is defined later but this
  // needs to run unconditionally as a hook call.
  const inputBaseUnits = (() => {
    const parsed = parseFloat(inputAmount);
    if (isNaN(parsed) || parsed <= 0) return 0;
    const decimals = inputToken === "SOL" ? SOL_DECIMALS : TOKEN_DECIMALS;
    return Math.floor(parsed * 10 ** decimals);
  })();

  const routesResult = useRoutes(
    inputToken, outputToken, inputBaseUnits,
    pools, epochState, slippageBps,
    smartRouting,
  );

  // ==========================================================================
  // Token selection with pair validation
  // ==========================================================================

  const setInputToken = useCallback(
    (token: TokenSymbol) => {
      clearTerminalStatus();
      setInputTokenRaw(token);
      // If the current output is no longer valid for the new input, pick the first valid one
      const validOutputs = VALID_PAIRS[token];
      if (!validOutputs.includes(outputToken)) {
        setOutputTokenRaw(validOutputs[0]);
      }
      // Clear amounts when changing tokens
      setInputAmountRaw("");
      setOutputAmountRaw("");
      setQuote(null);
    },
    [outputToken, clearTerminalStatus],
  );

  const setOutputToken = useCallback(
    (token: TokenSymbol) => {
      clearTerminalStatus();
      setOutputTokenRaw(token);
      // If the current input is no longer valid for the new output, pick the first valid one
      const validInputs = Object.entries(VALID_PAIRS)
        .filter(([, outputs]) => outputs.includes(token))
        .map(([input]) => input as TokenSymbol);
      if (!validInputs.includes(inputToken)) {
        setInputTokenRaw(validInputs[0]);
      }
      // Clear amounts when changing tokens
      setInputAmountRaw("");
      setOutputAmountRaw("");
      setQuote(null);
    },
    [inputToken, clearTerminalStatus],
  );

  // ==========================================================================
  // Quoting logic
  // ==========================================================================

  /**
   * Get the decimals for a token (SOL = 9, all others = 6).
   */
  const getDecimals = useCallback((token: TokenSymbol): number => {
    return token === "SOL" ? SOL_DECIMALS : TOKEN_DECIMALS;
  }, []);

  /**
   * Convert a user-visible amount string to base units.
   */
  const toBaseUnits = useCallback(
    (amount: string, token: TokenSymbol): number => {
      const parsed = parseFloat(amount);
      if (isNaN(parsed) || parsed <= 0) return 0;
      const decimals = getDecimals(token);
      return Math.floor(parsed * 10 ** decimals);
    },
    [getDecimals],
  );

  /**
   * Convert base units to a display string.
   */
  const fromBaseUnits = useCallback(
    (baseUnits: number, token: TokenSymbol): string => {
      if (baseUnits <= 0) return "";
      const decimals = getDecimals(token);
      const value = baseUnits / 10 ** decimals;
      // Show up to `decimals` places, but trim trailing zeros
      return value.toFixed(decimals).replace(/\.?0+$/, "");
    },
    [getDecimals],
  );

  /**
   * Compute a quote based on the current token pair and amounts.
   * Called after debounce timer fires.
   */
  const computeQuote = useCallback(
    (field: "input" | "output", amount: string) => {
      // Try AMM pool first (SOL pairs)
      const poolConfig = resolvePool(inputToken, outputToken);

      // Try vault conversion (CRIME/FRAUD <-> PROFIT)
      const routeConfig = !poolConfig ? resolveRoute(inputToken, outputToken) : null;

      if (!poolConfig && (!routeConfig || routeConfig.type !== "vaultConvert")) {
        // Multi-hop pair (SOL<->PROFIT, CRIME<->FRAUD) — handled by route engine
        setQuote(null);
        return;
      }

      // --- Vault conversion (deterministic, no reserves needed) ---
      if (routeConfig && routeConfig.type === "vaultConvert") {
        if (field === "input") {
          const baseUnits = toBaseUnits(amount, inputToken);
          if (baseUnits <= 0) { setQuote(null); setOutputAmountRaw(""); return; }

          const isProfitInput = inputToken === "PROFIT";
          const result = quoteVaultConvert(BigInt(baseUnits), VAULT_CONVERSION_RATE_BI, isProfitInput);
          const outputAmount = Number(result.outputAmount);
          // Deterministic conversion: no slippage, minimumOutput = outputAmount
          setQuote({
            outputAmount,
            lpFee: 0,
            taxAmount: 0,
            priceImpactBps: 0,
            minimumOutput: outputAmount,
            totalFeePct: "0%",
          });
          setOutputAmountRaw(fromBaseUnits(outputAmount, outputToken));
        } else {
          const desiredOutput = toBaseUnits(amount, outputToken);
          if (desiredOutput <= 0) { setQuote(null); setInputAmountRaw(""); return; }

          const isProfitInput = inputToken === "PROFIT";
          const result = reverseQuoteVaultConvert(BigInt(desiredOutput), VAULT_CONVERSION_RATE_BI, isProfitInput);
          if (!result) { setQuote(null); setInputAmountRaw(""); return; }

          setQuote({
            outputAmount: desiredOutput,
            lpFee: 0,
            taxAmount: 0,
            priceImpactBps: 0,
            minimumOutput: desiredOutput,
            totalFeePct: "0%",
          });
          setInputAmountRaw(fromBaseUnits(Number(result.inputNeeded), inputToken));
        }
        return;
      }

      // --- AMM pool swap (SOL pairs) ---
      const isTaxed = poolConfig!.isTaxed;
      if (isTaxed && !epochState) {
        setQuote(null);
        return;
      }

      const poolData = pools[poolConfig!.label];
      if (!poolData || poolData.loading) {
        setQuote(null);
        return;
      }

      const instruction = poolConfig!.instruction;

      if (field === "input") {
        const baseUnits = toBaseUnits(amount, inputToken);
        if (baseUnits <= 0) {
          setQuote(null);
          setOutputAmountRaw("");
          return;
        }

        if (instruction === "swapSolBuy") {
          // SOL -> Token (taxed)
          const reserveWsol = poolData.reserveA;
          const reserveToken = poolData.reserveB;
          const isCrime = outputToken === "CRIME";
          const buyTaxBps = isCrime ? epochState!.crimeBuyTaxBps : epochState!.fraudBuyTaxBps;

          const result = quoteSolBuy(BigInt(baseUnits), BigInt(reserveWsol), BigInt(reserveToken), BigInt(buyTaxBps), SOL_POOL_FEE_BPS_BI);
          const outputTokens = Number(result.outputTokens);
          const lpFee = Number(result.lpFee);
          const taxAmount = Number(result.taxAmount);
          const priceImpactBps = Number(result.priceImpactBps);
          const minimumOutput = Math.floor(outputTokens * (10_000 - slippageBps) / 10_000);
          const totalFeeAmount = taxAmount + lpFee;
          const totalFeePct = baseUnits > 0
            ? ((totalFeeAmount / baseUnits) * 100).toFixed(1) + "%"
            : "0%";

          setQuote({
            outputAmount: outputTokens,
            lpFee,
            taxAmount,
            priceImpactBps,
            minimumOutput,
            totalFeePct,
          });
          setOutputAmountRaw(fromBaseUnits(outputTokens, outputToken));

        } else if (instruction === "swapSolSell") {
          // Token -> SOL (taxed)
          const reserveWsol = poolData.reserveA;
          const reserveToken = poolData.reserveB;
          const isCrime = inputToken === "CRIME";
          const sellTaxBps = isCrime ? epochState!.crimeSellTaxBps : epochState!.fraudSellTaxBps;

          const result = quoteSolSell(BigInt(baseUnits), BigInt(reserveWsol), BigInt(reserveToken), BigInt(sellTaxBps), SOL_POOL_FEE_BPS_BI);
          const outputSol = Number(result.outputSol);
          const lpFee = Number(result.lpFee);
          const taxAmount = Number(result.taxAmount);
          const grossSolOutput = Number(result.grossSolOutput);
          const priceImpactBps = Number(result.priceImpactBps);
          const minimumOutput = Math.floor(outputSol * (10_000 - slippageBps) / 10_000);
          const totalFeePct = grossSolOutput > 0
            ? ((taxAmount / grossSolOutput) * 100 + (lpFee / baseUnits) * 100).toFixed(1) + "%"
            : "0%";

          setQuote({
            outputAmount: outputSol,
            lpFee,
            taxAmount,
            priceImpactBps,
            minimumOutput,
            totalFeePct,
          });
          setOutputAmountRaw(fromBaseUnits(outputSol, outputToken));
        }

      } else {
        // User typed output amount, reverse-compute input
        const desiredOutput = toBaseUnits(amount, outputToken);
        if (desiredOutput <= 0) {
          setQuote(null);
          setInputAmountRaw("");
          return;
        }

        if (instruction === "swapSolBuy") {
          // Want X tokens, need Y SOL
          const reserveWsol = poolData.reserveA;
          const reserveToken = poolData.reserveB;
          const isCrime = outputToken === "CRIME";
          const buyTaxBps = isCrime ? epochState!.crimeBuyTaxBps : epochState!.fraudBuyTaxBps;

          const result = reverseQuoteSolBuy(BigInt(desiredOutput), BigInt(reserveWsol), BigInt(reserveToken), BigInt(buyTaxBps), SOL_POOL_FEE_BPS_BI);
          if (!result) {
            setQuote(null);
            setInputAmountRaw("");
            return;
          }
          const inputSolNeeded = Number(result.inputSolNeeded);
          const lpFee = Number(result.lpFee);
          const taxAmount = Number(result.taxAmount);
          const minimumOutput = Math.floor(desiredOutput * (10_000 - slippageBps) / 10_000);
          const totalFeeAmount = taxAmount + lpFee;
          const totalFeePct = inputSolNeeded > 0
            ? ((totalFeeAmount / inputSolNeeded) * 100).toFixed(1) + "%"
            : "0%";

          setQuote({
            outputAmount: desiredOutput,
            lpFee,
            taxAmount,
            priceImpactBps: 0,
            minimumOutput,
            totalFeePct,
          });
          setInputAmountRaw(fromBaseUnits(inputSolNeeded, inputToken));

        } else if (instruction === "swapSolSell") {
          // Want X SOL, need Y tokens
          const reserveWsol = poolData.reserveA;
          const reserveToken = poolData.reserveB;
          const isCrime = inputToken === "CRIME";
          const sellTaxBps = isCrime ? epochState!.crimeSellTaxBps : epochState!.fraudSellTaxBps;

          const result = reverseQuoteSolSell(BigInt(desiredOutput), BigInt(reserveWsol), BigInt(reserveToken), BigInt(sellTaxBps), SOL_POOL_FEE_BPS_BI);
          if (!result) {
            setQuote(null);
            setInputAmountRaw("");
            return;
          }
          const inputTokensNeeded = Number(result.inputTokensNeeded);
          const lpFee = Number(result.lpFee);
          const taxAmount = Number(result.taxAmount);
          const minimumOutput = Math.floor(desiredOutput * (10_000 - slippageBps) / 10_000);
          const totalFeePct = desiredOutput > 0
            ? ((taxAmount / (desiredOutput + taxAmount)) * 100 + (lpFee / inputTokensNeeded) * 100).toFixed(1) + "%"
            : "0%";

          setQuote({
            outputAmount: desiredOutput,
            lpFee,
            taxAmount,
            priceImpactBps: 0,
            minimumOutput,
            totalFeePct,
          });
          setInputAmountRaw(fromBaseUnits(inputTokensNeeded, inputToken));
        }
      }
    },
    [inputToken, outputToken, pools, epochState, slippageBps, toBaseUnits, fromBaseUnits],
  );

  // ==========================================================================
  // Debounced setters for input/output amounts
  // ==========================================================================

  const setInputAmount = useCallback(
    (amount: string) => {
      clearTerminalStatus();
      setInputAmountRaw(amount);
      setEditingField("input");

      // Clear existing debounce timer
      if (quoteTimerRef.current) {
        clearTimeout(quoteTimerRef.current);
      }

      if (!amount || parseFloat(amount) <= 0) {
        setOutputAmountRaw("");
        setQuote(null);
        setQuoteLoading(false);
        return;
      }

      setQuoteLoading(true);
      quoteTimerRef.current = setTimeout(() => {
        computeQuote("input", amount);
        setQuoteLoading(false);
      }, QUOTE_DEBOUNCE_MS);
    },
    [computeQuote, clearTerminalStatus],
  );

  const setOutputAmount = useCallback(
    (amount: string) => {
      clearTerminalStatus();
      setOutputAmountRaw(amount);
      setEditingField("output");

      if (quoteTimerRef.current) {
        clearTimeout(quoteTimerRef.current);
      }

      if (!amount || parseFloat(amount) <= 0) {
        setInputAmountRaw("");
        setQuote(null);
        setQuoteLoading(false);
        return;
      }

      setQuoteLoading(true);
      quoteTimerRef.current = setTimeout(() => {
        computeQuote("output", amount);
        setQuoteLoading(false);
      }, QUOTE_DEBOUNCE_MS);
    },
    [computeQuote, clearTerminalStatus],
  );

  // ==========================================================================
  // Flip tokens
  // ==========================================================================

  const flipTokens = useCallback(() => {
    clearTerminalStatus();
    const newInput = outputToken;
    const newOutput = inputToken;

    // Check if the flipped pair is valid
    if (!VALID_PAIRS[newInput]?.includes(newOutput)) {
      // If not valid, just swap the tokens and clear amounts
      setInputTokenRaw(newInput);
      setOutputTokenRaw(VALID_PAIRS[newInput][0]);
      setInputAmountRaw("");
      setOutputAmountRaw("");
      setQuote(null);
      return;
    }

    // Swap tokens and amounts for natural UX
    setInputTokenRaw(newInput);
    setOutputTokenRaw(newOutput);
    const prevInput = inputAmount;
    const prevOutput = outputAmount;
    setInputAmountRaw(prevOutput);
    setOutputAmountRaw(prevInput);
    setQuote(null);

    // Recompute quote with new direction if there's a value
    if (prevOutput && parseFloat(prevOutput) > 0) {
      setQuoteLoading(true);
      if (quoteTimerRef.current) clearTimeout(quoteTimerRef.current);
      quoteTimerRef.current = setTimeout(() => {
        // After flip: user's "input" is what was the "output"
        // We need to call computeQuote but it reads state which hasn't updated yet.
        // Instead, re-trigger via setInputAmount on next tick.
        setQuoteLoading(false);
      }, QUOTE_DEBOUNCE_MS);
    }
  }, [inputToken, outputToken, inputAmount, outputAmount, clearTerminalStatus]);

  // Re-quote when tokens change (with existing amount)
  useEffect(() => {
    if (editingField === "input" && inputAmount && parseFloat(inputAmount) > 0) {
      setQuoteLoading(true);
      if (quoteTimerRef.current) clearTimeout(quoteTimerRef.current);
      quoteTimerRef.current = setTimeout(() => {
        computeQuote("input", inputAmount);
        setQuoteLoading(false);
      }, QUOTE_DEBOUNCE_MS);
    } else if (editingField === "output" && outputAmount && parseFloat(outputAmount) > 0) {
      setQuoteLoading(true);
      if (quoteTimerRef.current) clearTimeout(quoteTimerRef.current);
      quoteTimerRef.current = setTimeout(() => {
        computeQuote("output", outputAmount);
        setQuoteLoading(false);
      }, QUOTE_DEBOUNCE_MS);
    }
    // Only re-quote when pool data or epoch state updates
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [pools, epochState, slippageBps]);

  // Cleanup timers on unmount
  useEffect(() => {
    return () => {
      if (quoteTimerRef.current) clearTimeout(quoteTimerRef.current);
      if (autoResetTimerRef.current) clearTimeout(autoResetTimerRef.current);
    };
  }, []);

  // ==========================================================================
  // Reset form (defined early: used by both executeSwap and executeRoute)
  // ==========================================================================

  const resetForm = useCallback(() => {
    setInputAmountRaw("");
    setOutputAmountRaw("");
    setQuote(null);
    setStatus("idle");
    setTxSignature(null);
    setErrorMessage(null);
    setQuoteLoading(false);
    if (autoResetTimerRef.current) {
      clearTimeout(autoResetTimerRef.current);
      autoResetTimerRef.current = null;
    }
  }, []);

  // ==========================================================================
  // Execute swap (direct, single-pool path)
  // ==========================================================================

  const executeSwap = useCallback(async () => {
    if (!wallet.publicKey || !wallet.connected || !quote) {
      return;
    }

    const connection = getConnection();
    const poolConfig = resolvePool(inputToken, outputToken);

    try {
      // 1. Build transaction
      setStatus("building");
      setErrorMessage(null);
      setTxSignature(null);

      const isCrime = inputToken === "CRIME" || outputToken === "CRIME";
      const priorityMicroLamports = PRIORITY_FEE_MAP[priorityFeePreset];

      let tx;

      if (!poolConfig) {
        // Not an AMM pool — check for vault conversion
        const routeConfig = resolveRoute(inputToken, outputToken);
        if (routeConfig && routeConfig.type === "vaultConvert") {
          const amountInBaseUnits = toBaseUnits(inputAmount, inputToken);
          tx = await buildVaultConvertTransaction({
            connection,
            userPublicKey: wallet.publicKey,
            amountInBaseUnits,
            minimumOutput: quote.minimumOutput,
            inputMint: routeConfig.inputMint,
            outputMint: routeConfig.outputMint,
            priorityFeeMicroLamports: priorityMicroLamports,
          });
        } else {
          // Multi-hop — handled by executeRoute, not executeSwap
          return;
        }
      } else {
        switch (poolConfig.instruction) {
          case "swapSolBuy": {
            const amountInLamports = toBaseUnits(inputAmount, inputToken);
            const buyTaxBps = isCrime
              ? epochState!.crimeBuyTaxBps
              : epochState!.fraudBuyTaxBps;
            tx = await buildSolBuyTransaction({
              connection,
              userPublicKey: wallet.publicKey,
              amountInLamports,
              minimumOutput: quote.minimumOutput,
              isCrime,
              taxBps: buyTaxBps,
              priorityFeeMicroLamports: priorityMicroLamports,
            });
            break;
          }
          case "swapSolSell": {
            const amountInBaseUnits = toBaseUnits(inputAmount, inputToken);
            tx = await buildSolSellTransaction({
              connection,
              userPublicKey: wallet.publicKey,
              amountInBaseUnits,
              minimumOutput: quote.minimumOutput,
              isCrime,
              priorityFeeMicroLamports: priorityMicroLamports,
            });
            break;
          }
        }
      }

      // 2. Compile to v0 VersionedTransaction for taxed swaps (Phantom simulation fix),
      //    or set blockhash/feePayer for legacy vault convert transactions.
      let finalTx: Transaction | import("@solana/web3.js").VersionedTransaction;
      let lastValidBlockHeight: number;

      if (poolConfig && tx instanceof Transaction) {
        // Taxed swap (SOL buy/sell): compile to v0 with ALT for Phantom compatibility
        const compiled = await compileToVersionedTransaction(tx, connection, wallet.publicKey);
        finalTx = compiled.transaction;
        lastValidBlockHeight = compiled.lastValidBlockHeight;
      } else if (tx instanceof Transaction) {
        // Vault convert: legacy TX, set blockhash + feePayer
        const latest = await connection.getLatestBlockhash("confirmed");
        tx.recentBlockhash = latest.blockhash;
        tx.feePayer = wallet.publicKey;
        finalTx = tx;
        lastValidBlockHeight = latest.lastValidBlockHeight;
      } else {
        // Shouldn't happen, but handle gracefully
        const latest = await connection.getLatestBlockhash("confirmed");
        finalTx = tx;
        lastValidBlockHeight = latest.lastValidBlockHeight;
      }

      // 3. Sign and send transaction (single wallet prompt, Blowfish-compatible)
      setStatus("signing");
      const signature = await wallet.sendTransaction(finalTx, connection, {
        skipPreflight: false,
        maxRetries: 2,
      });
      setTxSignature(signature);

      // 5. Confirm transaction (HTTP polling — more reliable than websocket)
      setStatus("confirming");
      const confirmation = await pollTransactionConfirmation(
        connection,
        signature,
        lastValidBlockHeight,
      );

      // 6. Check for errors
      if (confirmation.err) {
        setStatus("failed");
        setErrorMessage(parseSwapError(confirmation.err));
        return;
      }

      // 7. Success
      setStatus("confirmed");

      // 8. Refresh token balances
      refreshBalances();

      // 9. Auto-reset after 10 seconds
      autoResetTimerRef.current = setTimeout(() => {
        resetForm();
      }, AUTO_RESET_MS);
    } catch (error) {
      console.error("[useSwap] executeSwap error:", error);
      setStatus("failed");
      setErrorMessage(parseSwapError(error));
    }
  }, [
    wallet,
    quote,
    inputToken,
    outputToken,
    inputAmount,
    priorityFeePreset,
    toBaseUnits,
    refreshBalances,
    epochState,
  ]);

  // ==========================================================================
  // Smart routing: sync output amount from selected route
  // ==========================================================================

  useEffect(() => {
    if (smartRouting && routesResult.selectedRoute) {
      const route = routesResult.selectedRoute;
      const decimals = outputToken === "SOL" ? SOL_DECIMALS : TOKEN_DECIMALS;
      const value = route.outputAmount / 10 ** decimals;
      const displayStr = route.outputAmount > 0
        ? value.toFixed(decimals).replace(/\.?0+$/, "")
        : "";
      setOutputAmountRaw(displayStr);
      setQuote({
        outputAmount: route.outputAmount,
        lpFee: route.totalLpFee,
        taxAmount: route.totalTax,
        priceImpactBps: route.totalPriceImpactBps,
        minimumOutput: route.minimumOutput,
        totalFeePct: route.totalFeePct,
      });
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [smartRouting, routesResult.selectedRoute]);

  // ==========================================================================
  // Execute route (smart routing aware)
  // ==========================================================================

  const executeRoute = useCallback(async () => {
    // If smart routing is OFF or no route selected, fallback to direct swap
    if (!smartRouting || !routesResult.selectedRoute) {
      return executeSwap();
    }

    const route = routesResult.selectedRoute;

    // Single-hop non-split route: use existing direct swap path
    if (route.hops === 1 && !route.isSplit) {
      return executeSwap();
    }

    // Multi-hop or split: atomic v0 transaction (single signature, all-or-nothing)
    if (!wallet.publicKey || !wallet.connected) return;

    const connection = getConnection();

    try {
      setStatus("building");
      setErrorMessage(null);
      setTxSignature(null);

      const priorityMicroLamports = PRIORITY_FEE_MAP[priorityFeePreset];

      // Build one atomic v0 transaction containing ALL steps
      // (works for 2-step multi-hop AND 4-step split routes)
      const build = await buildAtomicRoute(
        route,
        connection,
        wallet.publicKey,
        priorityMicroLamports,
      );

      // Sign (single wallet prompt) and send
      setStatus("signing");
      const result = await executeAtomicRoute(build, wallet, connection);

      if (result.status === "confirmed") {
        setStatus("confirmed");
        setTxSignature(result.signatures[0] || null);
        refreshBalances();
        autoResetTimerRef.current = setTimeout(resetForm, AUTO_RESET_MS);
      } else {
        setStatus("failed");
        setTxSignature(result.signatures[0] || null);
        setErrorMessage(result.error || "Swap failed");
      }
    } catch (error) {
      console.error("[useSwap] executeRoute error:", error);
      setStatus("failed");
      setErrorMessage(parseSwapError(error));
    }
  }, [
    smartRouting,
    routesResult.selectedRoute,
    wallet,
    priorityFeePreset,
    executeSwap,
    refreshBalances,
    resetForm,
  ]);

  // ==========================================================================
  // Return
  // ==========================================================================

  return {
    // Form state
    inputToken,
    outputToken,
    inputAmount,
    outputAmount,
    setInputToken,
    setOutputToken,
    setInputAmount,
    setOutputAmount,
    flipTokens,

    // Quote
    quote,
    quoteLoading,

    // Config
    slippageBps,
    setSlippageBps,
    priorityFeePreset,
    setPriorityFeePreset,

    // Execution
    executeSwap,
    executeRoute,
    status,
    txSignature,
    errorMessage,

    // Smart routing
    smartRouting,
    setSmartRouting,
    routes: routesResult.routes,
    selectedRoute: routesResult.selectedRoute,
    selectRoute: routesResult.selectRoute,
    routesLoading: routesResult.routesLoading,
    refreshCountdown: routesResult.refreshCountdown,

    // Wallet
    connected: wallet.connected,

    // Balances
    balances: { sol, crime, fraud, profit },
    balancesLoading,

    // Reset
    resetForm,
  };
}
