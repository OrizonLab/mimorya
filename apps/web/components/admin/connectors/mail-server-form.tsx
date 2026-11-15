'use client';

import { useState, type FormEvent } from 'react';
import { Button, Checkbox, ErrorNote, Field, Input, Select } from '@/components/ui';
import {
  ApiError,
  api,
  useAction,
  type ConnectorUpdate,
  type ConnectorView,
  type MailImapServer,
  type MailSmtpSecurity,
  type MailSmtpServer,
  type NewMailServerConnector,
} from '@/lib/api';

/** The port each way of securing SMTP usually listens on. */
const SMTP_PORTS: Record<MailSmtpSecurity, string> = { tls: '465', starttls: '587' };

/**
 * How many linked accounts a change would let go of, when the server
 * refused it for that (ADR-205): it counts them under the administration
 * lock, for the question to ask.
 */
function accountsLinked(error: unknown): number | null {
  if (!(error instanceof ApiError) || error.status !== 409) return null;
  const details = error.details as { reason?: unknown; accounts?: unknown } | undefined;
  return details?.reason === 'mail_accounts_linked' && typeof details.accounts === 'number'
    ? details.accounts
    : null;
}

/**
 * Declares the company's standard mail (ADR-205): its IMAP and SMTP
 * servers, its CalDAV agenda if any, and whether its tools may write; or
 * changes them. It holds no secret: each person links their own account
 * in « Mon compte ». A server moved to another host, or an agenda added
 * or removed, lets go of the accounts linked there: the server refuses it
 * until the administrator agrees, the question naming how many it counts,
 * and the form stays busy from the first sending to the last.
 */
