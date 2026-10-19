'use client';

import { KeyRound } from 'lucide-react';
import { Button, Card } from '@/components/ui';

/** A temporary password, shown once: the server never displays it again. */
export function TemporaryPassword({
  who,
  password,
  onClose,
}: {
  who: string;
  password: string;
  onClose: () => void;
}) {
  return (
    <Card className="mb-4 flex flex-col gap-2 border-warn p-4">
      <p className="flex items-center gap-2 text-sm font-medium">
        <KeyRound className="size-4" aria-hidden /> Mot de passe provisoire de {who}
      </p>
      <code className="w-fit rounded bg-surface-2 px-2 py-1 text-sm select-all">{password}</code>
      <p className="text-xs text-muted">
        Transmettez-le par un canal sûr : il ne sera plus affiché. À sa prochaine connexion, la
        personne devra le remplacer par un mot de passe de son choix.
      </p>
      <Button variant="secondary" className="w-fit" onClick={onClose}>
        J’ai transmis le mot de passe
      </Button>
    </Card>
  );
}
