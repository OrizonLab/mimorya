import type { Microsoft365App } from '@/lib/api';

/**
 * What a declared Microsoft 365 application shows (ADR-116, ADR-118): its
 * IDs, its tools, how many people linked their account, and the address
 * to register in it, where Microsoft sends each person back.
 */
export function Microsoft365Details({ app }: { app: Microsoft365App }) {
  return (
    <div className="flex flex-col gap-1 text-sm text-muted">
      <p>
        Annuaire <code>{app.directory}</code> · application <code>{app.clientId}</code> · outils{' '}
        <code>m365_…</code> · {app.secretStored ? 'secret enregistré' : 'secret absent'} ·{' '}
        {app.links} compte(s) relié(s)
      </p>
      <p>
        Adresse de retour à inscrire dans l’application : <code>{app.redirectUri}</code>
      </p>
    </div>
  );
}
