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

async function request<T>(method: string, path: string, body?: unknown): Promise<T> {
  const res = await fetch(`/api${path}`, {
    method,
    credentials: 'same-origin',
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
