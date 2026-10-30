'use client';

import Link from 'next/link';
import { useState } from 'react';
import { Button, Card, ErrorNote } from '@/components/ui';
import { api, useAction, type GatewayLinkCode, type GatewayLinkPreview } from '@/lib/api';
import { formatDate } from '@/lib/format';

/**
 * What the link the company's Slack application, or its Teams bot, sent a
 * person would link (ADR-125, ADR-126, ADR-138), shown in their own
 * session, then confirmed: the server asks the service again whether this
 * identity has their account's address.
 */
export function GatewayLinkConfirmation({
  code,
  preview,
}: {
  code: string;
  preview: GatewayLinkPreview;
}) {
  const [linked, setLinked] = useState(false);
  const action = useAction();
  const teams = preview.type === 'teams';
  const service = teams ? 'Teams' : 'Slack';

  async function confirm() {
    const done = await action.run(() =>
      api.post('/account/gateways/link', { code } satisfies GatewayLinkCode),
    );
    if (done) setLinked(true);
  }

  return (
    <Card className="flex flex-col gap-3 p-6">
      <h2 className="font-medium">Relier votre compte {service}</h2>
      {linked ? (
        <>
          <p role="status" className="text-sm text-ok">
            Votre compte {service} est relié à votre compte.
          </p>
          <Link href="/account" className="text-sm font-medium text-accent underline">
            Mon compte
          </Link>
        </>
      ) : (
        <>
          <p className="text-sm">
            Relier le compte {service} <code>{preview.externalId}</code>{' '}
            {teams ? 'de l’annuaire' : 'de l’espace'} {`« ${preview.workspaceName} »`} (
            {preview.name}) à votre compte ? {service} doit donner pour lui l’adresse de votre
            compte. Lien valable jusqu’au {formatDate(preview.expiresAt)}.
          </p>
          {action.error && <ErrorNote message={action.error} />}
          <Button loading={action.pending} onClick={() => void confirm()}>
            Relier mon compte {service}
          </Button>
        </>
      )}
    </Card>
  );
}
