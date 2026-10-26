'use client';

import { KeyRound } from 'lucide-react';
import { useEffect, useId, useRef, useState } from 'react';
import { Button, Card, Field, Input } from '@/components/ui';
import type { InboundHookToken } from '@/lib/api';

/**
 * A hook's token, just made or renewed: shown this once (the API keeps only
 * its digest, ADR-109), with the address another system posts to and a
 * command that does it. It takes the focus when shown, to be read first;
 * another token is shown by another notice (its `key`), so that nothing
 * said of the former (« Jeton copié ») stays.
 */
export function TokenNotice({
  title,
  issued,
  onClose,
}: {
  title: string;
  issued: InboundHookToken;
  onClose: () => void;
}) {
  const heading = useId();
  const headingRef = useRef<HTMLHeadingElement>(null);
  // Whether the token went to the clipboard, once tried.
  const [copied, setCopied] = useState<boolean | null>(null);
  const example = `curl -X POST ${issued.url} \\
  -H "Authorization: Bearer ${issued.token}" \\
  -H "Content-Type: application/json" \\
  --data '{"alerte": "Pompe P2 à l’arrêt", "site": "Lyon Nord"}'`;

  useEffect(() => headingRef.current?.focus(), []);

  async function copy() {
    try {
      await navigator.clipboard.writeText(issued.token);
      setCopied(true);
    } catch {
      setCopied(false);
    }
  }

  return (
    <section aria-labelledby={heading} className="mb-6">
      <Card className="flex flex-col gap-2 border-warn p-4">
        <h2
          id={heading}
          ref={headingRef}
          tabIndex={-1}
          className="flex items-center gap-2 font-medium outline-none"
        >
          <KeyRound className="size-4" aria-hidden /> Jeton de « {title} »
        </h2>
        <p className="text-sm">Adresse à appeler (méthode POST) :</p>
        <code className="w-fit max-w-full rounded bg-surface-2 px-2 py-1 text-xs break-all select-all">
          {issued.url}
        </code>
        <Field label="Jeton, à envoyer dans l’en-tête « Authorization: Bearer »">
          <Input
            readOnly
            value={issued.token}
            className="font-mono text-xs"
            onFocus={(e) => e.target.select()}
          />
        </Field>
        <p className="text-xs text-warn">
          Notez-le maintenant : il ne sera plus affiché. Qui le détient peut confier des demandes à
          l’agent en votre nom ; renouvelez-le s’il a pu être divulgué.
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
          <summary className="cursor-pointer select-none hover:text-fg">Exemple d’envoi</summary>
          <pre className="mt-1 overflow-x-auto rounded bg-surface-2 p-2">{example}</pre>
        </details>
        <Button variant="secondary" className="w-fit" onClick={onClose}>
          J’ai noté le jeton
        </Button>
      </Card>
    </section>
  );
}
