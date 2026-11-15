'use client';

import { useState, type FormEvent } from 'react';
import { Button, Checkbox, ErrorNote, Field, Input, Select } from '@/components/ui';
import {
  api,
  useAction,
  type ConnectorUpdate,
  type ConnectorView,
  type Connectors,
  type MailImapServer,
  type MailServerView,
  type MailSmtpSecurity,
  type MailSmtpServer,
  type NewMailServerConnector,
} from '@/lib/api';

/** The port each way of securing SMTP usually listens on. */
const SMTP_PORTS: Record<MailSmtpSecurity, string> = { tls: '465', starttls: '587' };

/**
 * The hosts a person's password would go to, as the server compares them:
 * the IMAP and SMTP servers' and the agenda's (ADR-205).
 */
function hosts(imap: string, smtp: string, caldav: string): string[] {
  let agenda = caldav.trim();
  try {
    if (agenda) agenda = new URL(agenda).hostname;
  } catch {
    // An address the server refuses: it says why.
  }
  return [imap.trim(), smtp.trim(), agenda].filter(Boolean).map((h) => h.toLowerCase());
}

/** The standard mail `id` as the server holds it now, if it reads. */
async function storedMail(id: string): Promise<MailServerView | null> {
  try {
    const { instances } = await api.get<Connectors>('/admin/connectors');
    return instances.find((c) => c.id === id)?.mailServer ?? null;
  } catch {
    return null;
  }
}

/**
 * Declares the company's standard mail (ADR-205): its IMAP and SMTP
 * servers, its CalDAV agenda if any, and whether its tools may write; or
 * changes them. It holds no secret: each person links their own account
 * in « Mon compte ». A server moved to another host, or an agenda added
 * or removed, lets go of the accounts linked there, which the
 * administrator confirms first, as the server reads them then.
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
      // Asked of the mail as stored now, not as the tab loaded it: people
      // may have linked their accounts since.
      const stored = (await storedMail(connector.id)) ?? view;
      const moved =
        hosts(imap.host, smtp.host, agenda).join(' ') !==
        hosts(stored.imap.host, stored.smtp.host, stored.caldav ?? '').join(' ');
      if (
        moved &&
        stored.accounts > 0 &&
        !window.confirm(
          `Les serveurs où partent les mots de passe changent : les ${stored.accounts} compte(s) relié(s) sont retirés, et chacun relie le sien de nouveau dans « Mon compte ». Continuer ?`,
        )
      ) {
        return;
      }
      const imapChanged = imap.host !== view.imap.host || imap.port !== view.imap.port;
      const smtpChanged =
        smtp.host !== view.smtp.host ||
        smtp.port !== view.smtp.port ||
        smtp.security !== view.smtp.security;
      const saved = await action.run(() =>
        api.patch(`/admin/connectors/${connector.id}`, {
          name,
          // Each sent when changed only: the audit tells what changed.
          imap: imapChanged ? imap : undefined,
          smtp: smtpChanged ? smtp : undefined,
          // Blank removes the agenda.
          caldav: agenda !== (view.caldav ?? '') ? agenda || null : undefined,
          writeTools: writeTools !== view.writeTools ? writeTools : undefined,
        } satisfies ConnectorUpdate),
      );
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
