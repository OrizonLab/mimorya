'use client';

import { Trash2 } from 'lucide-react';
import { useEffect, useRef } from 'react';
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

/**
 * An account just erased (ADR-190): what went with it, by its counts. The
 * row whose button had the focus is gone with the account: the notice takes
 * the focus, to be read first wherever the row was.
 */
export function ErasedNotice({
  who,
  erased,
  onClose,
}: {
  who: string;
  erased: ErasedAccount;
  onClose: () => void;
}) {
  const heading = useRef<HTMLHeadingElement>(null);
  // Taken again at each erasure, the notice staying in place between them.
  useEffect(() => heading.current?.focus(), [erased]);
  return (
    <Card className="mb-4 flex flex-col gap-2 p-4">
      <h2
        ref={heading}
        tabIndex={-1}
        className="flex items-center gap-2 text-sm font-medium outline-none"
      >
        <Trash2 className="size-4" aria-hidden /> Compte effacé : {who}
      </h2>
      <p className="text-sm">
        {erasedSummary(erased)} Le journal d’audit garde les entrées qui le nommaient.
      </p>
      <Button variant="secondary" className="w-fit" onClick={onClose}>
        Fermer
      </Button>
    </Card>
  );
}
