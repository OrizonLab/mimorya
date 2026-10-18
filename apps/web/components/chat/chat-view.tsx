'use client';

import { MessageSquarePlus, PanelLeft, Send } from 'lucide-react';
import Link from 'next/link';
import { useRouter } from 'next/navigation';
import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type FormEvent,
  type KeyboardEvent,
} from 'react';
import {
  api,
  ApiError,
  type Approval,
  type ConversationSummary,
  type Message,
  type RunStatus,
} from '@/lib/api';
import { useMe } from '@/lib/me-context';
import { Button, cx, ErrorNote, Spinner } from '../ui';
import { ActivityPanel } from './activity-panel';
import { ApprovalCard } from './approval-card';
import { MessageItem } from './message-item';
import { useRunStream } from './use-run-stream';

interface ConversationDetail {
  conversation: ConversationSummary;
  messages: Message[];
  runs: Array<{ id: string; status: RunStatus; createdAt: string }>;
}

const SUGGESTIONS = [
  'Analyse les comptes rendus des interventions de cette semaine, identifie les interventions non terminées, prépare une synthèse et propose un planning de suivi pour mon équipe.',
  'Quelles interventions sont encore en cours et pourquoi ?',
  'Retiens que je préfère des synthèses courtes, sous forme de tableau.',
];

