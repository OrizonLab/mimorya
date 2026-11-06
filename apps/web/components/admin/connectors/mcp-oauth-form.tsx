'use client';

import { useState, type FormEvent } from 'react';
import { Button, ErrorNote, Field, Input, Radio } from '@/components/ui';
import { api, useAction, type ConnectorView, type McpOAuthDeclaration } from '@/lib/api';
import { REGISTERED_CLIENT_STAYS } from './registered-client';

/**
 * Declares OAuth on an MCP server (ADR-171): the client registered at its
 * authorization server by hand, or the platform's own, which it registers
 * there itself (ADR-173) when it has its master key; the authorization
 * server is found from the server itself, its issuer given only when the
 * server names several, or none. The company authorizes it once, or each
 * person for themself in their account (ADR-174). Declared again,
 * everything is replaced, the secret too, and the authorizations given
 * under the former declaration go. A typed token goes as well: OAuth
 * replaces it.
 */
export function McpOAuthForm({
  connector,
  redirectUri,
  secretsAvailable,
  onDone,
}: {
  connector: ConnectorView;
  /** Where the authorization server sends the browser back. */
  redirectUri: string;
  secretsAvailable: boolean;
  onDone: (changed: boolean) => void;
}) {
  const declared = connector.mcp?.oauth ?? null;
  const [issuer, setIssuer] = useState(declared?.issuer ?? '');
  // The platform registers itself only where it can seal what is granted.
  const [register, setRegister] = useState(secretsAvailable && (declared?.registered ?? false));
  const [clientId, setClientId] = useState(declared?.registered ? '' : (declared?.clientId ?? ''));
  const [secret, setSecret] = useState('');
  const [scopes, setScopes] = useState(declared?.scopes.join(' ') ?? '');
  const [perPerson, setPerPerson] = useState(declared?.perPerson ?? false);
  const action = useAction();

  async function submit(event: FormEvent) {
    event.preventDefault();
    // What declaring again leaves behind, said before it is done.
    const warnings: string[] = [];
    if (declared?.authorization) {
      warnings.push(
        perPerson
          ? 'Déclarer de nouveau retire l’autorisation de l’entreprise : chaque personne autorisera ensuite ce serveur pour elle-même.'
          : 'Déclarer de nouveau retire l’autorisation donnée : il faudra autoriser ce serveur de nouveau.',
      );
    }
    if (declared && declared.personAuthorizations > 0) {
      warnings.push(
        perPerson
          ? 'Déclarer de nouveau retire les autorisations des personnes : chacune devra autoriser ce serveur de nouveau.'
          : `Déclarer de nouveau retire les autorisations des personnes (${declared.personAuthorizations}) : l’entreprise autorisera ensuite ce serveur pour tous les runs.`,
      );
    }
    if (declared?.registered) {
      warnings.push(REGISTERED_CLIENT_STAYS);
    }
    if (warnings.length > 0 && !window.confirm(`${warnings.join(' ')} Continuer ?`)) return;
    const asked = scopes.trim();
    const body = {
      issuer: issuer.trim() || undefined,
      register: register || undefined,
      clientId: register ? undefined : clientId,
      clientSecret: register ? undefined : secret.trim() || undefined,
      scopes: asked ? asked.split(/\s+/) : undefined,
      perPerson,
    } satisfies McpOAuthDeclaration;
    const saved = await action.run(() => api.put(`/admin/connectors/${connector.id}/oauth`, body));
    if (saved) onDone(true);
  }

  return (
    <form onSubmit={submit} className="flex flex-col gap-3">
      {secretsAvailable && (
        <fieldset className="flex flex-col gap-1 text-sm">
          <legend className="mb-1 font-medium">Client</legend>
          <label className="flex items-center gap-2">
            <Radio
              name={`client-${connector.id}`}
              checked={register}
              onChange={() => setRegister(true)}
            />
            La plateforme s’enregistre elle-même auprès du serveur d’autorisation
          </label>
          <label className="flex items-center gap-2">
            <Radio
              name={`client-${connector.id}`}
              checked={!register}
              onChange={() => setRegister(false)}
            />
            Client enregistré à la main
          </label>
        </fieldset>
      )}
      <fieldset className="flex flex-col gap-1 text-sm">
        <legend className="mb-1 font-medium">Qui autorise ce serveur</legend>
        <label className="flex items-center gap-2">
          <Radio
            name={`holder-${connector.id}`}
            checked={!perPerson}
            onChange={() => setPerPerson(false)}
          />
          L’entreprise, une fois pour tous les runs
        </label>
        <label className="flex items-center gap-2">
          <Radio
            name={`holder-${connector.id}`}
            checked={perPerson}
            onChange={() => setPerPerson(true)}
          />
          Chaque personne, pour elle-même, dans « Mon compte »
        </label>
      </fieldset>
      <p className="text-sm text-muted">
        {register ? (
          <>
            La plateforme s’enregistre comme client auprès du serveur d’autorisation, pour l’adresse
            de retour <code>{redirectUri}</code>, s’il le permet sans jeton d’enregistrement.
          </>
        ) : (
          <>
            Enregistrez d’abord la plateforme comme client auprès du serveur d’autorisation, avec
            l’adresse de retour <code>{redirectUri}</code>.
          </>
        )}{' '}
        La plateforme trouve ce serveur à partir du serveur MCP : n’indiquez son émetteur que s’il
        en annonce plusieurs, ou aucun.
        {!secretsAvailable &&
          ' Sans clé maître, la plateforme ne peut pas s’enregistrer elle-même : enregistrez le client à la main.'}
      </p>
      <div className="grid gap-3 sm:grid-cols-2">
        {!register && (
          <Field label="Identifiant du client">
            <Input
              required
              maxLength={255}
              autoComplete="off"
              value={clientId}
              onChange={(e) => setClientId(e.target.value)}
            />
          </Field>
        )}
        {secretsAvailable && !register && (
          <Field
            label={
              declared
                ? 'Secret du client (à saisir de nouveau ; vide : client public)'
                : 'Secret du client (vide : client public)'
            }
          >
            <Input
              type="password"
              autoComplete="off"
              maxLength={512}
              value={secret}
              onChange={(e) => setSecret(e.target.value)}
            />
          </Field>
        )}
        <Field label="Émetteur du serveur d’autorisation (facultatif)">
          <Input
            type="url"
            maxLength={2000}
            placeholder="https://auth.exemple.fr"
            value={issuer}
            onChange={(e) => setIssuer(e.target.value)}
          />
        </Field>
        <Field label="Portées demandées, séparées par des espaces (vide : celles du serveur)">
          <Input maxLength={4100} value={scopes} onChange={(e) => setScopes(e.target.value)} />
        </Field>
      </div>
      {connector.mcp?.tokenStored && (
        <p className="text-xs text-muted">Le jeton enregistré est retiré : OAuth le remplace.</p>
      )}
      {action.error && <ErrorNote message={action.error} />}
      <div className="flex gap-2">
        <Button type="submit" loading={action.pending}>
          Déclarer
        </Button>
        <Button
          type="button"
          variant="ghost"
          disabled={action.pending}
          onClick={() => onDone(false)}
        >
          Annuler
        </Button>
      </div>
    </form>
  );
}
