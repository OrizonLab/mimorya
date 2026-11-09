'use client';

import { Trash2 } from 'lucide-react';
import { Button, Card } from '@/components/ui';
import type { ErasedAccount } from '@/lib/api';
import { formatNumber } from '@/lib/format';

/** `count` of a thing, its words agreeing with it. */
function counted(count: number, one: string, many: string): string {
  return `${formatNumber(count)} ${count > 1 ? many : one}`;
}

/** What went with an erased account, by its counts. */
export function erasedSummary(erased: ErasedAccount): string {
  const conversations = counted(erased.conversations, 'conversation', 'conversations');
  const messages = counted(erased.messages, 'message', 'messages');
  const memories = counted(erased.memories, 'mémoire personnelle', 'mémoires personnelles');
  const runs = counted(erased.runs, 'demande faite pour lui', 'demandes faites pour lui');
  return `Sont partis avec lui ${conversations} (${messages}), ${memories} et ${runs}.`;
}

/** An account just erased (ADR-189): what went with it, by its counts. */
export function ErasedNotice({
  who,
  erased,
  onClose,
}: {
  who: string;
  erased: ErasedAccount;
  onClose: () => void;
}) {
  return (
    <Card className="mb-4 flex flex-col gap-2 p-4">
      <p className="flex items-center gap-2 text-sm font-medium">
        <Trash2 className="size-4" aria-hidden /> Compte de {who} effacé
      </p>
      <p className="text-sm">
        {erasedSummary(erased)} Le journal d’audit garde les entrées qui le nommaient.
      </p>
      <Button variant="secondary" className="w-fit" onClick={onClose}>
        Fermer
      </Button>
    </Card>
  );
}
