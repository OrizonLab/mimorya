'use client';

import { Mail } from 'lucide-react';
import { useState } from 'react';
import { Button, Card } from '@/components/ui';
import type { IssuedInvitation } from '@/lib/api';
import { formatDate } from '@/lib/format';

/** An invitation just issued: sent by e-mail, or a link shown once to transmit. */
export function InvitationNotice({
  who,
  email,
  invitation,
  onClose,
}: {
  who: string;
  email: string;
  invitation: IssuedInvitation;
  onClose: () => void;
}) {
  const [copied, setCopied] = useState(false);
  const until = formatDate(invitation.expiresAt);

  async function copy(link: string) {
    try {
      await navigator.clipboard.writeText(link);
      setCopied(true);
    } catch {
      setCopied(false);
    }
  }

  return (
    <Card className={`mb-4 flex flex-col gap-2 p-4 ${invitation.link ? 'border-warn' : ''}`}>
      <p className="flex items-center gap-2 text-sm font-medium">
        <Mail className="size-4" aria-hidden /> Invitation de {who}
      </p>
      {invitation.emailed ? (
        <p className="text-sm">
          Invitation envoyée à {email}. Le lien est valable jusqu’au {until}.
        </p>
      ) : (
        <>
          {invitation.problem && <p className="text-sm text-warn">{invitation.problem}</p>}
          {invitation.link && (
            <>
              <code className="w-fit max-w-full rounded bg-surface-2 px-2 py-1 text-xs break-all select-all">
                {invitation.link}
              </code>
              <p className="text-xs text-muted">
                Transmettez ce lien à {who} par un canal sûr : il ne sera plus affiché. Il sert une
                seule fois, jusqu’au {until}, pour choisir son mot de passe.
              </p>
              <Button
                variant="secondary"
                className="w-fit"
                onClick={() => copy(invitation.link ?? '')}
              >
                {copied ? 'Lien copié' : 'Copier le lien'}
              </Button>
            </>
          )}
        </>
      )}
      <Button variant="secondary" className="w-fit" onClick={onClose}>
        {invitation.emailed ? 'Fermer' : 'J’ai transmis le lien'}
      </Button>
    </Card>
  );
}
