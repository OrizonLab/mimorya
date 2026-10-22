'use client';

import { useState } from 'react';
import { Badge, Button, Card, ErrorNote } from '@/components/ui';
import { api, useAction, type IdentityProviderUpdate, type IdentityProviderView } from '@/lib/api';
import { formatDate } from '@/lib/format';
import { IDENTITY_PROVIDER_KINDS } from '@/lib/labels';
import { IdentityProviderForm } from './identity-provider-form';
import { SignInLink } from './sign-in-link';

/** One identity provider: what it is, and what may change here. */
export function IdentityProviderCard({
  provider,
  onChanged,
}: {
  provider: IdentityProviderView;
  onChanged: () => void;
}) {
  const [editing, setEditing] = useState(false);
  const action = useAction();

  async function toggle() {
    const update = { enabled: !provider.enabled } satisfies IdentityProviderUpdate;
    if (await action.run(() => api.patch(`/admin/identity-providers/${provider.id}`, update))) {
      onChanged();
    }
  }

  async function remove() {
    const confirmed = window.confirm(
      `Supprimer « ${provider.name} » ? Ses comptes reliés devront l’être de nouveau s’il est redéclaré.`,
    );
    if (!confirmed) return;
    if (await action.run(() => api.delete(`/admin/identity-providers/${provider.id}`))) {
      onChanged();
    }
  }

  return (
    <Card className="flex flex-col gap-3 p-4">
      <div className="flex flex-wrap items-center gap-2">
        <p className="flex-1 font-medium">{provider.name}</p>
        <Badge>{IDENTITY_PROVIDER_KINDS[provider.kind] ?? provider.kind}</Badge>
        {provider.required && <Badge tone="accent">Obligatoire</Badge>}
        {provider.createAccounts && <Badge tone="accent">Crée les comptes</Badge>}
        <Badge tone={provider.enabled ? 'ok' : 'neutral'}>
          {provider.enabled ? 'Actif' : 'Désactivé'}
        </Badge>
      </div>
      {provider.kind === 'saml' ? (
        // Its client is the company's own entity ID, shown with the tab.
        <p className="text-sm text-muted">
          <code>{provider.issuer}</code> · métadonnées SAML
        </p>
      ) : (
        <p className="text-sm text-muted">
          <code>{provider.issuer}</code> · client <code>{provider.clientId}</code> ·{' '}
          {provider.secretStored ? 'secret enregistré' : 'sans secret'}
        </p>
      )}
      <p className="text-sm">Domaines : {provider.domains.join(', ')}</p>
      {provider.enabled && (
        <SignInLink link={provider.signInLink} createsAccounts={provider.createAccounts} />
      )}
      <p className="text-xs text-muted">Modifié le {formatDate(provider.updatedAt)}</p>
      {action.error && <ErrorNote message={action.error} />}
      <div className="flex flex-wrap gap-2">
        {!editing && (
          <Button variant="secondary" onClick={() => setEditing(true)}>
            Modifier
          </Button>
        )}
        <Button variant="secondary" onClick={toggle}>
          {provider.enabled ? 'Désactiver' : 'Activer'}
        </Button>
        <Button variant="danger" onClick={remove}>
          Supprimer
        </Button>
      </div>
      {editing && (
        <IdentityProviderForm
          provider={provider}
          onDone={(changed) => {
            setEditing(false);
            if (changed) onChanged();
          }}
        />
      )}
    </Card>
  );
}
