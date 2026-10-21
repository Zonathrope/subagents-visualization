import type { TranscriptStats } from './types'

/**
 * First-party API list prices, USD per million tokens (input / output / cache read),
 * as of 2026-10. Cache writes use the standard multipliers on input: 1.25x for the
 * 5-minute cache, 2x for the 1-hour cache. Shown only as an estimate: subscriptions,
 * partner platforms, fast mode and long-prompt tiers are billed differently.
 */
const PRICES: Record<string, { input: number; output: number; cacheRead: number }> = {
  'claude-fable-5-1': { input: 10, output: 50, cacheRead: 0.25 },
  'claude-fable-5': { input: 10, output: 50, cacheRead: 1 },
  'claude-opus-5-5': { input: 4, output: 20, cacheRead: 0.2 },
  'claude-opus-5': { input: 5, output: 25, cacheRead: 0.5 },
  'claude-opus-4-8': { input: 5, output: 25, cacheRead: 0.5 },
  'claude-opus-4-7': { input: 5, output: 25, cacheRead: 0.5 },
  'claude-opus-4-6': { input: 5, output: 25, cacheRead: 0.5 },
  'claude-sonnet-5-5': { input: 2, output: 10, cacheRead: 0.2 },
  'claude-sonnet-5': { input: 2, output: 10, cacheRead: 0.2 },
  'claude-sonnet-4-6': { input: 3, output: 15, cacheRead: 0.3 },
  'claude-haiku-5-5': { input: 0.1, output: 0.5, cacheRead: 0.01 },
  'claude-haiku-4-5': { input: 1, output: 5, cacheRead: 0.1 },
}

/** Estimated USD for a transcript, or undefined when its model isn't in the table. */
export function estimateCost(stats: TranscriptStats | undefined): number | undefined {
  const model = stats?.model?.replace(/\[.*\]$/, '')
  const p = model ? PRICES[model] : undefined
  if (!stats || !p) return undefined
  const t = stats.tokens
  const usd =
    t.input * p.input +
    t.output * p.output +
    t.cacheRead * p.cacheRead +
    t.cacheWrite5m * p.input * 1.25 +
    t.cacheWrite1h * p.input * 2
  return usd / 1_000_000
}

export function formatUsd(usd: number): string {
  return usd < 0.01 ? '<$0.01' : `$${usd.toFixed(2)}`
}
