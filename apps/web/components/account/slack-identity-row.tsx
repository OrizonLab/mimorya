'use client';

import type { AccountGatewayView } from '@/lib/api';
import { formatDate } from '@/lib/format';
import { LinkedByMessageRow } from './linked-by-message-row';

/**
 * The signed-in person's Slack user on the company's Slack application
 * (ADR-125, ADR-126): linked from the link the application sent them in
 * Slack, and removed here.
 */
export function SlackIdentityRow({
  gateway,
  onChanged,
}: {
  gateway: AccountGatewayView;
  onChanged: () => void;
}) {
  const space = `« ${gateway.workspaceName} »`;
  return (
    <LinkedByMessageRow
      gateway={gateway}
      linked={(identity) => (
        <>
          Votre compte Slack <code>{identity.externalId}</code> ({identity.address}) de l’espace{' '}
          {space} est relié depuis le {formatDate(identity.linkedAt)}. Vous y parlez à l’agent, qui
          vous y annonce les demandes attendant votre validation.
        </>
      )}
      howTo={
        <>
          Pour relier votre compte Slack, écrivez à l’application de l’entreprise dans l’espace{' '}
          {space} : elle vous répond par un lien à ouvrir ici, connecté. Slack doit donner pour
          votre compte l’adresse de votre compte.
        </>
      }
      unlinkQuestion={`Retirer la liaison de votre compte Slack de l’espace ${space} ? Vous pourrez le relier de nouveau depuis le lien que l’application vous enverra.`}
      onChanged={onChanged}
    />
  );
}
