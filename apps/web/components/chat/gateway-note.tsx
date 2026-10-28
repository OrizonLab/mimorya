import { MessagesSquare } from 'lucide-react';
import { gatewayLabel } from '@/lib/labels';

/**
 * Where else a conversation is held (ADR-127, ADR-128): in a thread of the
 * company's Slack application, where its author's messages follow it and
 * are answered while the gateway is enabled and their Slack account linked;
 * what is written in the application is answered here only.
 */
export function GatewayNote({ gateway }: { gateway: string }) {
  const service = gatewayLabel(gateway);
  return (
    <p
      role="note"
      className="flex items-start gap-2 rounded-lg border border-line bg-surface-2 px-3 py-2 text-xs text-muted"
    >
      <MessagesSquare className="mt-0.5 size-3.5 shrink-0" aria-hidden />
      <span>
        Cette conversation se tient aussi dans un fil {service} : tant que la passerelle de
        l’entreprise est active et votre compte {service} relié, vos messages écrits dans {service}{' '}
        y reçoivent leur réponse. Ce que vous écrivez ici reçoit sa réponse ici seulement.
      </span>
    </p>
  );
}
