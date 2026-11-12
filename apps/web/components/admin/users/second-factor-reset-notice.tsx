'use client';

import { ShieldOff } from 'lucide-react';
import { Button, Card } from '@/components/ui';

/** An account's second factor just reset (ADR-202): what follows for it. */
export function SecondFactorResetNotice({ who, onClose }: { who: string; onClose: () => void }) {
  return (
    <Card className="mb-4 flex flex-col gap-2 p-4">
      <p role="status" className="flex items-center gap-2 text-sm font-medium">
        <ShieldOff className="size-4" aria-hidden /> Second facteur de {who} réinitialisé
      </p>
      <p className="text-sm">
        Ses sessions sont fermées. À sa prochaine connexion, son mot de passe suffit, ou il lui est
        demandé d’enrôler une nouvelle application si l’entreprise l’exige de lui.
      </p>
      <Button variant="secondary" className="w-fit" onClick={onClose}>
        Fermer
      </Button>
    </Card>
  );
}