export function MailServerForm({
  connector,
  onDone,
}: {
  connector?: ConnectorView;
  onDone: (changed: boolean) => void;
}) {
  const view = connector?.mailServer;
  const [name, setName] = useState(connector?.name ?? 'Messagerie de l’entreprise');
  const [imapHost, setImapHost] = useState(view?.imap.host ?? '');
  const [imapPort, setImapPort] = useState(String(view?.imap.port ?? 993));
  const [smtpHost, setSmtpHost] = useState(view?.smtp.host ?? '');
  const [smtpSecurity, setSmtpSecurity] = useState<MailSmtpSecurity>(view?.smtp.security ?? 'tls');
  const [smtpPort, setSmtpPort] = useState(String(view?.smtp.port ?? 465));
  const [caldav, setCaldav] = useState(view?.caldav ?? '');
  const [writeTools, setWriteTools] = useState(view?.writeTools ?? false);
  const action = useAction();

  function secureWith(security: MailSmtpSecurity) {
    // The usual port follows, unless one was chosen.
    if (smtpPort === SMTP_PORTS[smtpSecurity]) setSmtpPort(SMTP_PORTS[security]);
    setSmtpSecurity(security);
  }

  async function submit(event: FormEvent) {
    event.preventDefault();
    const imap = { host: imapHost.trim(), port: Number(imapPort) } satisfies MailImapServer;
    const smtp = {
      host: smtpHost.trim(),
      port: Number(smtpPort),
      security: smtpSecurity,
    } satisfies MailSmtpServer;
    const agenda = caldav.trim();
    if (connector && view) {
      const imapChanged = imap.host !== view.imap.host || imap.port !== view.imap.port;
      const smtpChanged =
        smtp.host !== view.smtp.host ||
        smtp.port !== view.smtp.port ||
        smtp.security !== view.smtp.security;
      const path = `/admin/connectors/${connector.id}`;
      const save = (letGo: number | undefined) =>
        api.patch(path, {
          name,
          // Each sent when changed only: the audit tells what changed.
          imap: imapChanged ? imap : undefined,
          smtp: smtpChanged ? smtp : undefined,
          // Blank removes the agenda.
          caldav: agenda !== (view.caldav ?? '') ? agenda || null : undefined,
          writeTools: writeTools !== view.writeTools ? writeTools : undefined,
          mailAccountsLetGo: letGo,
        } satisfies ConnectorUpdate);
      const saved = await action.run(async () => {
        try {
          await save(undefined);
          return true;
        } catch (error) {
          const linked = accountsLinked(error);
          if (linked === null) throw error;
          if (
            !window.confirm(
              `Les serveurs où partent les mots de passe changent : les ${linked} compte(s) relié(s) sont retirés, et chacun relie le sien de nouveau dans « Mon compte ». Continuer ?`,
            )
          ) {
            return false;
          }
          // Refused again, saying how many, if more were linked since.
          await save(linked);
          return true;
        }
      });
      if (saved) onDone(true);
      return;
    }
    const saved = await action.run(() =>
      api.post('/admin/connectors/mail-server', {
        name,
        imap,
        smtp,
        caldav: agenda || undefined,
        writeTools,
      } satisfies NewMailServerConnector),
    );
    if (saved) onDone(true);
  }

  return (
    <form aria-label="Messagerie standard" onSubmit={submit} className="flex flex-col gap-3">
      <Field label="Nom">
        <Input
          required
          minLength={2}
          maxLength={80}
          value={name}
          onChange={(e) => setName(e.target.value)}
        />
      </Field>
      <div className="grid gap-3 sm:grid-cols-[1fr_8rem]">
        <Field label="Serveur IMAP (TLS dès la connexion)">
          <Input
            required
            maxLength={253}
            placeholder="imap.entreprise.fr"
            value={imapHost}
            onChange={(e) => setImapHost(e.target.value)}
          />
        </Field>
        <Field label="Port IMAP">
          <Input
            required
            type="number"
            min={1}
            max={65535}
            value={imapPort}
            onChange={(e) => setImapPort(e.target.value)}
          />
        </Field>
      </div>
      <div className="grid gap-3 sm:grid-cols-[1fr_12rem_8rem]">
        <Field label="Serveur SMTP">
          <Input
            required
            maxLength={253}
            placeholder="smtp.entreprise.fr"
            value={smtpHost}
            onChange={(e) => setSmtpHost(e.target.value)}
          />
        </Field>
        <Field label="Sécurité SMTP">
          <Select
            value={smtpSecurity}
            onChange={(e) => secureWith(e.target.value as MailSmtpSecurity)}
          >
            <option value="tls">TLS dès la connexion</option>
            <option value="starttls">STARTTLS exigé</option>
          </Select>
        </Field>
        <Field label="Port SMTP">
          <Input
            required
            type="number"
            min={1}
            max={65535}
            value={smtpPort}
            onChange={(e) => setSmtpPort(e.target.value)}
          />
        </Field>
      </div>
      <Field label="Agenda CalDAV (facultatif, adresse en https://)">
        <Input
          type="url"
          maxLength={2048}
          placeholder="https://agenda.entreprise.fr/dav/"
          value={caldav}
          onChange={(e) => setCaldav(e.target.value)}
        />
      </Field>
      <label className="flex items-center gap-2 text-sm">
        <Checkbox checked={writeTools} onChange={(e) => setWriteTools(e.target.checked)} />
        Permettre l’envoi d’e-mails et la création de rendez-vous, chacun validé
      </label>
      <p className="text-xs text-muted">
        La messagerie n’a aucun secret : chaque collaborateur relie son propre compte dans « Mon
        compte », avec son identifiant et son mot de passe, vérifiés auprès de ces serveurs et
        chiffrés pour lui seul. Les connexions sont toujours chiffrées (TLS) ; un serveur SMTP en
        clair qui n’offre pas STARTTLS est refusé, et l’agenda n’est joint qu’en HTTPS. Un e-mail ou
        un rendez-vous n’est jamais envoyé sans validation, quel que soit le niveau d’autonomie ;
        les invitations sont envoyées par le serveur d’agenda lui-même. Changer l’hôte d’un serveur,
        ou ajouter ou retirer l’agenda, retire les comptes reliés.
      </p>
      {action.error && <ErrorNote message={action.error} />}
      <div className="flex gap-2">
        <Button type="submit" loading={action.pending}>
          {connector ? 'Enregistrer' : 'Déclarer la messagerie'}
        </Button>
        <Button
          type="button"
          variant="ghost"
          // Closed before its change is answered, it could be reopened from the
          // former resource, and that change overwritten.
          disabled={action.pending}
          onClick={() => onDone(false)}
        >
          Annuler
        </Button>
      </div>
    </form>
  );
}
