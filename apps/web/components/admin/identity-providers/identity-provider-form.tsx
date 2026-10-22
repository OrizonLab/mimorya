'use client';

import { useState, type FormEvent } from 'react';
import { Button, Checkbox, ErrorNote, Field, Input, Select, Textarea } from '@/components/ui';
import {
  api,
  useAction,
  type IdentityProviderUpdate,
  type IdentityProviderView,
  type NewIdentityProvider,
} from '@/lib/api';
import { IDENTITY_PROVIDER_KINDS } from '@/lib/labels';
import { PROVIDER_GUIDANCE } from './provider-guidance';

/** The domains typed, one by one (separated by commas or spaces). */
function domainList(typed: string): string[] {
  return typed
    .split(/[\s,;]+/)
    .map((domain) => domain.trim())
    .filter(Boolean);
}

/** The most a provider's metadata may weigh (ADR-068). */
const METADATA_MAX = 256 * 1024;

/**
 * Declares an identity provider, or changes one: its name, its client
 * secret (write-only) or, for SAML, its metadata (its certificates rolled
 * over, ADR-068), the e-mail domains it covers, whether it is required for
 * them (ADR-066) and whether it creates the accounts of their people at
 * the first sign-in through the company's link (ADR-067). Kind, issuer and
 * client stay as declared (ADR-063).
 */
