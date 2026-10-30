'use client';

import type { AccountGatewayView } from '@/lib/api';
import { formatDate } from '@/lib/format';
import { LinkedByMessageRow } from './linked-by-message-row';

/**
 * The signed-in person's Teams account on the company's Teams bot
 * (ADR-138, ADR-139): linked from the link the bot sent them in Teams,
 * and removed here. While the bot is available, the agent answers them
 * there (ADR-140), and they validate on its cards those of their own
 * actions that may be validated outside the application (ADR-141), and
 * those awaiting them as a manager or stand-in, told of there (ADR-142).
 */
export function TeamsIdentityRow({
  gateway,
  onChanged,
}: {
  gateway: AccountGatewayView;
  onChanged: () => void;
}) {
  return (
    <LinkedByMessageRow
      gateway={gateway}
      linked={(identity) => (
        <>
          Votre compte Teams ({identity.address}) est relié depuis le{' '}
          {formatDate(identity.linkedAt)}.
          {gateway.available &&
            ' Vous y parlez à l’agent, qui vous répond dans votre conversation avec le bot ; vous y validez vos propres actions sans effet hors de l’entreprise et réversibles, les autres dans l’application ; responsable, vous y êtes prévenu des demandes qui attendent votre validation.'}
        </>
      )}
      howTo="Pour relier votre compte Teams, écrivez au bot de l’entreprise dans Teams : il vous répond par un lien à ouvrir ici, connecté. Teams doit donner pour votre compte l’adresse de votre compte."
      unlinkQuestion={`Retirer la liaison de votre compte Teams au bot « ${gateway.name} » ? Vous pourrez le relier de nouveau depuis le lien que le bot vous enverra.`}
      onChanged={onChanged}
    />
  );
}
