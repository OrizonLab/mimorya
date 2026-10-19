'use client';

import { useRouter } from 'next/navigation';
import { useCallback, useEffect, useState } from 'react';
import {
  api,
  ApiError,
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

  const loadConversations = useCallback(() => {
    setListError(null);
    api
      .get<ConversationList>('/conversations')
      .then((r) => setConversations(r.conversations))
      // Said as such: an empty list would claim there is no conversation.
      .catch((e: unknown) => setListError(failure(e)));
  }, []);

  const loadApprovals = useCallback(async (runId: string) => {
    const r = await api.get<ApprovalList>('/approvals');
    setApprovals(r.approvals.filter((a) => a.runId === runId));
  }, []);

  const loadDetail = useCallback(async () => {
    if (!conversationId) return;
    try {
      const d = await api.get<ConversationDetail>(`/conversations/${conversationId}`);
      setDetail(d);
      const active = d.runs.find((r) => r.status === 'running' || r.status === 'awaiting_approval');
      setActiveRunId(active?.id ?? null);
      if (active?.status === 'awaiting_approval') await loadApprovals(active.id);
      else setApprovals([]);
    } catch (e) {
      setError(e instanceof ApiError && e.status === 404 ? 'Conversation introuvable' : failure(e));
    }
  }, [conversationId, loadApprovals]);

  useEffect(loadConversations, [loadConversations]);
  useEffect(() => {
    setDetail(null);
    setError(null);
    void loadDetail();
  }, [loadDetail]);

  const onRunFinished = useCallback(
    (status: RunStatus) => {
      if (status === 'awaiting_approval' && activeRunId) {
        loadApprovals(activeRunId).catch((e: unknown) => setError(failure(e)));
        return;
      }
      void loadDetail();
      loadConversations();
    },
    [activeRunId, loadApprovals, loadDetail, loadConversations],
  );
  const activity = useRunStream(activeRunId, onRunFinished);

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
    sending,
    busy: activeRunId !== null,
    activity,
    pendingApprovals: approvals.filter((a) => a.status === 'pending'),
    send,
    dismissApproval,
  };
}

/** What a failed load says: the API's message (a timeout's included), or a general one. */
function failure(error: unknown): string {
  return error instanceof ApiError ? error.message : 'Chargement impossible';
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
