'use client';

import { Card } from '@/components/ui';
import type { DomainProofView } from '@/lib/api';
import { DomainProofRow } from './domain-proof-row';

/**
 * The company's e-mail domains and their proof (ADR-069): the DNS record to
 * publish for each, read on request. A proved domain keeps its new addresses
 * for the company, and a generic OpenID Connect or SAML provider creates
 * accounts only there.
 */
export function DomainProofs({
  domains,
  onChanged,
}: {
  domains: DomainProofView[];
  onChanged: () => void;
}) {
  if (domains.length === 0) return null;
  return (
    <Card className="flex flex-col gap-3 p-4">
      <p className="font-medium">Domaines de l’entreprise</p>
      <p className="text-sm text-muted">
        Prouvez que l’entreprise possède chacun de ses domaines : publiez dans son DNS
        l’enregistrement TXT indiqué, puis vérifiez. Un domaine prouvé réserve ses nouvelles
        adresses à l’entreprise, et un fournisseur OpenID Connect générique ou SAML n’y crée les
        comptes qu’une fois prouvé.
      </p>
      {domains.map((proof) => (
        <DomainProofRow key={proof.domain} proof={proof} onChanged={onChanged} />
      ))}
    </Card>
  );
}
