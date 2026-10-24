'use client';

import { AlertTriangle, ShieldCheck, Target } from 'lucide-react';
import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import { Badge, cx } from '@/components/ui';
import type { Message } from '@/lib/api';

const SOURCE_LABELS: Record<string, string> = {
  intervention: 'Intervention',
  memory: 'Mémoire',
  skill: 'Compétence',
  deliverable: 'Livrable',
};

export function Markdown({ children }: { children: string }) {
  return (
    <div className="prose-answer text-[0.95rem]">
      <ReactMarkdown remarkPlugins={[remarkGfm]} skipHtml>
        {children}
      </ReactMarkdown>
    </div>
  );
}

export function MessageItem({ message }: { message: Message }) {
  const { verification, evidence, status, mission } = message.metadata;
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
  if (message.role === 'user') {
    return (
      <div className="flex flex-col items-end gap-1">
        {mission === 'brief' && <Badge tone="accent">Mission confiée</Badge>}
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
                  <Badge>
                    {SOURCE_LABELS[c.type] ?? c.type}{' '}
                    {c.id.length > 12 ? (c.label ?? c.id.slice(0, 8)) : c.id}
                  </Badge>
                </li>
              ))}
            </ul>
          </details>
        )}
      </div>
    </div>
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
