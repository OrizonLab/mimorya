'use client';

import { KeyRound } from 'lucide-react';
import { useEffect, useId, useRef, useState, type ReactNode } from 'react';
import { Button } from './button';
import { Card } from './card';
import { cx } from './cx';
import { Field, Input } from './field';

const COPIED = 'Jeton copié dans le presse-papiers.';
const NOT_COPIED =
  'Copie impossible depuis ce navigateur : sélectionnez le jeton et copiez-le vous-même.';

/**
 * A secret shown this once, the API keeping only its digest (a hook's token,
 * ADR-110; an audit export token, ADR-182; a SCIM token, ADR-184): what `children` say of its use
 * first, the secret in a read-only field, `warning`, a copy button whose
 * outcome a status region always there says, alone, afresh at each attempt
 * even when it is the same, an `example`, and `onClose`. It takes the focus
 * when shown, to be read first; another secret is shown by another notice
 * (its `key`), so that nothing said of the former (« Jeton copié ») stays.
 */
export function SecretNotice({
  heading,
  level = 'h2',
  secret,
  label,
  warning,
  example,
  className,
  children,
  onClose,
}: {
  heading: string;
  level?: 'h2' | 'h3';
  secret: string;
  label: string;
  warning: string;
  example: { summary: string; text: string };
  className?: string;
  children?: ReactNode;
  onClose: () => void;
}) {
  const headingId = useId();
  const headingRef = useRef<HTMLHeadingElement>(null);
  // Whether the secret went to the clipboard at the last attempt, and which
  // attempt that was: each one's outcome is a new text in the status region,
  // announced even when the former said the same.
  const [copied, setCopied] = useState<{ ok: boolean; attempt: number } | null>(null);
  const Heading = level;

  useEffect(() => headingRef.current?.focus(), []);

  async function copy() {
    let ok = true;
    try {
      await navigator.clipboard.writeText(secret);
    } catch {
      ok = false;
    }
    setCopied((former) => ({ ok, attempt: (former?.attempt ?? 0) + 1 }));
  }

  return (
    <section aria-labelledby={headingId} className={className}>
      <Card className="flex flex-col gap-2 border-warn p-4">
        <Heading
          id={headingId}
          ref={headingRef}
          tabIndex={-1}
          className="flex items-center gap-2 font-medium outline-none"
        >
          <KeyRound className="size-4" aria-hidden /> {heading}
        </Heading>
        {children}
        <Field label={label}>
          <Input
            readOnly
            value={secret}
            className="font-mono text-xs"
            onFocus={(e) => e.target.select()}
          />
        </Field>
        <p className="text-xs text-warn">{warning}</p>
        <Button variant="secondary" className="w-fit" onClick={copy}>
          Copier le jeton
        </Button>
        {/* There before it speaks, so that what it says is announced; each
            attempt's words a node of their own, added to it. */}
        <p
          role="status"
          className={cx('text-xs', copied?.ok === false ? 'text-warn' : 'text-muted')}
        >
          {copied && <span key={copied.attempt}>{copied.ok ? COPIED : NOT_COPIED}</span>}
        </p>
        <details className="text-xs text-muted">
          <summary className="cursor-pointer select-none hover:text-fg">{example.summary}</summary>
          <pre className="mt-1 overflow-x-auto rounded bg-surface-2 p-2">{example.text}</pre>
        </details>
        <Button variant="secondary" className="w-fit" onClick={onClose}>
          J’ai noté le jeton
        </Button>
      </Card>
    </section>
  );
}
