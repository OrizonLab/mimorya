'use client';

import { useState, type FormEvent } from 'react';
import { Button, Checkbox, ErrorNote, Field, Input } from '@/components/ui';
import {
  api,
  useAction,
  type ConnectorUpdate,
  type ConnectorView,
  type NewGoogleWorkspaceConnector,
} from '@/lib/api';

/** The OAuth client ID Google Cloud shows, as the server checks it. */
const CLIENT_ID = '[A-Za-z0-9.\\-]+\\.apps\\.googleusercontent\\.com';

/**
 * Declares the company's Google Workspace client (ADR-121, ADR-124), or
 * changes its name, its secret and whether its tools may write (ADR-123):
 * its client ID stays, as the links its people made there do.
 */
export function GoogleWorkspaceForm({
  connector,
  onDone,
}: {
  connector?: ConnectorView;
  onDone: (changed: boolean) => void;
}) {
  const [name, setName] = useState(connector?.name ?? 'Google Workspace');
  const [clientId, setClientId] = useState('');
  const [secret, setSecret] = useState('');
  const [writeTools, setWriteTools] = useState(connector?.googleWorkspace?.writeTools ?? false);
  const action = useAction();

  async function submit(event: FormEvent) {
    event.preventDefault();
    const saved = await action.run(() =>
      connector
        ? api.patch(`/admin/connectors/${connector.id}`, {
            name,
            // Left out when blank: the stored secret is kept.
            token: secret.trim() || undefined,
            // Sent when changed only: the audit tells what changed.
            writeTools:
              writeTools !== connector.googleWorkspace?.writeTools ? writeTools : undefined,
          } satisfies ConnectorUpdate)
        : api.post('/admin/connectors/google-workspace', {
            name,
            clientId: clientId.trim(),
            clientSecret: secret.trim(),
            writeTools,
          } satisfies NewGoogleWorkspaceConnector),
    );
    if (saved) onDone(true);
  }

  return (
    <form aria-label="Client Google Workspace" onSubmit={submit} className="flex flex-col gap-3">
      <div className="grid gap-3 sm:grid-cols-2">
        <Field label="Nom">
          <Input
            required
            minLength={2}
            maxLength={80}
            value={name}
            onChange={(e) => setName(e.target.value)}
          />
        </Field>
        {!connector && (
          <Field label="ID client OAuth">
            <Input
              required
              pattern={CLIENT_ID}
              maxLength={255}
              placeholder="123456789012-abc….apps.googleusercontent.com"
              value={clientId}
              onChange={(e) => setClientId(e.target.value)}
            />
          </Field>
        )}
        <Field
          label={
            connector
              ? 'Code secret du client (laisser vide pour conserver le secret enregistré)'
              : 'Code secret du client'
          }
        >
          <Input
            required={!connector}
            type="password"
            autoComplete="off"
            maxLength={2000}
            value={secret}
            onChange={(e) => setSecret(e.target.value)}
          />
        </Field>
      </div>
      <label className="flex items-center gap-2 text-sm">
        <Checkbox checked={writeTools} onChange={(e) => setWriteTools(e.target.checked)} />
        Permettre l’envoi d’e-mails et la création de rendez-vous, chacun validé
      </label>
      <p className="text-xs text-muted">
        Le client est celui que l’entreprise crée dans son projet Google Cloud (type « Application
        Web »), les API Gmail et Google Calendar activées, avec les champs d’application{' '}
        <code>gmail.readonly</code> et <code>calendar.readonly</code>, et, si l’envoi est permis,{' '}
        <code>gmail.send</code> et <code>calendar.events</code> ; jamais de délégation à l’échelle
        du domaine : l’agent ne lit et n’écrit que ce que la personne servie peut lire et écrire,
        après son accord, avec son compte Workspace. Un e-mail ou un rendez-vous n’est jamais envoyé
        sans validation, quel que soit le niveau d’autonomie ; ceux qui ont relié leur compte avant
        que l’envoi soit permis le relient de nouveau pour l’accorder. Le secret est chiffré sur le
        serveur et jamais réaffiché.
      </p>
      {action.error && <ErrorNote message={action.error} />}
      <div className="flex gap-2">
        <Button type="submit" loading={action.pending}>
          {connector ? 'Enregistrer' : 'Déclarer le client'}
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
