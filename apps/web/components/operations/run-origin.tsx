import Link from 'next/link';
import type { RunView } from '@/lib/api';

/** Where a run comes from: its conversation, or the run a sub-agent worked for (ADR-099). */
export function RunOrigin({ run }: { run: RunView }) {
  const link = 'text-sm text-accent underline';
  if (run.parentRunId) {
    return (
      <Link href={`/operations/${run.parentRunId}`} className={link}>
        Demande d’origine
      </Link>
    );
  }
  if (run.conversationId) {
    return (
      <Link href={`/chat/${run.conversationId}`} className={link}>
        Ouvrir la conversation
      </Link>
    );
  }
  return null;
}
