'use client';

import { useEffect, useRef } from 'react';
import { Button } from '@/components/ui';

/**
 * The recovery codes, shown this once (ADR-200): each signs in once
 * without the app. The notice takes the focus when shown, to be read first;
 * the person says when they are noted.
 */
export function RecoveryCodesNotice({
  codes,
  doneLabel,
  onDone,
}: {
  codes: string[];
  doneLabel: string;
  onDone: () => void;
}) {
  const heading = useRef<HTMLHeadingElement>(null);
  useEffect(() => heading.current?.focus(), []);

  return (
    <section aria-labelledby="recovery-codes-heading" className="flex flex-col gap-3">
      <h3
        id="recovery-codes-heading"
        ref={heading}
        tabIndex={-1}
        className="text-sm font-medium outline-none"
      >
        Vos codes de secours
      </h3>
      <p className="text-sm text-muted">
        Chacun permet une connexion si vous n’avez plus votre application. Notez-les ou imprimez-les
        et gardez-les en lieu sûr : ils ne seront plus affichés.
      </p>
      <ol aria-label="Codes de secours" className="grid w-fit grid-cols-2 gap-x-6 gap-y-1">
        {codes.map((code) => (
          <li key={code} className="font-mono text-sm">
            {code}
          </li>
        ))}
      </ol>
      <Button type="button" className="w-fit" onClick={onDone}>
        {doneLabel}
      </Button>
    </section>
  );
}
