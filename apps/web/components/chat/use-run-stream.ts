'use client';

import { useEffect, useRef, useState } from 'react';
import type { RunStatus } from '@/lib/api';

export interface ActivityStep {
  toolCallId: string;
  toolName: string;
  title: string;
  decision: 'allow' | 'require_approval' | 'deny';
  reason?: string;
  summary?: string;
  isError?: boolean;
  done: boolean;
}

export interface PendingApprovalView {
  approvalId: string;
  toolName: string;
  actionSummary: string;
  risk: string;
  resolved?: boolean;
}

export interface RunActivity {
  status: RunStatus;
  draft: string;
  notes: string[];
  steps: ActivityStep[];
  approvals: PendingApprovalView[];
  verification: { attempt: number; passed: boolean; issues: string[] } | null;
  error: string | null;
}

const INITIAL: RunActivity = {
  status: 'running',
  draft: '',
  notes: [],
  steps: [],
  approvals: [],
  verification: null,
  error: null,
};

const TERMINAL = new Set(['completed', 'failed', 'cancelled']);

/**
 * Follows a run through server-sent events. The browser reconnects on its own
 * after network errors (resuming from the last event id); the stream is closed
 * client-side once the run reaches a terminal state.
 */
export function useRunStream(runId: string | null, onFinished: (status: RunStatus) => void) {
  const [activity, setActivity] = useState<RunActivity>(INITIAL);
  const finishedRef = useRef(onFinished);
  finishedRef.current = onFinished;

  useEffect(() => {
    if (!runId) return;
    setActivity(INITIAL);
    const source = new EventSource(`/api/runs/${runId}/events`);
    const on = <T>(type: string, handler: (data: T) => void) =>
      source.addEventListener(type, (event) =>
        handler(JSON.parse((event as MessageEvent).data) as T),
      );

    on<{ text: string }>('assistant_delta', ({ text }) =>
      setActivity((a) => ({ ...a, draft: a.draft + text })),
    );
    on<{ text: string; final: boolean }>('assistant_message', ({ text, final }) =>
      setActivity((a) =>
        final ? { ...a, draft: '' } : { ...a, draft: '', notes: [...a.notes, text] },
      ),
    );
    on<Omit<ActivityStep, 'done'>>('tool_call', (step) =>
      setActivity((a) => ({
        ...a,
        draft: '',
        status: 'running',
        steps: [...a.steps, { ...step, done: step.decision === 'deny' }],
      })),
    );
    on<{ toolCallId: string; summary: string; isError: boolean }>('tool_result', (r) =>
      setActivity((a) => ({
        ...a,
        steps: a.steps.map((s) =>
          s.toolCallId === r.toolCallId
            ? { ...s, summary: r.summary, isError: r.isError, done: true }
            : s,
        ),
      })),
    );
    on<PendingApprovalView>('approval_requested', (approval) =>
      setActivity((a) => ({ ...a, approvals: [...a.approvals, approval] })),
    );
    on<{ approvalId: string }>('approval_resolved', ({ approvalId }) =>
      setActivity((a) => ({
        ...a,
        status: 'running',
        approvals: a.approvals.map((p) =>
          p.approvalId === approvalId ? { ...p, resolved: true } : p,
        ),
      })),
    );
    on<RunActivity['verification']>('verification', (verification) =>
      setActivity((a) => ({ ...a, verification })),
    );
    on<{ status: RunStatus; error?: string }>('run_finished', ({ status, error }) => {
      setActivity((a) => ({ ...a, status, error: error ?? null, draft: '' }));
      // Closing first prevents the browser from reconnecting to an ended stream.
      if (TERMINAL.has(status)) source.close();
      finishedRef.current(status);
    });

    return () => source.close();
  }, [runId]);

  return activity;
}
