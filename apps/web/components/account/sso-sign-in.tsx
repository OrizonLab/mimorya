'use client';

import { useEffect, useState } from 'react';
import { Button, ErrorNote } from '@/components/ui';
import { api, useAction, type SsoDiscovery, type SsoLookup, type SsoStart } from '@/lib/api';

const FAILED =
  'La connexion par votre fournisseur d’identité n’a pas abouti. Réessayez, ou connectez-vous avec votre mot de passe.';

/** Looks like an address worth asking about. */
const ADDRESS = /^[^@\s]+@[^@\s]+\.[^@\s]+$/;

/**
 * Signing in through the company's identity provider (ADR-063): offered
 * when the address's account belongs to a company that uses one; the
 * browser then goes to the provider, which sends it back signed in, or to
 * the sign-in page with `?sso=echec`.
 */
export function SsoSignIn({ email }: { email: string }) {
  const [provider, setProvider] = useState<SsoDiscovery['provider']>(null);
  const [failed, setFailed] = useState(false);
  const action = useAction();

  useEffect(() => {
    function read() {
      setFailed(new URLSearchParams(window.location.search).get('sso') === 'echec');
    }
    read();
  }, []);

  useEffect(() => {
    let current = true;
    function discover() {
      if (!ADDRESS.test(email)) {
        setProvider(null);
        return;
      }
      api
        .post<SsoDiscovery>('/auth/sso/discover', { email } satisfies SsoLookup)
        .then((found) => {
          if (current) setProvider(found.provider);
        })
        // The password stays: nothing to offer then.
        .catch(() => {
          if (current) setProvider(null);
        });
    }
    discover();
    return () => {
      current = false;
    };
  }, [email]);

  async function start() {
    const started = await action.run(() =>
      api.post<SsoStart>('/auth/sso/start', { email } satisfies SsoLookup),
    );
    if (started) window.location.assign(started.redirectUrl);
  }

  return (
    <>
      {failed && <ErrorNote message={FAILED} />}
      {provider && (
        <div className="flex flex-col gap-2 rounded-lg border border-line p-3">
          <p className="text-sm text-muted">Votre entreprise vous connecte avec {provider.name}.</p>
          <Button type="button" loading={action.pending} onClick={start}>
            Se connecter avec {provider.name}
          </Button>
          {action.error && <ErrorNote message={action.error} />}
        </div>
      )}
    </>
  );
}
