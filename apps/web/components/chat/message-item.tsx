'use client';

import { AlertTriangle, MessagesSquare, ShieldCheck, Target } from 'lucide-react';
import { documentFileUrl } from '@/components/documents';
import { Badge, cx, Markdown } from '@/components/ui';
import type { Citation, Message } from '@/lib/api';
import { gatewayLabel } from '@/lib/labels';
import { HookTurn } from './hook-turn';

const SOURCE_LABELS: Record<string, string> = {
  intervention: 'Intervention',
  memory: 'Mémoire',
  skill: 'Compétence',
  deliverable: 'Livrable',
  document: 'Document',
  email: 'E-mail',
  event: 'Rendez-vous',
};

export function MessageItem({ message }: { message: Message }) {
  const { verification, evidence, status, mission, inboundHookId, gateway } = message.metadata;
  // The word that sets a mission's next stage going is the platform's.
  if (mission === 'resumption') {
    return (
      <p
        role="note"
        className="flex items-center justify-center gap-2 text-center text-xs text-muted"
      >
        <Target className="size-3" aria-hidden />
        {message.content}
      </p>
    );
  }
  // Sent by an inbound hook: its request, then the data it received (ADR-109).
  if (message.role === 'user' && inboundHookId) {
    return <HookTurn content={message.content} />;
  }
  if (message.role === 'user') {
    return (
      <div className="flex flex-col items-end gap-1">
        {mission === 'brief' && <Badge tone="accent">Mission confiée</Badge>}
        {/* Written to the company's Slack application or Telegram bot (ADR-128, ADR-135). */}
        {gateway && (
          <Badge>
            <MessagesSquare className="size-3" aria-hidden />
            Écrit dans {gatewayLabel(gateway)}
          </Badge>
        )}
        <div className="max-w-[85%] rounded-2xl rounded-br-sm bg-accent px-4 py-2.5 text-[0.95rem] whitespace-pre-wrap text-accent-fg">
          {message.content}
        </div>
      </div>
    );
  }

  const sources = dedupe(evidence ?? []).filter((c) => c.type !== 'skill');
  return (
    <div className="flex flex-col gap-2">
      {mission === 'progress' && (
        <div className="px-1">
          <Badge tone="accent">Point d’étape</Badge>
        </div>
      )}
      <div
        className={cx(
          'rounded-2xl rounded-bl-sm border bg-surface px-4 py-3',
          status === 'failed' ? 'border-danger/40' : 'border-line',
        )}
      >
        <Markdown>{message.content}</Markdown>
      </div>
      <div className="flex flex-wrap items-center gap-2 px-1">
        {verification &&
          (verification.passed ? (
            <Badge tone="ok">
              <ShieldCheck className="size-3" aria-hidden />
              Vérifié {verification.judged ? '(contrôle qualité)' : '(sources)'}
            </Badge>
          ) : (
            <Badge tone="warn">
              <AlertTriangle className="size-3" aria-hidden />
              Points non vérifiés
            </Badge>
          ))}
        {sources.length > 0 && (
          <details className="text-xs text-muted">
            <summary className="cursor-pointer select-none hover:text-fg">
              {sources.length} source(s) consultée(s)
            </summary>
            <ul className="mt-1 flex flex-wrap gap-1">
              {sources.map((c) => (
                <li key={`${c.type}:${c.id}`}>
                  <Source citation={c} />
                </li>
              ))}
            </ul>
          </details>
        )}
      </div>
    </div>
  );
}

/** A source of the answer; a document's downloads its file (ADR-096). */
function Source({ citation }: { citation: Citation }) {
  const name = citation.id.length > 12 ? (citation.label ?? citation.id.slice(0, 8)) : citation.id;
  const badge = (
    <Badge>
      {SOURCE_LABELS[citation.type] ?? citation.type} {name}
    </Badge>
  );
  if (citation.type !== 'document') return badge;
  return (
    <a
      href={documentFileUrl(citation.id)}
      download
      aria-label={`Télécharger le document « ${name} »`}
      className="hover:underline"
    >
      {badge}
    </a>
  );
}

function dedupe<T extends { type: string; id: string }>(items: T[]): T[] {
  const seen = new Set<string>();
  return items.filter((i) => {
    const key = `${i.type}:${i.id}`;
    if (seen.has(key)) return false;
    seen.add(key);
    return true;
  });
}
