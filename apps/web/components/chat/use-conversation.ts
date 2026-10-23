'use client';

import { useRouter } from 'next/navigation';
import { useCallback, useEffect, useRef, useState } from 'react';
import {
  api,
  ApiError,
  loadFailure,
  type Approval,
  type ApprovalList,
  type ConversationCreated,
  type ConversationDetail,
  type ConversationList,
  type ConversationSummary,
  type Message,
  type MessageAccepted,
  type NewMessage,
  type RunStatus,
} from '@/lib/api';
import { useRunStream } from './use-run-stream';

/** A run the conversation still follows: waiting for a worker, under way, or paused. */
const ACTIVE: ReadonlySet<RunStatus> = new Set<RunStatus>([
  'queued',
  'running',
  'awaiting_approval',
]);

/**
 * State of the chat screen: the conversation list, the open conversation,
 * its active run (streamed) and the approvals that run is waiting for.
 */
export function useConversation(conversationId: string | null) {
  const router = useRouter();
  const [conversations, setConversations] = useState<ConversationSummary[] | null>(null);
  const [detail, setDetail] = useState<ConversationDetail | null>(null);
  const [activeRunId, setActiveRunId] = useState<string | null>(null);
  const [approvals, setApprovals] = useState<Approval[]>([]);
  const [sending, setSending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [listError, setListError] = useState<string | null>(null);
  const [approvalsError, setApprovalsError] = useState<string | null>(null);

  const loadConversations = useCallback(() => {
    setListError(null);
    api
      .get<ConversationList>('/conversations')
      .then((r) => setConversations(r.conversations))
      // Said as such: an empty list would claim there is no conversation.
      .catch((e: unknown) => setListError(loadFailure(e)));
  }, []);

  // A failure is kept apart: the run waits on these, which can be loaded
  // again. Loaded when the run's stream announces a wait, live or replayed
  // when a waiting conversation is opened: a run that waited more than once
  // replays each wait, so loads may overlap; only the latest one is shown.
  const approvalsLoad = useRef(0);
  const loadApprovals = useCallback(async (runId: string) => {
    const load = ++approvalsLoad.current;
    setApprovalsError(null);
    try {
      const r = await api.get<ApprovalList>('/approvals');
      if (load !== approvalsLoad.current) return;
      setApprovals(r.approvals.filter((a) => a.runId === runId));
    } catch (e) {
      if (load === approvalsLoad.current) setApprovalsError(loadFailure(e));
    }
  }, []);

  const loadDetail = useCallback(async () => {
    if (!conversationId) return;
    try {
      const d = await api.get<ConversationDetail>(`/conversations/${conversationId}`);
      setDetail(d);
      const active = d.runs.find((r) => ACTIVE.has(r.status));
      setActiveRunId(active?.id ?? null);
      // A run waiting for a validation announces it again on its stream, which
      // replays its events: its approvals are loaded then, not twice.
      setApprovals([]);
    } catch (e) {
      setError(
        e instanceof ApiError && e.status === 404 ? 'Conversation introuvable' : loadFailure(e),
      );
    }
  }, [conversationId]);

  useEffect(loadConversations, [loadConversations]);
  useEffect(() => {
    setDetail(null);
    setError(null);
    void loadDetail();
  }, [loadDetail]);

  const onRunFinished = useCallback(
    (status: RunStatus) => {
      if (status === 'awaiting_approval' && activeRunId) {
        void loadApprovals(activeRunId);
        return;
      }
      void loadDetail();
      loadConversations();
    },
    [activeRunId, loadApprovals, loadDetail, loadConversations],
  );
  const { activity, reconnect } = useRunStream(activeRunId, onRunFinished);

  /** Sends a message, creating the conversation first if needed; true once accepted. */
  const send = useCallback(
    async (text: string): Promise<boolean> => {
      const content = text.trim();
      if (!content || sending) return false;
      setSending(true);
      setError(null);
      try {
        let id = conversationId;
        if (!id) {
          const created = await api.post<ConversationCreated>('/conversations');
          id = created.conversation.id;
        }
        const started = await api.post<MessageAccepted>(`/conversations/${id}/messages`, {
          content,
        } satisfies NewMessage);
        if (id !== conversationId) {
          router.push(`/chat/${id}`);
          return true;
        }
        setDetail((d) =>
          d ? { ...d, messages: [...d.messages, localMessage(content, started.runId)] } : d,
        );
        setActiveRunId(started.runId);
        setApprovals([]);
        return true;
      } catch (e) {
        setError(e instanceof ApiError ? e.message : 'Envoi impossible');
        return false;
      } finally {
        setSending(false);
      }
    },
    [conversationId, router, sending],
  );

  const dismissApproval = useCallback(
    (approvalId: string) => setApprovals((list) => list.filter((a) => a.id !== approvalId)),
    [],
  );

  return {
    conversations,
    listError,
    reloadConversations: loadConversations,
    detail,
    error,
    approvalsError,
    reloadApprovals: () => void (activeRunId && loadApprovals(activeRunId)),
    sending,
    busy: activeRunId !== null,
    activity,
    reconnectRun: reconnect,
    pendingApprovals: approvals.filter((a) => a.status === 'pending'),
    send,
    dismissApproval,
  };
}

/** Optimistic copy of the user's message until the conversation is reloaded. */
function localMessage(content: string, runId: string): Message {
  return {
    id: `local-${Date.now()}`,
    role: 'user',
    content,
    runId,
    createdAt: new Date().toISOString(),
    metadata: {},
  };
}
