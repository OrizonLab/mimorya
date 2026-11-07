'use client';

import { KeyRound } from 'lucide-react';
import { useEffect, useId, useRef, useState } from 'react';
import { Button, Card, Field, Input } from '@/components/ui';
import type { AuditExportTokenCreated } from '@/lib/api';

/**
 * An export token just made: shown this once (the API keeps only its
 * digest, ADR-181), with the address the SIEM reads and how it reads it
 * page after page. It takes the focus when shown, to be read first; another
 * token is shown by another notice (its `key`), so that nothing said of the
 * former (« Jeton copié ») stays.
 */
export function ExportTokenNotice({
  made,
  exportUrl,
  onClose,
}: {
  made: AuditExportTokenCreated;
  exportUrl: string;
  onClose: () => void;
}) {
  const heading = useId();
  const headingRef = useRef<HTMLHeadingElement>(null);
  // Whether the token went to the clipboard, once tried.
  const [copied, setCopied] = useState<boolean | null>(null);
  const example = `# La première page, depuis le début du journal :
curl -H "Authorization: Bearer ${made.secret}" \\
  "${exportUrl}?limit=500"
# Les suivantes, après la dernière place lue et son sceau :
curl -H "Authorization: Bearer ${made.secret}" \\
  "${exportUrl}?after=<place>&seal=<sceau>&limit=500"`;

  useEffect(() => headingRef.current?.focus(), []);

  async function copy() {
    try {
      await navigator.clipboard.writeText(made.secret);
      setCopied(true);
    } catch {
      setCopied(false);
    }
  }

  return (
    <section aria-labelledby={heading}>
      <Card className="flex flex-col gap-2 border-warn p-4">
        <h3
          id={heading}
          ref={headingRef}
          tabIndex={-1}
          className="flex items-center gap-2 font-medium outline-none"
        >
          <KeyRound className="size-4" aria-hidden /> Jeton « {made.token.name} »
        </h3>
        <Field label="Jeton, à envoyer dans l’en-tête « Authorization: Bearer »">
          <Input
            readOnly
            value={made.secret}
            className="font-mono text-xs"
            onFocus={(e) => e.target.select()}
          />
        </Field>
        <p className="text-xs text-warn">
          Notez-le maintenant : il ne sera plus affiché. Qui le détient lit tout le journal d’audit
          de l’entreprise ; retirez-le s’il a pu être divulgué.
        </p>
        <Button variant="secondary" className="w-fit" onClick={copy}>
          {copied ? 'Jeton copié' : 'Copier le jeton'}
        </Button>
        {copied === false && (
          <p role="status" className="text-xs text-warn">
            Copie impossible depuis ce navigateur : sélectionnez le jeton et copiez-le vous-même.
          </p>
        )}
        <details className="text-xs text-muted">
          <summary className="cursor-pointer select-none hover:text-fg">Exemple de lecture</summary>
          <pre className="mt-1 overflow-x-auto rounded bg-surface-2 p-2">{example}</pre>
        </details>
        <Button variant="secondary" className="w-fit" onClick={onClose}>
          J’ai noté le jeton
        </Button>
      </Card>
    </section>
  );
}
