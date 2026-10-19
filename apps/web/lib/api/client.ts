/** Thin client for the API (relayed under /api by Next.js). */
import type { ErrorBody } from './contract';

export class ApiError extends Error {
  constructor(
    readonly status: number,
    readonly code: string,
    message: string,
  ) {
    super(message);
  }
}

/** What a failed load says: the API's message (a timeout's included), or a general one. */
export function loadFailure(error: unknown): string {
  return error instanceof ApiError ? error.message : 'Chargement impossible';
}

/**
 * How long a read may wait for its answer: past it, it fails rather than
 * leave its screen locked (ADR-042, ADR-044). Changes are not cut short: an
 * answer may still be on its way after the server applied them.
 */
export const READ_TIMEOUT_MS = 30_000;

const NO_ANSWER = 'Le serveur ne répond pas. Réessayez dans un instant.';

async function request<T>(method: string, path: string, body?: unknown): Promise<T> {
  const read = method === 'GET';
  const controller = new AbortController();
  // A timer rather than AbortSignal.timeout: the page's clock drives it.
  const timer = read ? setTimeout(() => controller.abort(), READ_TIMEOUT_MS) : undefined;
  try {
    return await send<T>(method, path, body, controller.signal);
  } catch (error) {
    if (controller.signal.aborted) throw new ApiError(0, 'timeout', NO_ANSWER);
    throw error;
  } finally {
    clearTimeout(timer);
  }
}

async function send<T>(
  method: string,
  path: string,
  body: unknown,
  signal: AbortSignal,
): Promise<T> {
  const res = await fetch(`/api${path}`, {
    method,
    credentials: 'same-origin',
    signal,
    headers: {
      ...(body !== undefined ? { 'Content-Type': 'application/json' } : {}),
      // Required by the API's CSRF protection on state-changing requests.
      ...(method !== 'GET' ? { 'X-Mimorya-Request': '1' } : {}),
    },
    ...(body !== undefined ? { body: JSON.stringify(body) } : {}),
  });
  if (!res.ok) {
    // The API's error body, or anything else (a proxy's page, an empty body).
    const payload = (await res.json().catch(() => null)) as Partial<ErrorBody> | null;
    throw new ApiError(
      res.status,
      payload?.error?.code ?? 'error',
      payload?.error?.message ?? `Erreur ${res.status}`,
    );
  }
  return (await res.json()) as T;
}

export const api = {
  get: <T>(path: string) => request<T>('GET', path),
  post: <T>(path: string, body: unknown = {}) => request<T>('POST', path, body),
  patch: <T>(path: string, body: unknown) => request<T>('PATCH', path, body),
  put: <T>(path: string, body: unknown) => request<T>('PUT', path, body),
  delete: <T>(path: string) => request<T>('DELETE', path),
};