export function IdentityProviderForm({
  provider,
  onDone,
}: {
  provider?: IdentityProviderView;
  onDone: (changed: boolean) => void;
}) {
  const [kind, setKind] = useState(provider?.kind ?? 'microsoft');
  const [name, setName] = useState(provider?.name ?? '');
  const [directoryId, setDirectoryId] = useState('');
  const [issuer, setIssuer] = useState('');
  const [clientId, setClientId] = useState('');
  const [clientSecret, setClientSecret] = useState('');
  const [metadata, setMetadata] = useState('');
  const [domains, setDomains] = useState(provider?.domains.join(', ') ?? '');
  const [required, setRequired] = useState(provider?.required ?? false);
  const [createAccounts, setCreateAccounts] = useState(provider?.createAccounts ?? false);
  const action = useAction();
  // Declared by its metadata alone: no client, no secret.
  const saml = kind === 'saml';

  async function submit(event: FormEvent) {
    event.preventDefault();
    const saved = await action.run(() =>
      provider
        ? api.patch(`/admin/identity-providers/${provider.id}`, {
            name: name.trim() || undefined,
            // Left out when blank: the stored secret, or metadata, is kept.
            clientSecret: clientSecret.trim() || undefined,
            metadata: metadata.trim() || undefined,
            domains: domainList(domains),
            required,
            createAccounts,
          } satisfies IdentityProviderUpdate)
        : api.post('/admin/identity-providers', {
            kind,
            name: name.trim() || undefined,
            directoryId: kind === 'microsoft' ? directoryId.trim() : undefined,
            issuer: kind === 'oidc' ? issuer.trim() : undefined,
            clientId: saml ? undefined : clientId.trim(),
            clientSecret: saml ? undefined : clientSecret.trim(),
            metadata: saml ? metadata.trim() : undefined,
            domains: domainList(domains),
            required,
            createAccounts,
          } satisfies NewIdentityProvider),
    );
    if (saved) onDone(true);
  }

  return (
    <form onSubmit={submit} className="flex flex-col gap-3">
      <div className="grid gap-3 sm:grid-cols-2">
        {!provider && (
          <Field label="Type de fournisseur">
            <Select value={kind} onChange={(e) => setKind(e.target.value)}>
              {Object.entries(IDENTITY_PROVIDER_KINDS).map(([value, label]) => (
                <option key={value} value={value}>
                  {label}
                </option>
              ))}
            </Select>
          </Field>
        )}
        <Field
          label={
            // Once declared, a provider keeps a name: changed, never emptied.
            kind === 'oidc' || saml || provider
              ? 'Nom affiché sur la page de connexion'
              : 'Nom affiché sur la page de connexion (facultatif)'
          }
        >
          <Input
            required={kind === 'oidc' || saml || Boolean(provider)}
            maxLength={80}
            placeholder={
              kind === 'microsoft' ? 'Microsoft 365' : kind === 'google' ? 'Google Workspace' : ''
            }
            value={name}
            onChange={(e) => setName(e.target.value)}
          />
        </Field>
        {!provider && kind === 'microsoft' && (
          <Field label="Identifiant de l’annuaire (ID de locataire)">
            <Input
              required
              placeholder="xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx"
              value={directoryId}
              onChange={(e) => setDirectoryId(e.target.value)}
            />
          </Field>
        )}
        {!provider && kind === 'oidc' && (
          <Field label="Adresse de l’émetteur">
            <Input
              required
              type="url"
              maxLength={2000}
              placeholder="https://id.exemple.fr/realms/entreprise"
              value={issuer}
              onChange={(e) => setIssuer(e.target.value)}
            />
          </Field>
        )}
        {!provider && !saml && (
          <Field label="Identifiant client (ID d’application)">
            <Input
              required
              maxLength={255}
              value={clientId}
              onChange={(e) => setClientId(e.target.value)}
            />
          </Field>
        )}
        {!saml && (
          <Field
            label={
              provider
                ? 'Nouveau secret client (laisser vide pour conserver le secret enregistré)'
                : 'Secret client'
            }
          >
            <Input
              required={!provider}
              type="password"
              autoComplete="off"
              maxLength={1024}
              value={clientSecret}
              onChange={(e) => setClientSecret(e.target.value)}
            />
          </Field>
        )}
        <Field label="Domaines d’e-mail (séparés par des virgules)">
          <Input
            required
            placeholder="entreprise.fr, filiale.fr"
            value={domains}
            onChange={(e) => setDomains(e.target.value)}
          />
        </Field>
        {saml && (
          <Field
            className="sm:col-span-2"
            label={
              provider
                ? 'Nouvelles métadonnées du fournisseur, pour un certificat renouvelé (laisser vide pour conserver celles enregistrées)'
                : 'Métadonnées du fournisseur (XML)'
            }
          >
            <Textarea
              required={!provider}
              rows={6}
              maxLength={METADATA_MAX}
              spellCheck={false}
              className="font-mono"
              placeholder="<EntityDescriptor …>"
              value={metadata}
              onChange={(e) => setMetadata(e.target.value)}
            />
          </Field>
        )}
      </div>
      <label className="flex items-start gap-2 text-sm">
        <Checkbox
          className="mt-0.5"
          checked={required}
          onChange={(e) => setRequired(e.target.checked)}
        />
        <span>
          Connexion obligatoire par ce fournisseur pour ses domaines : le mot de passe y est refusé,
          sauf aux administrateurs de la connexion de l’entreprise, qui gardent le leur en cas de
          panne du fournisseur.
        </span>
      </label>
      <label className="flex items-start gap-2 text-sm">
        <Checkbox
          className="mt-0.5"
          checked={createAccounts}
          onChange={(e) => setCreateAccounts(e.target.checked)}
        />
        <span>
          Créer le compte d’un collaborateur de ces domaines à sa première connexion par le lien de
          connexion de l’entreprise : actif, sans rôle ni unité, à son nom tel que le fournisseur le
          donne.
        </span>
      </label>
      <p className="text-xs text-muted">
        {!provider && `${PROVIDER_GUIDANCE[kind] ?? ''} `}
        {saml
          ? 'Les métadonnées sont lues avant l’enregistrement : identifiant, adresse de connexion et certificats de signature du fournisseur.'
          : 'Le secret est chiffré sur le serveur et jamais réaffiché. Le fournisseur est joint avant l’enregistrement.'}
      </p>
      {action.error && <ErrorNote message={action.error} />}
      <div className="flex gap-2">
        <Button type="submit" loading={action.pending}>
          {provider ? 'Enregistrer' : 'Ajouter le fournisseur'}
        </Button>
        <Button
          type="button"
          variant="ghost"
          // Closed before its change is answered, it could be reopened from the
          // former provider, and that change overwritten.
          disabled={action.pending}
          onClick={() => onDone(false)}
        >
          Annuler
        </Button>
      </div>
    </form>
  );
}
