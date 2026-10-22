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
 * their address; an invited one is activated by its first sign-in there
 * (ADR-064). The company's sign-in link may also create the account at the
 * first sign-in, when the provider is told to (ADR-067). A SAML provider is
 * given the company's own entity ID and the address it posts to (ADR-068).
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
              Workspace, fournisseur OpenID Connect ou SAML…) : la page de connexion le leur propose
              d’après leur adresse e-mail, et le lien de connexion de l’entreprise sans qu’ils la
              saisissent. Un compte invité s’active ainsi, sans choisir de mot de passe ; un
              fournisseur peut aussi créer le compte d’un collaborateur à sa première connexion par
              ce lien. La connexion par mot de passe reste possible.
            </p>
            <p className="text-sm">
              Adresse de retour à enregistrer chez le fournisseur : <code>{data.redirectUri}</code>
            </p>
            <p className="text-sm">
              Pour un fournisseur SAML, identifiant de l’entreprise (adresse de ses métadonnées) :{' '}
              <code>{data.samlEntityId}</code> ; adresse de réception des réponses :{' '}
              <code>{data.samlAcsUrl}</code>
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
