import type { Microsoft365App } from '@/lib/api';

/**
 * What a declared Microsoft 365 application shows (ADR-116, ADR-118): its
 * IDs, its tools and whether they may write (ADR-120), how many people
 * linked their account, and the address to register in it, where
 * Microsoft sends each person back.
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
        {app.writeTools
          ? 'Envoi d’e-mails et création de rendez-vous permis, chacun validé'
          : 'Lecture seule : ni envoi d’e-mails ni création de rendez-vous'}
      </p>
      <p>
        Adresse de retour à inscrire dans l’application : <code>{app.redirectUri}</code>
      </p>
    </div>
  );
}
