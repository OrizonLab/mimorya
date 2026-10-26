'use client';

import { Plus } from 'lucide-react';
import { useState, type FormEvent } from 'react';
import { Button, ErrorNote, Field, Input } from '@/components/ui';
import {
  api,
  useAction,
  type ConnectorUpdate,
  type ConnectorView,
  type Connectors,
  type NewRestConnector,
} from '@/lib/api';
import { emptyOperation, fromDrafts, toDrafts, type OperationDraft } from './rest-drafts';
import { RestOperationFields } from './rest-operation-fields';

/**
 * Adds a REST API, or changes its name, address, token and operations
 * (ADR-112, ADR-113). The platform checks the declaration as a whole and
 * says what it refuses; the API is not called until a run needs it.
 */
export function RestForm({
  data,
  connector,
  onDone,
}: {
  data: Connectors;
  connector?: ConnectorView;
  onDone: (changed: boolean) => void;
}) {
  const stored = connector?.rest;
  const [name, setName] = useState(connector?.name ?? '');
  const [prefix, setPrefix] = useState(stored?.prefix ?? '');
  const [url, setUrl] = useState(stored?.url ?? '');
  const [header, setHeader] = useState(stored?.tokenHeader ?? '');
  const [token, setToken] = useState('');
  const [operations, setOperations] = useState<OperationDraft[]>(() =>
    stored ? toDrafts(stored.operations) : [emptyOperation()],
  );
  const action = useAction();
  const moved = stored && url.trim() !== stored.url;

  function change(changed: OperationDraft) {
    setOperations((current) => current.map((o) => (o.key === changed.key ? changed : o)));
  }

  async function submit(event: FormEvent) {
    event.preventDefault();
    const declared = fromDrafts(operations);
    const saved = await action.run(() =>
      connector
        ? api.patch(`/admin/connectors/${connector.id}`, {
            name,
            url,
            // Blank: back to `Authorization: Bearer`.
            tokenHeader: header.trim() || null,
            // Left out when blank: the stored token is kept.
            token: token.trim() || undefined,
            operations: declared,
          } satisfies ConnectorUpdate)
        : api.post('/admin/connectors/rest', {
            name,
            prefix,
            url,
            tokenHeader: header.trim() || undefined,
            token: token.trim() || undefined,
            operations: declared,
          } satisfies NewRestConnector),
    );
    if (saved) onDone(true);
  }

  return (
    <form onSubmit={submit} className="flex flex-col gap-3">
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
        <Field label="Préfixe des outils (définitif)">
          <Input
            required
            disabled={Boolean(connector)}
            pattern="[a-z][a-z0-9]{1,15}"
            placeholder="crm"
            value={prefix}
            onChange={(e) => setPrefix(e.target.value)}
          />
        </Field>
        <Field label={data.allowPrivateNetwork ? 'Adresse de l’API' : 'Adresse de l’API (https)'}>
          <Input
            required
            type="url"
            maxLength={2000}
            placeholder="https://api.exemple.fr/v1"
            value={url}
            onChange={(e) => setUrl(e.target.value)}
          />
        </Field>
        <Field label="En-tête du jeton (vide : Authorization: Bearer)">
          <Input
            maxLength={64}
            pattern="[A-Za-z][A-Za-z0-9\-]*"
            placeholder="X-API-Key"
            value={header}
            onChange={(e) => setHeader(e.target.value)}
          />
        </Field>
        {data.secretsAvailable && (
          <Field
            label={
              stored?.tokenStored && !moved
                ? 'Jeton d’accès (laisser vide pour conserver le jeton enregistré)'
                : 'Jeton d’accès (facultatif)'
            }
          >
            <Input
              type="password"
              autoComplete="off"
              maxLength={2000}
              value={token}
              onChange={(e) => setToken(e.target.value)}
            />
          </Field>
        )}
      </div>
      <p className="text-xs text-muted">
        Chaque opération devient un outil, qui demande une validation tant qu’il n’est pas associé à
        des permissions et à un risque. Le jeton (lettres sans accent, chiffres et - . _ ~ + /, puis
        éventuellement des =) est chiffré sur le serveur et jamais réaffiché ; il ne suit pas l’API
        vers une autre adresse.{' '}
        {data.allowPrivateNetwork
          ? 'Ce déploiement autorise les API de son réseau privé.'
          : 'Seules les adresses publiques en https sont acceptées.'}
      </p>
      {operations.map((operation, index) => (
        <RestOperationFields
          key={operation.key}
          operation={operation}
          index={index}
          prefix={prefix}
          onChange={change}
          onRemove={
            operations.length > 1
              ? () => setOperations((current) => current.filter((o) => o.key !== operation.key))
              : undefined
          }
        />
      ))}
      <Button
        type="button"
        variant="secondary"
        className="w-fit"
        onClick={() => setOperations((current) => [...current, emptyOperation()])}
      >
        <Plus className="size-4" aria-hidden /> Ajouter une opération
      </Button>
      {action.error && <ErrorNote message={action.error} />}
      <div className="flex gap-2">
        <Button type="submit" loading={action.pending}>
          {connector ? 'Enregistrer' : 'Ajouter l’API'}
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
