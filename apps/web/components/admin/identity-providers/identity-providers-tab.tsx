'use client';

import { Plus } from 'lucide-react';
import { useState } from 'react';
import { Button, Card, EmptyState, ErrorNote, Loaded } from '@/components/ui';
import { useApiData, type IdentityProvidersView } from '@/lib/api';
import { IdentityProviderCard } from './identity-provider-card';
import { IdentityProviderForm } from './identity-provider-form';

/**
 * The company's identity providers (ADR-063): its collaborators sign in
 * with the account they already have, offered by the sign-in page from
 * their address. No account is created: each must exist here.
 */
export function IdentityProvidersTab() {
  const state = useApiData<IdentityProvidersView>('/admin/identity-providers');
  const [adding, setAdding] = useState(false);
  return (
    <Loaded state={state}>
      {(data) => (
        <div className="flex flex-col gap-4">
          <Card className="flex flex-col gap-2 p-4">
            <p className="font-medium">Connexion par le fournisseur d’identité de l’entreprise</p>
            <p className="text-sm text-muted">
              Vos collaborateurs se connectent avec le compte qu’ils ont déjà (Microsoft 365, Google
              Workspace…) : la page de connexion le leur propose d’après leur adresse e-mail. Leur
              compte doit exister ici et être actif ; la connexion par mot de passe reste possible.
            </p>
            <p className="text-sm">
              Adresse de retour à enregistrer chez le fournisseur : <code>{data.redirectUri}</code>
            </p>
          </Card>
          {!data.secretsAvailable ? (
            <ErrorNote message="Ce serveur ne peut pas conserver de secret client : l’exploitant n’a pas configuré la clé maîtresse (MIMORYA_MASTER_KEY)." />
          ) : adding ? (
            <Card className="p-4">
              <p className="mb-3 font-medium">Nouveau fournisseur d’identité</p>
              <IdentityProviderForm
                onDone={(changed) => {
                  setAdding(false);
                  if (changed) state.reload();
                }}
              />
            </Card>
          ) : (
            <Button variant="secondary" className="w-fit" onClick={() => setAdding(true)}>
              <Plus className="size-4" aria-hidden /> Ajouter un fournisseur
            </Button>
          )}
          {data.providers.length === 0 && <EmptyState title="Aucun fournisseur d’identité" />}
          {data.providers.map((provider) => (
            <IdentityProviderCard key={provider.id} provider={provider} onChanged={state.reload} />
          ))}
        </div>
      )}
    </Loaded>
  );
}
