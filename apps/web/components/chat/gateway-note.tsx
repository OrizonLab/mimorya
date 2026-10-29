import { MessagesSquare } from 'lucide-react';
import { gatewayLabel } from '@/lib/labels';

/**
 * Where else a conversation is held (ADR-127, ADR-128, ADR-135): in a
 * thread of the company's Slack application, or in its author's private
 * chat with the company's Telegram bot while it is the one open there.
 * Their messages written there are answered there while the gateway is
 * enabled and their account linked; what is written in the application is
 * answered here only.
 */
export function GatewayNote({ gateway }: { gateway: string }) {
  const service = gatewayLabel(gateway);
  const where =
    gateway === 'telegram'
      ? 'dans votre conversation privée avec le bot Telegram de l’entreprise, tant que vous n’y en ouvrez pas une nouvelle'
      : `dans un fil ${service}`;
  return (
    <p
      role="note"
      className="flex items-start gap-2 rounded-lg border border-line bg-surface-2 px-3 py-2 text-xs text-muted"
    >
      <MessagesSquare className="mt-0.5 size-3.5 shrink-0" aria-hidden />
      <span>
        Cette conversation se tient aussi {where} : tant que la passerelle de l’entreprise est
        active et votre compte {service} relié, vos messages écrits dans {service} y reçoivent leur
        réponse. Ce que vous écrivez ici reçoit sa réponse ici seulement.
      </span>
    </p>
  );
}
