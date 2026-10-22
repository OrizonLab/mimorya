'use client';

import { Badge, Button, ErrorNote } from '@/components/ui';
import { api, useAction, type DomainProofRequest, type DomainProofView } from '@/lib/api';
import { formatDate } from '@/lib/format';

/** One domain: its record to publish, whether it is proved, and its reading. */
export function DomainProofRow({
  proof,
  onChanged,
}: {
  proof: DomainProofView;
  onChanged: () => void;
}) {
  const action = useAction();

  async function verify() {
    const body = { domain: proof.domain } satisfies DomainProofRequest;
    if (await action.run(() => api.post('/admin/identity-providers/domain-proofs', body))) {
      onChanged();
    }
  }

  return (
    <div className="flex flex-col gap-2 border-t border-line pt-3">
      <div className="flex flex-wrap items-center gap-2">
        <p className="flex-1 font-medium">{proof.domain}</p>
        <Badge tone={proof.verifiedAt ? 'ok' : 'warn'}>
          {proof.verifiedAt ? `Prouvé le ${formatDate(proof.verifiedAt)}` : 'Non prouvé'}
        </Badge>
      </div>
      <p className="text-sm">
        Enregistrement TXT : <code className="break-all select-all">{proof.recordName}</code>
      </p>
      <p className="text-sm">
        Valeur : <code className="break-all select-all">{proof.recordValue}</code>
      </p>
      {action.error && <ErrorNote message={action.error} />}
      <Button variant="secondary" className="w-fit" loading={action.pending} onClick={verify}>
        {proof.verifiedAt ? 'Vérifier de nouveau' : 'Vérifier'}
      </Button>
    </div>
  );
}
