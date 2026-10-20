'use client';

import { PanelLeft } from 'lucide-react';
import { useEffect, useRef, useState } from 'react';
import { ApprovalCard } from '@/components/approvals';
import { ErrorNote, Failure, Spinner } from '@/components/ui';
import { useMe } from '@/lib/me-context';
import { ActivityPanel } from './activity-panel';
import { Composer } from './composer';
import { ConversationList } from './conversation-list';
import { MessageItem } from './message-item';
import { useConversation } from './use-conversation';
import { Welcome } from './welcome';

export function ChatView({ conversationId }: { conversationId: string | null }) {
  const me = useMe();
  const chat = useConversation(conversationId);
  const { detail, activity, busy, pendingApprovals } = chat;
  const [input, setInput] = useState('');
  const [listOpen, setListOpen] = useState(false);
  const bottomRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: 'smooth', block: 'end' });
  }, [detail?.messages.length, activity.steps.length, activity.draft, pendingApprovals.length]);

  async function send() {
    if (await chat.send(input)) setInput('');
  }

  return (
    <div className="flex min-h-0 flex-1">
      <ConversationList
        conversations={chat.conversations}
        error={chat.listError}
        onRetry={chat.reloadConversations}
        activeId={conversationId}
        open={listOpen}
        onClose={() => setListOpen(false)}
      />
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
            {!conversationId && <Welcome onPick={setInput} />}
            {conversationId && !detail && !chat.error && <Spinner />}
            {detail?.messages.map((m) => (
              <MessageItem key={m.id} message={m} />
            ))}
            {busy && <ActivityPanel activity={activity} />}
            {busy && activity.lost && (
              <Failure
                message="Le suivi en direct de la demande est interrompu."
                onRetry={chat.reconnectRun}
              />
            )}
            {pendingApprovals.map((a) => (
              <ApprovalCard key={a.id} approval={a} onDecided={() => chat.dismissApproval(a.id)} />
            ))}
            {activity.status === 'awaiting_approval' &&
              pendingApprovals.length === 0 &&
              busy &&
              (chat.approvalsError ? (
                <Failure message={chat.approvalsError} onRetry={chat.reloadApprovals} />
              ) : (
                <p className="text-sm text-muted">Chargement de la demande de validation…</p>
              ))}
            {chat.error && <ErrorNote message={chat.error} />}
            <div ref={bottomRef} />
          </div>
        </div>
        <Composer
          value={input}
          onChange={setInput}
          onSend={() => void send()}
          busy={busy}
          sending={chat.sending}
          disabled={!me.assistant.available}
        />
      </section>
    </div>
  );
}
