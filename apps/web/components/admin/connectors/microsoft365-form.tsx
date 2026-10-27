'use client';

import { useState, type FormEvent } from 'react';
import { Button, ErrorNote, Field, Input } from '@/components/ui';
import {
  api,
  useAction,
  type ConnectorUpdate,
  type ConnectorView,
  type NewMicrosoft365Connector,
} from '@/lib/api';

const GUID = '[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}';

/**
 * Declares the company's Microsoft 365 application (ADR-116, ADR-118), or
 * changes its name and secret: its directory and client IDs stay, as the
 * links its people made there do.
 */
export function Microsoft365Form({
  connector,
  onDone,
}: {
  connector?: ConnectorView;
  onDone: (changed: boolean) => void;
}) {
  const [name, setName] = useState(connector?.name ?? 'Microsoft 365');
  const [directory, setDirectory] = useState('');
  const [clientId, setClientId] = useState('');
  const [secret, setSecret] = useState('');
  const action = useAction();

  async function submit(event: FormEvent) {
    event.preventDefault();
    const saved = await action.run(() =>
      connector
        ? api.patch(`/admin/connectors/${connector.id}`, {
            name,
            // Left out when blank: the stored secret is kept.
            token: secret.trim() || undefined,
          } satisfies ConnectorUpdate)
        : api.post('/admin/connectors/microsoft365', {
            name,
            directory: directory.trim(),
            clientId: clientId.trim(),
            clientSecret: secret.trim(),
          } satisfies NewMicrosoft365Connector),
    );
    if (saved) onDone(true);
  }

  return (
    <form aria-label="Application Microsoft 365" onSubmit={submit} className="flex flex-col gap-3">
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
          <>
            <Field label="ID de l’annuaire (locataire)">
              <Input
                required
                pattern={GUID}
                placeholder="xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx"
                value={directory}
                onChange={(e) => setDirectory(e.target.value)}
              />
            </Field>
            <Field label="ID de l’application (client)">
              <Input
                required
                pattern={GUID}
                placeholder="xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx"
                value={clientId}
                onChange={(e) => setClientId(e.target.value)}
              />
            </Field>
          </>
        )}
        <Field
          label={
            connector
              ? 'Secret de l’application (laisser vide pour conserver le secret enregistré)'
              : 'Secret de l’application'
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
      <p className="text-xs text-muted">
        L’application est celle que l’entreprise inscrit dans son annuaire Microsoft Entra, avec les
        permissions déléguées <code>Mail.Read</code>, <code>Calendars.Read</code> et{' '}
        <code>offline_access</code>, jamais de permission d’application : l’agent ne lit que ce que
        la personne servie peut lire, après son accord. Le secret est chiffré sur le serveur et
        jamais réaffiché.
      </p>
      {action.error && <ErrorNote message={action.error} />}
      <div className="flex gap-2">
        <Button type="submit" loading={action.pending}>
          {connector ? 'Enregistrer' : 'Déclarer l’application'}
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
