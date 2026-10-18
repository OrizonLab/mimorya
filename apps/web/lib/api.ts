/** Thin client for the API (relayed under /api by Next.js). */

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
    const payload = (await res.json().catch(() => null)) as {
      error?: { code?: string; message?: string };
    } | null;
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
  delete: <T>(path: string) => request<T>('DELETE', path),
};

export interface Me {
  user: {
    id: string;
    email: string;
    displayName: string;
    jobTitle: string | null;
    orgUnit: { id: string; path: string } | null;
  };
  tenant: { id: string; name: string; demo: boolean };
  grants: string[];
  permissions: string[];
  assistant: { name: string; available: boolean };
}

export interface ConversationSummary {
  id: string;
  title: string;
  updatedAt: string;
}

export interface Citation {
  type: string;
  id: string;
  label?: string;
}

export interface Message {
  id: string;
  role: 'user' | 'assistant';
  content: string;
  runId: string | null;
  createdAt: string;
  metadata: {
    status?: string;
    evidence?: Citation[];
    verification?: { passed: boolean; issues: string[]; judged: boolean };
  };
}

export type RunStatus =
  'queued' | 'running' | 'awaiting_approval' | 'completed' | 'failed' | 'cancelled';

export interface Approval {
  id: string;
  runId: string;
  toolName: string;
  actionSummary: string;
  payload: unknown;
  risk: string;
  reason: string;
  status: string;
  decisionComment: string | null;
  createdAt: string;
  decidedAt: string | null;
}

export interface RunUsage {
  steps: number;
  toolCalls: number;
  inputTokens: number;
  outputTokens: number;
  costMicros: number;
}

export function formatCost(micros: number): string {
  return `${(micros / 1_000_000).toLocaleString('fr-FR', { style: 'currency', currency: 'USD', minimumFractionDigits: 3 })}`;
}

export function formatDate(value: string | null | undefined): string {
  if (!value) return '—';
  return new Date(value).toLocaleString('fr-FR', { dateStyle: 'short', timeStyle: 'short' });
}
