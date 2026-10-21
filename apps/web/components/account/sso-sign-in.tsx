'use client';

import { useEffect, useRef, useState } from 'react';
import { Button, ErrorNote } from '@/components/ui';
import {
  api,
  ApiError,
  useAction,
  type SsoDiscovery,
  type SsoLookup,
  type SsoStart,
} from '@/lib/api';

const FAILED =
  'La connexion par votre fournisseur d’identité n’a pas abouti. Réessayez, ou connectez-vous avec votre mot de passe (compte invité : depuis le lien de votre invitation).';

const LIMITED =
  'Trop d’adresses essayées depuis votre réseau : la connexion par le fournisseur d’identité de votre entreprise sera de nouveau proposée dans une minute.';

/** The API's window for the addresses asked about: asked again after it. */
const RETRY_AFTER_MS = 60_000;

/** Looks like an address worth asking about. */
const ADDRESS = /^[^@\s]+@[^@\s]+\.[^@\s]+$/;

/**
 * Signing in through the company's identity provider (ADR-063): offered
 * when the address's account belongs to a company that uses one; the
 * browser then goes to the provider, which sends it back signed in, or to
 * the sign-in page with `?sso=echec`. On an invitation's page, the same
 * sign-in accepts it, without a password (ADR-064). The page learns the
 * provider found (`onProvider`): a required one leaves no password to type
 * (ADR-066).
 */
export function SsoSignIn({
  email,
  invitation = false,
  onProvider,
}: {
  email: string;
  invitation?: boolean;
  onProvider?: (provider: SsoDiscovery['provider']) => void;
}) {
  const [provider, setProvider] = useState<SsoDiscovery['provider']>(null);
  const [failed, setFailed] = useState(false);
  // Asked about too many addresses: said, rather than offering nothing.
  const [limited, setLimited] = useState(false);
  const action = useAction();
  // The page's latest callback, without asking again whenever it renders.
  const tell = useRef(onProvider);
  useEffect(() => {
    tell.current = onProvider;
  });

  useEffect(() => {
    function read() {
      setFailed(new URLSearchParams(window.location.search).get('sso') === 'echec');
    }
    read();
  }, []);

  useEffect(() => {
    let current = true;
    let retry: number | undefined;
    function show(found: SsoDiscovery['provider']) {
      setProvider(found);
      tell.current?.(found);
    }
    function discover() {
      // Never another address's provider while this one is asked about.
      show(null);
      setLimited(false);
      if (!ADDRESS.test(email)) return;
      api
        .post<SsoDiscovery>('/auth/sso/discover', { email } satisfies SsoLookup)
        .then((found) => {
          if (current) show(found.provider);
        })
        // The password stays: nothing to offer then.
        .catch((error: unknown) => {
          if (!current) return;
          show(null);
          const tooMany = error instanceof ApiError && error.status === 429;
          setLimited(tooMany);
          if (tooMany) retry = window.setTimeout(discover, RETRY_AFTER_MS);
        });
    }
    discover();
    return () => {
      current = false;
      window.clearTimeout(retry);
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
      {limited && <ErrorNote message={LIMITED} />}
      {provider && (
        <div className="flex flex-col gap-2 rounded-lg border border-line p-3">
          <p className="text-sm text-muted">
            {invitation
              ? `Votre entreprise vous connecte avec ${provider.name} : activez votre compte avec lui, sans choisir de mot de passe.`
              : provider.required
                ? `Votre entreprise impose la connexion avec ${provider.name}.`
                : `Votre entreprise vous connecte avec ${provider.name}.`}
          </p>
          <Button type="button" loading={action.pending} onClick={start}>
            {invitation
              ? `Activer mon compte avec ${provider.name}`
              : `Se connecter avec ${provider.name}`}
          </Button>
          {action.error && <ErrorNote message={action.error} />}
        </div>
      )}
    </>
  );
}
