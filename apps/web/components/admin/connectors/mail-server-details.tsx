import type { MailServerView } from '@/lib/api';

/**
 * What the declared standard mail shows (ADR-205): its servers and how
 * each is secured, its agenda if any, its tools and whether they may
 * write, and how many people linked their account.
 */
export function MailServerDetails({ mail }: { mail: MailServerView }) {
  const smtp = mail.smtp.security === 'tls' ? 'TLS dès la connexion' : 'STARTTLS exigé';
  return (
    <div className="flex flex-col gap-1 text-sm text-muted">
      <p>
        IMAP{' '}
        <code>
          {mail.imap.host}:{mail.imap.port}
        </code>{' '}
        · SMTP{' '}
        <code>
          {mail.smtp.host}:{mail.smtp.port}
        </code>{' '}
        ({smtp}) · outils <code>mail_…</code> · {mail.accounts} compte(s) relié(s)
      </p>
      <p>
        {mail.caldav ? (
          <>
            Agenda CalDAV <code>{mail.caldav}</code>
          </>
        ) : (
          'Aucun agenda déclaré'
        )}
      </p>
      <p>
        {mail.writeTools
          ? 'Envoi d’e-mails et création de rendez-vous permis, chacun validé'
          : 'Lecture seule : ni envoi d’e-mails ni création de rendez-vous'}
      </p>
    </div>
  );
}
