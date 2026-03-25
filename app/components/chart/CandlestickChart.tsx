"use client";

/**
 * CandlestickChart -- TradingView Lightweight Charts v5 wrapper
 *
 * Renders an OHLCV candlestick chart with:
 * - Steampunk factory theme via centralized chart-theme.ts constants
 * - Volume histogram overlaid at bottom 20% of chart area
 * - OHLC legend overlay (full on desktop, compact on mobile)
 * - Responsive width via RAF-debounced ResizeObserver
 * - Incremental update optimization: series.update() for single-candle
 *   changes, series.setData() only for full data loads
 * - Logarithmic price scale toggle for memecoin-scale price ranges
 * - Loading overlay when fetching historical data
 *
 * CRITICAL v5 API notes (from RESEARCH.md):
 * - v5 uses chart.addSeries(CandlestickSeries, options) NOT chart.addCandlestickSeries()
 * - v5 is ESM-only (Next.js Turbopack handles this natively)
 * - subscribeCrosshairMove must be unsubscribed BEFORE chart.remove()
 * - Volume visibility toggled via applyOptions, NOT remove/re-add series
 * - ResizeObserver callbacks must be RAF-debounced to prevent loop warnings
 */

import { useEffect, useRef, useState } from "react";
import {
  PriceScaleMode,
  type IChartApi,
  type ISeriesApi,
  type CandlestickData,
  type UTCTimestamp,
  type MouseEventParams,
} from "lightweight-charts";
import type { CandleData } from "@/hooks/useChartData";
import { createThemedChart, toVolumeData } from "./create-chart";
import { FACTORY_VOLUME_COLORS } from "./chart-theme";
import { OhlcLegend } from "./OhlcLegend";

// =============================================================================
// Types
// =============================================================================

interface CandlestickChartProps {
  /** Array of OHLCV candle data (time in unix seconds) */
  candles: CandleData[];
  /** Whether historical data is currently loading */
  loading: boolean;
  /** Chart width -- omit for responsive (fills container) */
  width?: number;
  /** Chart height in pixels. Default: 400 */
  height?: number;
  /** Custom price formatter for Y-axis labels (e.g., USD market cap) */
  priceFormatter?: (price: number) => string;
  /** Show volume histogram overlay at bottom 20% of chart. Default: true */
  showVolume?: boolean;
  /** Use logarithmic price scale. Default: false */
  logScale?: boolean;
  /** Label for aria-label on chart container (e.g., pool name). Default: '' */
  poolLabel?: string;
  /**
   * Callback fired after the chart instance is created.
   * Used by ChartWrapper to enable keyboard navigation (arrow scroll, +/- zoom).
   * Ref pattern avoids re-creating chart when callback identity changes.
   */
  onChartReady?: (chart: IChartApi) => void;
}

// =============================================================================
// Data Conversion
// =============================================================================

/**
 * Convert our CandleData to TradingView's CandlestickData format.
 *
 * TradingView expects time as UTCTimestamp (branded number type = unix seconds).
 * Our CandleData already uses unix seconds, but needs the type assertion
 * because UTCTimestamp is a branded type (not just number).
 */
function toCandlestickData(candle: CandleData): CandlestickData {
  return {
    time: candle.time as UTCTimestamp,
    open: candle.open,
    high: candle.high,
    low: candle.low,
    close: candle.close,
  };
}

// =============================================================================
// Component
// =============================================================================

