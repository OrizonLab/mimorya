import Link from 'next/link';
import { LabelBadge } from '@/components/ui';
import type { InboundHook } from '@/lib/api';
import { formatDate } from '@/lib/format';
import { DELIVERY_OUTCOME } from '@/lib/labels';

/**
 * What became of a hook's last delivery, and why nothing started, as the
 * platform says it to the hook's author alone; the conversation of the last
 * run one started, which may await a validation (ADR-109).
 */
export function LastDelivery({ hook }: { hook: InboundHook }) {
  const last = hook.lastDelivery;
  if (!last && !hook.lastConversationId) {
    return <p className="text-xs text-muted">Aucun envoi reçu pour l’instant.</p>;
  }
  return (
    <div className="flex flex-col gap-1 text-xs text-muted">
      {last && (
        <p className="flex flex-wrap items-center gap-2">
          Dernier envoi, le {formatDate(last.at)} :
          <LabelBadge labels={DELIVERY_OUTCOME} value={last.outcome} />
        </p>
      )}
      {last?.problem && <p>{last.problem}</p>}
      {hook.lastConversationId && (
        <Link className="w-fit underline" href={`/chat/${hook.lastConversationId}`}>
          Ouvrir la conversation du dernier envoi lancé
        </Link>
      )}
    </div>
  );
}
