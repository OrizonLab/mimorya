import type { BrandView } from '@/lib/api';
import { DEFAULT_BRAND } from './brand';

/** The API as this server reaches it, as the relay of `/api/*` does. */
const API_URL = process.env.API_URL ?? 'http://localhost:4000';
/** The brand changes only when the API restarts: read again after a minute. */
const FRESH_MS = 60_000;

let kept: { brand: BrandView; at: number } | null = null;

/**
 * The installation's brand, as the API serves it (ADR-222), read by this
 * server for the pages it renders. If the API does not answer (restarting),
 * the brand last read, or the platform's own, and asked again next time.
 * Server side only: the browser never reads it on its own.
 */
export async function readBrand(): Promise<BrandView> {
  if (kept && Date.now() - kept.at < FRESH_MS) return kept.brand;
  try {
    const response = await fetch(`${API_URL}/brand`, {
      cache: 'no-store',
      signal: AbortSignal.timeout(2_000),
    });
    if (response.ok) {
      kept = { brand: (await response.json()) as BrandView, at: Date.now() };
      return kept.brand;
    }
  } catch {
    // Unreachable for now: what was read before.
  }
  return kept?.brand ?? DEFAULT_BRAND;
}
