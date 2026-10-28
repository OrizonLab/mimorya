import type { GoogleWorkspaceApp } from '@/lib/api';

/**
 * What a declared Google Workspace client shows (ADR-121, ADR-124): its
 * ID, its tools and whether they may write (ADR-123), how many people
 * linked their account, and the address to register in it, where Google
 * sends each person back.
 */
export function GoogleWorkspaceDetails({ app }: { app: GoogleWorkspaceApp }) {
  return (
    <div className="flex flex-col gap-1 text-sm text-muted">
      <p>
        Client <code>{app.clientId}</code> · outils <code>google_…</code> ·{' '}
        {app.secretStored ? 'secret enregistré' : 'secret absent'} · {app.links} compte(s) relié(s)
      </p>
      <p>
        {app.writeTools
          ? 'Envoi d’e-mails et création de rendez-vous permis, chacun validé'
          : 'Lecture seule : ni envoi d’e-mails ni création de rendez-vous'}
      </p>
      <p>
        URI de redirection autorisé à inscrire dans le client : <code>{app.redirectUri}</code>
      </p>
    </div>
  );
}
