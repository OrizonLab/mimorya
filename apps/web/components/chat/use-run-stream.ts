'use client';

import { useCallback, useEffect, useRef, useState } from 'react';
import type {
  ApprovalRequestedEvent,
  RunStatus,
  RunStreamEvent,
  ToolCallEvent,
  VerificationEvent,
} from '@/lib/api';

export type ActivityStep = ToolCallEvent & {
  summary?: string;
  isError?: boolean;
  done: boolean;
  /** Lost to a stop of the service: the run went on from its last saved step (ADR-053). */
  interrupted?: boolean;
};

export type PendingApprovalView = ApprovalRequestedEvent & { resolved?: boolean };

/** The data of the stream's events named `K`. */
type EventData<K extends RunStreamEvent['type']> = Extract<RunStreamEvent, { type: K }>['data'];

export interface RunActivity {
  status: RunStatus;
  draft: string;
  notes: string[];
  steps: ActivityStep[];
  approvals: PendingApprovalView[];
  verification: VerificationEvent | null;
  error: string | null;
  /** Connectors in reach of the user that could not start (ADR-048). */
  unavailable: string[];
  /** Where, among the steps, the run went on after a stop of the service (ADR-053). */
  resumptions: number[];
  /** The browser gave the stream up (an error answered it): it follows nothing more. */
  lost: boolean;
}

const INITIAL: RunActivity = {
  status: 'running',
  draft: '',
  notes: [],
  steps: [],
  approvals: [],
  verification: null,
  error: null,
  unavailable: [],
  resumptions: [],
  lost: false,
};

/** Where a run ends (the server's `RunStatus::FINISHED`). */
const TERMINAL: ReadonlySet<RunStatus> = new Set<RunStatus>(['completed', 'failed', 'cancelled']);

/**
 * Follows a run through server-sent events. The browser reconnects on its own
 * after network errors or a stream that ends (resuming from the last event
 * id); the stream is closed client-side once the run reaches a terminal state.
 * A stream the browser gave up (an error answered it) marks the activity
 * `lost`; `reconnect` opens it again, replayed from the start (ADR-046).
 * A run taken up after a stop of the service marks the steps it lost, and
 * `onResumed` drops what belonged to them (ADR-053).
 */
export function useRunStream(
  runId: string | null,
  onFinished: (status: RunStatus) => void,
  onResumed: () => void,
): { activity: RunActivity; reconnect: () => void } {
  const [activity, setActivity] = useState<RunActivity>(INITIAL);
  const [attempt, setAttempt] = useState(0);
  const finishedRef = useRef(onFinished);
  finishedRef.current = onFinished;
  const resumedRef = useRef(onResumed);
  resumedRef.current = onResumed;

  useEffect(() => {
    if (!runId) return;
    setActivity(INITIAL);
    const source = new EventSource(`/api/runs/${runId}/events`);
    // The browser retries a dropped connection by itself, but gives up on an
    // answer that is not a stream (an HTTP error): say so, as nothing more
    // would come (the approvals the run waits for included).
    source.addEventListener('error', () => {
      if (source.readyState === EventSource.CLOSED) setActivity((a) => ({ ...a, lost: true }));
    });
    const on = <K extends RunStreamEvent['type']>(type: K, handler: (data: EventData<K>) => void) =>
      source.addEventListener(type, (event) =>
        handler(JSON.parse((event as MessageEvent).data) as EventData<K>),
      );

    on('connectors_unavailable', ({ connectors }) =>
      setActivity((a) => ({ ...a, unavailable: connectors })),
    );
    on('assistant_delta', ({ text }) => setActivity((a) => ({ ...a, draft: a.draft + text })));
    on('assistant_message', ({ text, final }) =>
      setActivity((a) =>
        final ? { ...a, draft: '' } : { ...a, draft: '', notes: [...a.notes, text] },
      ),
    );
    on('tool_call', (step) =>
      setActivity((a) => ({
        ...a,
        draft: '',
        status: 'running',
        steps: [...a.steps, { ...step, done: step.decision === 'deny' }],
      })),
    );
    on('tool_result', (r) =>
      setActivity((a) => ({
        ...a,
        steps: a.steps.map((s) =>
          s.toolCallId === r.toolCallId
            ? { ...s, summary: r.summary, isError: r.isError, done: true }
            : s,
        ),
      })),
    );
    on('approval_requested', (approval) =>
      setActivity((a) => ({ ...a, approvals: [...a.approvals, approval] })),
    );
    on('approval_resolved', ({ approvalId }) =>
      setActivity((a) => ({
        ...a,
        status: 'running',
        approvals: a.approvals.map((p) =>
          p.approvalId === approvalId ? { ...p, resolved: true } : p,
        ),
      })),
    );
    on('verification', (verification) => setActivity((a) => ({ ...a, verification })));
    // The steps under way when the service stopped are lost: the run goes on
    // from its last saved step, and asks again what it still needs.
    on('run_resumed', () => {
      setActivity((a) => ({
        ...a,
        status: 'running',
        draft: '',
        steps: a.steps.map((s) => (s.done ? s : { ...s, done: true, interrupted: true })),
        resumptions: [...a.resumptions, a.steps.length],
      }));
      resumedRef.current();
    });
    on('run_finished', ({ status, error }) => {
      setActivity((a) => ({ ...a, status, error: error ?? null, draft: '' }));
      // Closing first prevents the browser from reconnecting to an ended stream.
      if (TERMINAL.has(status)) source.close();
      finishedRef.current(status);
    });

    return () => source.close();
  }, [runId, attempt]);

  // Opened again, the stream replays the run's events from the start.
  const reconnect = useCallback(() => setAttempt((n) => n + 1), []);
  return { activity, reconnect };
}
