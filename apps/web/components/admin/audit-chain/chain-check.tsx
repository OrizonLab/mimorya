'use client';

import { useState } from 'react';
import { Button, Card, ErrorNote, Field, Textarea } from '@/components/ui';
import { api, useAction, type AuditChainCheck, type AuditChainVerification } from '@/lib/api';
import { ChainReport } from './chain-report';

/** The anchors written one per line, blank lines left out. */
function anchorsOf(text: string): string[] {
  return text
    .split('\n')
    .map((line) => line.trim())
    .filter((line) => line !== '');
}

/**
 * The verification of the company's audit chain, asked on demand (ADR-179):
 * the whole chain read again, against the anchors kept outside the
 * platform; `onVerified` reloads the log, which traces it.
 */
export function ChainCheck({ onVerified }: { onVerified: () => void }) {
  const [anchors, setAnchors] = useState('');
  const [report, setReport] = useState<AuditChainVerification | null>(null);
  const action = useAction();

  async function verify() {
    const body = { anchors: anchorsOf(anchors) } satisfies AuditChainCheck;
    const verified = await action.run(() =>
      api.post<AuditChainVerification>('/admin/audit/verifications', body),
    );
    if (verified) {
      setReport(verified);
      onVerified();
    }
  }

  return (
    <Card className="flex flex-col gap-3 p-4">
      <p className="font-medium">Intégrité du journal</p>
      <p className="text-sm text-muted">
        Chaque entrée est scellée dans l’ordre, avec la précédente, sous une clé que seul le serveur
        détient : la vérification relit toute la chaîne, qu’une entrée modifiée ou supprimée rompt.
        Conservez l’ancre affichée hors de la plateforme : donnée à une vérification ultérieure,
        elle montre aussi des entrées retirées en fin de chaîne ou une chaîne scellée à nouveau.
      </p>
      <Field label="Ancres à vérifier (facultatif), une par ligne : entreprise:place:sceau">
        <Textarea
          rows={2}
          value={anchors}
          spellCheck={false}
          className="font-mono text-xs"
          onChange={(e) => setAnchors(e.target.value)}
        />
      </Field>
      {action.error && <ErrorNote message={action.error} />}
      <div>
        <Button variant="secondary" loading={action.pending} onClick={verify}>
          Vérifier le journal
        </Button>
      </div>
      {report && <ChainReport report={report} />}
    </Card>
  );
}
