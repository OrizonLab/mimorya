'use client';

import { useState, type FormEvent } from 'react';
import { Button, ErrorNote, Field, Input, Select } from '@/components/ui';
import {
  api,
  useAction,
  type IdentityProviderUpdate,
  type IdentityProviderView,
  type NewIdentityProvider,
} from '@/lib/api';
import { IDENTITY_PROVIDER_KINDS } from '@/lib/labels';

/** The domains typed, one by one (separated by commas or spaces). */
function domainList(typed: string): string[] {
  return typed
    .split(/[\s,;]+/)
    .map((domain) => domain.trim())
    .filter(Boolean);
}

/** What the company registers at each kind of provider. */
const GUIDANCE: Record<string, string> = {
  microsoft:
    'Dans Microsoft Entra ID : « Inscriptions d’applications », comptes de cet annuaire uniquement, ' +
    'adresse de retour de type Web ci-dessus ; puis un secret client (« Certificats et secrets »).',
  google:
    'Dans la console Google Cloud : un identifiant client OAuth « Application Web », avec l’adresse ' +
    'de retour ci-dessus. Seuls les comptes Workspace des domaines déclarés sont acceptés.',
  oidc: 'Chez votre fournisseur : un client confidentiel (code d’autorisation), avec l’adresse de retour ci-dessus.',
};

/**
 * Declares an identity provider, or changes one: its name, its client
 * secret (write-only) and the e-mail domains it covers. Kind, issuer and
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
  const [domains, setDomains] = useState(provider?.domains.join(', ') ?? '');
  const action = useAction();

  async function submit(event: FormEvent) {
    event.preventDefault();
    const saved = await action.run(() =>
      provider
        ? api.patch(`/admin/identity-providers/${provider.id}`, {
            name: name.trim() || undefined,
            // Left out when blank: the stored secret is kept.
            clientSecret: clientSecret.trim() || undefined,
            domains: domainList(domains),
          } satisfies IdentityProviderUpdate)
        : api.post('/admin/identity-providers', {
            kind,
            name: name.trim() || undefined,
            directoryId: kind === 'microsoft' ? directoryId.trim() : undefined,
            issuer: kind === 'oidc' ? issuer.trim() : undefined,
            clientId: clientId.trim(),
            clientSecret: clientSecret.trim(),
            domains: domainList(domains),
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
            kind === 'oidc' || provider
              ? 'Nom affiché sur la page de connexion'
              : 'Nom affiché sur la page de connexion (facultatif)'
          }
        >
          <Input
            required={kind === 'oidc' || Boolean(provider)}
            maxLength={80}
            placeholder={
              kind === 'google' ? 'Google Workspace' : kind === 'oidc' ? '' : 'Microsoft 365'
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
        {!provider && (
          <Field label="Identifiant client (ID d’application)">
            <Input
              required
              maxLength={255}
              value={clientId}
              onChange={(e) => setClientId(e.target.value)}
            />
          </Field>
        )}
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
        <Field label="Domaines d’e-mail (séparés par des virgules)">
          <Input
            required
            placeholder="entreprise.fr, filiale.fr"
            value={domains}
            onChange={(e) => setDomains(e.target.value)}
          />
        </Field>
      </div>
      <p className="text-xs text-muted">
        {!provider && `${GUIDANCE[kind] ?? ''} `}Le secret est chiffré sur le serveur et jamais
        réaffiché. Le fournisseur est joint avant l’enregistrement.
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
