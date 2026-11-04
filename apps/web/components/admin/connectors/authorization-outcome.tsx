'use client';

import { useEffect, useState } from 'react';
import { ErrorNote } from '@/components/ui';

/**
 * The parameter the administration is opened with when the browser comes
 * back from an MCP server's authorization server (ADR-171).
 */
export const AUTHORIZATION_RETURN = 'autorisation';

const OUTCOMES = {
  ok: { failed: false, text: 'Serveur MCP autorisé : ses jetons sont gardés chiffrés.' },
  echec: {
    failed: true,
    text: 'L’autorisation du serveur MCP n’a pas abouti. Sa raison est inscrite au journal d’audit.',
  },
} as const;

type Outcome = keyof typeof OUTCOMES;

function isOutcome(value: string | null): value is Outcome {
  return value === 'ok' || value === 'echec';
}

/**
 * How an MCP server's authorization went, said once on the return: a
 * reload of the page does not say it again.
 */
export function AuthorizationOutcome() {
  const [outcome, setOutcome] = useState<Outcome | null>(null);

  useEffect(() => {
    const said = new URLSearchParams(window.location.search).get(AUTHORIZATION_RETURN);
    if (!isOutcome(said)) return;
    setOutcome(said);
    window.history.replaceState(null, '', window.location.pathname);
  }, []);

  if (!outcome) return null;
  const { failed, text } = OUTCOMES[outcome];
  return failed ? (
    <ErrorNote message={text} />
  ) : (
    <p role="status" className="text-sm text-ok">
      {text}
    </p>
  );
}
