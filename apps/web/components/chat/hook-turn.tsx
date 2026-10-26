import { PlugZap } from 'lucide-react';
import { Badge } from '@/components/ui';

/** How the platform encloses what a hook received after its instruction. */
const OPEN = '\n\n<donnees_outil source="declencheur" fiabilite="non_fiable">\n';
const CLOSE = '\n</donnees_outil>';

/**
 * A turn an inbound hook sent as its author's (ADR-109), as the model read
 * it: the instruction, then what was posted, enclosed. What was posted can
 * open no envelope (the platform neutralised it), so the last opening is
 * the platform's.
 */
export function splitHookTurn(content: string): { instruction: string; received: string | null } {
  const at = content.lastIndexOf(OPEN);
  if (at < 0) return { instruction: content, received: null };
  const enclosed = content.slice(at + OPEN.length);
  const end = enclosed.lastIndexOf(CLOSE);
  return {
    instruction: content.slice(0, at),
    received: end < 0 ? enclosed : enclosed.slice(0, end),
  };
}

/** The turn as its author reads it: the request, and the data apart. */
export function HookTurn({ content }: { content: string }) {
  const { instruction, received } = splitHookTurn(content);
  return (
    <div className="flex flex-col items-end gap-1">
      <Badge tone="accent">
        <PlugZap className="size-3" aria-hidden />
        Envoyé par un déclencheur
      </Badge>
      <div className="max-w-[85%] rounded-2xl rounded-br-sm bg-accent px-4 py-2.5 text-[0.95rem] whitespace-pre-wrap text-accent-fg">
        {instruction}
      </div>
      {received !== null && (
        <details className="max-w-[85%] text-xs text-muted">
          <summary className="cursor-pointer text-right select-none hover:text-fg">
            Données reçues
          </summary>
          <pre className="mt-1 rounded bg-surface-2 p-2 whitespace-pre-wrap break-all">
            {received}
          </pre>
        </details>
      )}
    </div>
  );
}
