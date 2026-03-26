/**
 * Telegram Alert Module -- Zero-dependency, best-effort notifications.
 *
 * Sends push notifications to a Telegram chat when the crank circuit
 * breaker trips. Uses raw fetch() to POST to the Telegram Bot API,
 * matching the project's zero-dependency pattern (same approach as
 * lib/sentry.ts for error reporting).
 *
 * Guarantees:
 * - NEVER throws -- all errors are caught and logged.
 * - NEVER blocks the crank -- failures return false, not exceptions.
 * - NEVER logs the bot token -- URLs with tokens are never printed.
 * - Gracefully disables when env vars are missing (log + return false).
 * - 5-minute cooldown between alerts to prevent spam during restarts.
 *
 * Uses HTML parse_mode (not MarkdownV2) to avoid Telegram's aggressive
 * special-character escaping requirements.
 */

// ---- Types ----

export interface AlertContext {
  event: string;              // e.g. "CIRCUIT BREAKER TRIPPED"
  lastError: string;          // Last error message (will be truncated)
  epoch: number;              // Current epoch number
  walletBalanceSol: number;   // Wallet balance in SOL
  consecutiveErrors: number;  // Error count that triggered the alert
  uptimeSeconds: number;      // Crank process uptime
}

// ---- Cooldown ----

/** 5-minute cooldown between alerts to prevent duplicate spam. */
const ALERT_COOLDOWN_MS = 5 * 60 * 1000;

let lastAlertTs = 0;

// ---- Helpers ----

/**
 * Escape HTML special characters for safe embedding in Telegram HTML messages.
 * Telegram's HTML parse_mode requires &, <, > to be escaped.
 */
function escapeHtml(text: string): string {
  return text
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;");
}

/**
 * Format uptime seconds into a human-readable "Xh Ym" string.
 */
function formatUptime(seconds: number): string {
  const hours = Math.floor(seconds / 3600);
  const minutes = Math.floor((seconds % 3600) / 60);
  return `${hours}h ${minutes}m`;
}

/**
 * Build an HTML-formatted alert message for Telegram.
 * All dynamic content is escaped to prevent HTML injection.
 */
function buildAlertMessage(ctx: AlertContext): string {
  const truncatedError = ctx.lastError.length > 200
    ? ctx.lastError.slice(0, 200) + "..."
    : ctx.lastError;

  return [
    `<b>CRANK ALERT: ${escapeHtml(ctx.event)}</b>`,
    ``,
    `Epoch: <b>${ctx.epoch}</b>`,
    `Balance: <b>${ctx.walletBalanceSol.toFixed(3)} SOL</b>`,
    `Consecutive errors: <b>${ctx.consecutiveErrors}</b>`,
    `Uptime: <b>${formatUptime(ctx.uptimeSeconds)}</b>`,
    ``,
    `Last error:`,
    `<code>${escapeHtml(truncatedError)}</code>`,
  ].join("\n");
}

// ---- Main Export ----

/**
 * Send a Telegram alert. Returns true if the message was sent (HTTP 2xx),
 * false otherwise. NEVER throws.
 *
 * @param ctx - Alert context with event details
 * @returns true if sent successfully, false if skipped/failed
 */
export async function sendAlert(ctx: AlertContext): Promise<boolean> {
  try {
    // Cooldown check
    if (Date.now() - lastAlertTs < ALERT_COOLDOWN_MS) {
      console.log("[alert] Cooldown active, skipping Telegram alert");
      return false;
    }

    // Read env vars
    const botToken = process.env.TELEGRAM_BOT_TOKEN;
    const chatId = process.env.TELEGRAM_CHAT_ID;

    if (!botToken || !chatId) {
      console.log("[alert] TELEGRAM_BOT_TOKEN or TELEGRAM_CHAT_ID not set, skipping alert");
      return false;
    }

    // Build and send message
    const message = buildAlertMessage(ctx);
    const url = `https://api.telegram.org/bot${botToken}/sendMessage`;

    const res = await fetch(url, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        chat_id: chatId,
        text: message,
        parse_mode: "HTML",
      }),
    });

    // Update cooldown timestamp regardless of response status
    // (prevents hammering the API on repeated failures)
    lastAlertTs = Date.now();

    if (res.ok) {
      console.log("[alert] Telegram alert sent successfully");
      return true;
    }

    console.log(`[alert] WARNING: Telegram API returned ${res.status}`);
    return false;
  } catch (err) {
    // Update cooldown even on fetch errors to prevent rapid retries
    lastAlertTs = Date.now();

    const errMsg = String(err).slice(0, 100);
    console.log(`[alert] WARNING: Telegram send failed: ${errMsg}`);
    return false;
  }
}