export function CandlestickChart({
  candles,
  loading,
  width,
  height = 400,
  priceFormatter,
  showVolume = true,
  logScale = false,
  poolLabel = "",
  onChartReady,
}: CandlestickChartProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const chartRef = useRef<IChartApi | null>(null);
  const seriesRef = useRef<ISeriesApi<"Candlestick"> | null>(null);
  const volumeSeriesRef = useRef<ISeriesApi<"Histogram"> | null>(null);

  // Track previous candle count + last candle time for incremental update optimization
  const prevCandleCountRef = useRef(0);
  const prevLastTimeRef = useRef(0);

  // Ref for price formatter so chart creation effect can read latest without re-running
  const priceFormatterRef = useRef(priceFormatter);
  priceFormatterRef.current = priceFormatter;

  // Ref pattern for onChartReady: avoids re-creating the chart when callback identity changes.
  // The ref always holds the latest callback, and the effect calls it after chart creation.
  const onChartReadyRef = useRef(onChartReady);
  onChartReadyRef.current = onChartReady;

  // ── OHLC legend state ───────────────────────────────────────────────────
  const [legendData, setLegendData] = useState<{
    open: number;
    high: number;
    low: number;
    close: number;
  } | null>(null);
  const [legendVolume, setLegendVolume] = useState<number | undefined>(
    undefined,
  );
  const [prevClose, setPrevClose] = useState<number | undefined>(undefined);
  const [isMobile, setIsMobile] = useState(false);

  // ── Mobile detection ──────────────────────────────────────────────────
  // Listens for viewport width changes to toggle compact legend layout.
  // 640px matches Tailwind's 'sm' breakpoint.
  useEffect(() => {
    const mql = window.matchMedia("(max-width: 640px)");
    setIsMobile(mql.matches);

    const handler = (e: MediaQueryListEvent) => setIsMobile(e.matches);
    mql.addEventListener("change", handler);
    return () => mql.removeEventListener("change", handler);
  }, []);

  // ── Chart lifecycle: create on mount, destroy on unmount ────────────────
  useEffect(() => {
    if (!containerRef.current) return;

    // Create fully themed chart with candlestick + volume series
    const { chart, candleSeries, volumeSeries } = createThemedChart(
      containerRef.current,
      {
        height,
        width,
        priceFormatter: priceFormatterRef.current,
        logScale,
      },
    );

    chartRef.current = chart;
    seriesRef.current = candleSeries;
    volumeSeriesRef.current = volumeSeries;

    // Notify parent that chart instance is ready (for keyboard nav in ChartWrapper)
    onChartReadyRef.current?.(chart);

    // ── Crosshair move handler for OHLC legend ────────────────────────
    // When crosshair hovers over a candle, show that candle's OHLCV data.
    // When crosshair leaves, show the latest candle's data.
    const crosshairHandler = (param: MouseEventParams) => {
      if (!param.time) {
        // Crosshair left the chart area -- show latest candle
        const allData = candleSeries.data();
        const latest = allData?.[allData.length - 1];
        if (latest && "open" in latest) {
          setLegendData({
            open: latest.open,
            high: latest.high,
            low: latest.low,
            close: latest.close,
          });
          // Get previous candle's close for change%
          const prev = allData?.[allData.length - 2];
          setPrevClose(
            prev && "close" in prev ? prev.close : undefined,
          );
        }
        // Get volume from volume series
        const volData = volumeSeries.data();
        const latestVol = volData?.[volData.length - 1];
        setLegendVolume(
          latestVol && "value" in latestVol
            ? (latestVol as { value: number }).value
            : undefined,
        );
        return;
      }

      // Crosshair is over a specific candle
      const candleData = param.seriesData.get(candleSeries);
      const volData = param.seriesData.get(volumeSeries);

      if (candleData && "open" in candleData) {
        setLegendData(
          candleData as {
            open: number;
            high: number;
            low: number;
            close: number;
          },
        );
        // Find previous candle for change%
        const allData = candleSeries.data();
        if (allData) {
          const idx = allData.findIndex((d) => d.time === param.time);
          const prev = idx > 0 ? allData[idx - 1] : undefined;
          setPrevClose(
            prev && "close" in prev ? (prev as { close: number }).close : undefined,
          );
        }
      }
      if (volData && "value" in volData) {
        setLegendVolume((volData as { value: number }).value);
      }
    };

    chart.subscribeCrosshairMove(crosshairHandler);

    // ── Responsive resize via RAF-debounced ResizeObserver ─────────────
    // RAF debounce prevents "ResizeObserver loop completed with undelivered
    // notifications" warnings that occur when the observer callback triggers
    // layout recalculation within the same frame.
    let resizeObserver: ResizeObserver | undefined;
    let resizeRAF: number | undefined;

    if (!width && containerRef.current) {
      resizeObserver = new ResizeObserver((entries) => {
        if (resizeRAF) cancelAnimationFrame(resizeRAF);
        resizeRAF = requestAnimationFrame(() => {
          const entry = entries[0];
          if (entry && entry.contentRect.width > 0) {
            chart.applyOptions({ width: entry.contentRect.width });
          }
        });
      });
      resizeObserver.observe(containerRef.current);
    }

    // ── Cleanup on unmount ──────────────────────────────────────────────
    // CRITICAL: unsubscribeCrosshairMove BEFORE chart.remove() to prevent
    // calling handlers on a destroyed chart instance.
    return () => {
      if (resizeRAF) cancelAnimationFrame(resizeRAF);
      resizeObserver?.disconnect();
      chart.unsubscribeCrosshairMove(crosshairHandler);
      chart.remove();
      chartRef.current = null;
      seriesRef.current = null;
      volumeSeriesRef.current = null;
      prevCandleCountRef.current = 0;
      prevLastTimeRef.current = 0;
    };
  }, [width, height, logScale]);

  // ── Price formatter update: apply new formatter without recreating chart ─
  // Handles the transition from undefined -> formatter (when SOL price loads)
  // and any subsequent formatter changes.
  useEffect(() => {
    if (!seriesRef.current || !priceFormatter) return;
    seriesRef.current.applyOptions({
      priceFormat: {
        type: "custom" as const,
        formatter: priceFormatter,
        minMove: 0.000000001,
      },
    });
  }, [priceFormatter]);

  // ── Volume visibility toggle ────────────────────────────────────────────
  // Uses applyOptions({ visible }) instead of remove/re-add series.
  // Removing and re-adding a series causes data loss and is a common pitfall
  // documented in RESEARCH.md Pitfall 6.
  useEffect(() => {
    if (!volumeSeriesRef.current) return;
    volumeSeriesRef.current.applyOptions({ visible: showVolume });
  }, [showVolume]);

  // ── Log scale toggle ────────────────────────────────────────────────────
  // Logarithmic scale is essential for memecoin pools where prices can move
  // orders of magnitude -- linear scale compresses early action to a flat
  // line at the bottom.
  useEffect(() => {
    if (!chartRef.current) return;
    chartRef.current.applyOptions({
      rightPriceScale: {
        mode: logScale
          ? PriceScaleMode.Logarithmic
          : PriceScaleMode.Normal,
      },
    });
  }, [logScale]);

  // ── Data synchronization: update series when candles change ─────────────
  // Separate effect from chart creation so data updates don't recreate the chart.
  useEffect(() => {
    if (!seriesRef.current || !chartRef.current) return;

    // ── Memory hygiene: clear both series when no data ─────────────────
    if (candles.length === 0) {
      seriesRef.current.setData([]);
      volumeSeriesRef.current?.setData([]);
      prevCandleCountRef.current = 0;
      prevLastTimeRef.current = 0;
      return;
    }

    const lastCandle = candles[candles.length - 1];
    const prevCount = prevCandleCountRef.current;
    const prevLastTime = prevLastTimeRef.current;

    // Determine update strategy:
    // 1. Same length, same last time but different data -> series.update() (in-place edit)
    // 2. One more candle than before -> series.update() (append)
    // 3. Significant change (new fetch) -> series.setData() (full reload)
    const isIncrementalUpdate =
      (candles.length === prevCount && lastCandle.time === prevLastTime) ||
      (candles.length === prevCount + 1 && prevCount > 0);

    if (isIncrementalUpdate && prevCount > 0) {
      // Incremental: update just the last candle
      seriesRef.current.update(toCandlestickData(lastCandle));
      // Also update volume series with direction-based coloring
      volumeSeriesRef.current?.update({
        time: lastCandle.time as UTCTimestamp,
        value: lastCandle.volume,
        color:
          lastCandle.close >= lastCandle.open
            ? FACTORY_VOLUME_COLORS.up
            : FACTORY_VOLUME_COLORS.down,
      });
    } else {
      // Full data load: convert all candles and set both series
      const tvData = candles.map(toCandlestickData);
      seriesRef.current.setData(tvData);
      volumeSeriesRef.current?.setData(toVolumeData(candles));
      // Fit content to show all candles after a full data load
      chartRef.current.timeScale().fitContent();
    }

    prevCandleCountRef.current = candles.length;
    prevLastTimeRef.current = lastCandle.time;

    // ── Sync legend state with latest candle ──────────────────────────
    // Without this, switching tokens updates the chart series but leaves
    // the OHLCV header showing stale data from the previous token.
    // The crosshair handler also sets these on hover/leave, but we need
    // an eager update here so the legend is correct immediately.
    setLegendData({
      open: lastCandle.open,
      high: lastCandle.high,
      low: lastCandle.low,
      close: lastCandle.close,
    });
    setLegendVolume(lastCandle.volume);
    const prevCandle = candles.length >= 2 ? candles[candles.length - 2] : undefined;
    setPrevClose(prevCandle?.close);
  }, [candles]);

  return (
    <div className="relative">
      {/* Chart container -- TradingView renders into this div */}
      <div ref={containerRef} className="w-full" />

      {/* OHLC legend overlay -- updates on crosshair hover, shows latest when idle */}
      <OhlcLegend
        data={legendData}
        volume={legendVolume}
        prevClose={prevClose}
        compact={isMobile}
      />
    </div>
  );
}
