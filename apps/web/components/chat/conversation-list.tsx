import { MessageSquarePlus } from 'lucide-react';
import Link from 'next/link';
import { cx, Spinner } from '@/components/ui';
import type { ConversationSummary } from '@/lib/api';

/** Conversation list: a column on large screens, a drawer on small ones. */
export function ConversationList({
  conversations,
  activeId,
  open,
  onClose,
}: {
  conversations: ConversationSummary[] | null;
  activeId: string | null;
  open: boolean;
  onClose: () => void;
}) {
  return (
    <>
      <aside
        className={cx(
          'w-72 shrink-0 flex-col border-r border-line bg-surface',
          open ? 'fixed inset-y-0 left-0 z-40 flex shadow-xl' : 'hidden md:flex',
        )}
      >
        <div className="flex items-center justify-between gap-2 p-3">
          <span className="text-sm font-semibold">Conversations</span>
          <Link
            href="/chat"
            onClick={onClose}
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
                onClick={onClose}
                className={cx(
                  'block truncate rounded-lg px-3 py-2 text-sm',
                  c.id === activeId
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
      {open && (
        <button
          type="button"
          aria-label="Fermer la liste"
          className="fixed inset-0 z-30 bg-black/30 md:hidden"
          onClick={onClose}
        />
      )}
    </>
  );
}