export function ChatView({ conversationId }: { conversationId: string | null }) {
  const me = useMe();
  const router = useRouter();
  const [conversations, setConversations] = useState<ConversationSummary[] | null>(null);
  const [detail, setDetail] = useState<ConversationDetail | null>(null);
  const [activeRunId, setActiveRunId] = useState<string | null>(null);
  const [approvals, setApprovals] = useState<Approval[]>([]);
  const [input, setInput] = useState('');
  const [sending, setSending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [listOpen, setListOpen] = useState(false);
  const bottomRef = useRef<HTMLDivElement>(null);

  const loadConversations = useCallback(() => {
    api
      .get<{ conversations: ConversationSummary[] }>('/conversations')
      .then((r) => setConversations(r.conversations))
      .catch(() => setConversations([]));
  }, []);

  const loadApprovals = useCallback(async (runId: string) => {
    const r = await api.get<{ approvals: Approval[] }>('/approvals');
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
      setError(
        e instanceof ApiError && e.status === 404
          ? 'Conversation introuvable'
          : 'Chargement impossible',
      );
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
        void loadApprovals(activeRunId);
        return;
      }
      void loadDetail();
      loadConversations();
    },
    [activeRunId, loadApprovals, loadDetail, loadConversations],
  );
  const activity = useRunStream(activeRunId, onRunFinished);

  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: 'smooth', block: 'end' });
  }, [detail?.messages.length, activity.steps.length, activity.draft, approvals.length]);

  async function send(text: string) {
    const content = text.trim();
    if (!content || sending) return;
    setSending(true);
    setError(null);
    try {
      let id = conversationId;
      if (!id) {
        const created = await api.post<{ conversation: ConversationSummary }>('/conversations');
        id = created.conversation.id;
      }
      const started = await api.post<{ runId: string }>(`/conversations/${id}/messages`, {
        content,
      });
      setInput('');
      if (id !== conversationId) {
        router.push(`/chat/${id}`);
        return;
      }
      setDetail((d) =>
        d
          ? {
              ...d,
              messages: [
                ...d.messages,
                {
                  id: `local-${Date.now()}`,
                  role: 'user',
                  content,
                  runId: started.runId,
                  createdAt: new Date().toISOString(),
                  metadata: {},
                },
              ],
            }
          : d,
      );
      setActiveRunId(started.runId);
      setApprovals([]);
    } catch (e) {
      setError(e instanceof ApiError ? e.message : 'Envoi impossible');
    } finally {
      setSending(false);
    }
  }

  function onSubmit(event: FormEvent) {
    event.preventDefault();
    void send(input);
  }

  function onKeyDown(event: KeyboardEvent<HTMLTextAreaElement>) {
    if (event.key === 'Enter' && !event.shiftKey) {
      event.preventDefault();
      void send(input);
    }
  }

  const busy = activeRunId !== null;
  const pendingApprovals = approvals.filter((a) => a.status === 'pending');

  return (
    <div className="flex min-h-0 flex-1">
      <aside
        className={cx(
          'w-72 shrink-0 flex-col border-r border-line bg-surface',
          listOpen ? 'fixed inset-y-0 left-0 z-40 flex shadow-xl' : 'hidden md:flex',
        )}
      >
        <div className="flex items-center justify-between gap-2 p-3">
          <span className="text-sm font-semibold">Conversations</span>
          <Link
            href="/chat"
            onClick={() => setListOpen(false)}
            className="inline-flex items-center gap-1.5 rounded-lg px-2 py-1 text-sm text-accent hover:bg-accent-soft"
          >
            <MessageSquarePlus className="size-4" aria-hidden /> Nouvelle
          </Link>
        </div>
        <ul className="flex-1 overflow-y-auto px-2 pb-3">
          {conversations === null && <Spinner />}
          {conversations?.length === 0 && (
            <li className="px-2 py-4 text-sm text-muted">Aucune conversation pour le moment.</li>
          )}
          {conversations?.map((c) => (
            <li key={c.id}>
              <Link
                href={`/chat/${c.id}`}
                onClick={() => setListOpen(false)}
                className={cx(
                  'block truncate rounded-lg px-3 py-2 text-sm',
                  c.id === conversationId
                    ? 'bg-accent-soft text-accent'
                    : 'text-muted hover:bg-surface-2 hover:text-fg',
                )}
              >
                {c.title}
              </Link>
            </li>
          ))}
        </ul>
      </aside>
      {listOpen && (
        <button
          type="button"
          aria-label="Fermer la liste"
          className="fixed inset-0 z-30 bg-black/30 md:hidden"
          onClick={() => setListOpen(false)}
        />
      )}

      <section className="flex min-w-0 flex-1 flex-col">
        <div className="flex items-center gap-2 border-b border-line px-4 py-2 md:hidden">
          <button
            type="button"
            onClick={() => setListOpen(true)}
            className="flex items-center gap-2 text-sm text-muted"
          >
            <PanelLeft className="size-4" aria-hidden /> Conversations
          </button>
        </div>
        <div className="flex-1 overflow-y-auto">
          <div className="mx-auto flex max-w-3xl flex-col gap-5 px-4 py-6">
            {!conversationId && (
              <div className="py-8">
                <h1 className="text-2xl font-semibold tracking-tight">
                  Bonjour {me.user.displayName.split(' ')[0]},
                </h1>
                <p className="mt-2 text-muted">
                  Confiez-moi une analyse, une synthèse ou une tâche. J’agis avec vos droits et je
                  vous demande votre accord avant toute action sensible.
                </p>
                {!me.assistant.available && (
                  <div className="mt-4">
                    <ErrorNote message="Aucun modèle d’IA n’est configuré sur ce serveur : les conversations sont désactivées." />
                  </div>
                )}
                <div className="mt-6 grid gap-2">
                  {SUGGESTIONS.map((s) => (
                    <button
                      key={s}
                      type="button"
                      onClick={() => setInput(s)}
                      className="rounded-xl border border-line bg-surface px-4 py-3 text-left text-sm hover:border-accent"
                    >
                      {s}
                    </button>
                  ))}
                </div>
              </div>
            )}
            {conversationId && !detail && !error && <Spinner />}
            {detail?.messages.map((m) => (
              <MessageItem key={m.id} message={m} />
            ))}
            {busy && <ActivityPanel activity={activity} />}
            {pendingApprovals.map((a) => (
              <ApprovalCard
                key={a.id}
                approval={a}
                onDecided={() => setApprovals((list) => list.filter((x) => x.id !== a.id))}
              />
            ))}
            {activity.status === 'awaiting_approval' && pendingApprovals.length === 0 && busy && (
              <p className="text-sm text-muted">Chargement de la demande de validation…</p>
            )}
            {error && <ErrorNote message={error} />}
            <div ref={bottomRef} />
          </div>
        </div>
        <form onSubmit={onSubmit} className="border-t border-line bg-surface px-4 py-3">
          <div className="mx-auto flex max-w-3xl items-end gap-2">
            <label htmlFor="composer" className="sr-only">
              Votre message
            </label>
            <textarea
              id="composer"
              rows={Math.min(6, Math.max(1, input.split('\n').length))}
              value={input}
              onChange={(e) => setInput(e.target.value)}
              onKeyDown={onKeyDown}
              placeholder={busy ? 'Traitement en cours…' : 'Écrivez votre demande…'}
              disabled={busy || !me.assistant.available}
              className="min-h-[2.75rem] flex-1 resize-none rounded-xl border border-line bg-bg px-3 py-2.5 text-[0.95rem] outline-none focus:border-accent disabled:opacity-60"
            />
            <Button
              type="submit"
              loading={sending}
              disabled={busy || !input.trim()}
              aria-label="Envoyer"
            >
              <Send className="size-4" aria-hidden />
            </Button>
          </div>
        </form>
      </section>
    </div>
  );
}
