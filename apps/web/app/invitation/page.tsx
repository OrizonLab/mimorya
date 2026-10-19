'use client';

import { useEffect, useState } from 'react';
import { InvitationForm, type InvitationView } from '@/components/account';
import { Card, ErrorNote, Spinner } from '@/components/ui';
import { api, ApiError } from '@/lib/api';

type State =
  | { kind: 'loading' }
  | { kind: 'invalid'; message: string }
  | { kind: 'ready'; token: string; invitation: InvitationView };

const INCOMPLETE = 'Ce lien d’invitation est incomplet : ouvrez-le tel qu’il vous a été transmis.';

/** Opened from an invitation link (`/invitation#<token>`), before any sign-in. */
export default function InvitationPage() {
  const [state, setState] = useState<State>({ kind: 'loading' });

  useEffect(() => {
    // The token travels in the fragment: never sent to a server with the page.
    // A link pasted while the page is open only changes the fragment.
    function load() {
      const token = window.location.hash.slice(1);
      if (!token) {
        setState({ kind: 'invalid', message: INCOMPLETE });
        return;
      }
      setState({ kind: 'loading' });
      api
        .post<InvitationView>('/auth/invitation', { token })
        .then((invitation) => setState({ kind: 'ready', token, invitation }))
        .catch((e: unknown) =>
          setState({
            kind: 'invalid',
            message: e instanceof ApiError ? e.message : 'Invitation impossible à vérifier',
          }),
        );
    }
    load();
    window.addEventListener('hashchange', load);
    return () => window.removeEventListener('hashchange', load);
  }, []);

  return (
    <div className="flex min-h-dvh items-center justify-center px-4 py-10">
      <div className="w-full max-w-sm">
        <div className="mb-6 text-center">
          <h1 className="text-2xl font-semibold tracking-tight">Mimorya</h1>
          <p className="mt-1 text-sm text-muted">Votre collaborateur IA d’entreprise</p>
        </div>
        {state.kind === 'loading' && (
          <Card className="flex justify-center p-6">
            <Spinner />
          </Card>
        )}
        {state.kind === 'invalid' && (
          <Card className="p-6">
            <ErrorNote message={state.message} />
          </Card>
        )}
        {state.kind === 'ready' && (
          <InvitationForm
            token={state.token}
            invitation={state.invitation}
            onAccepted={() => window.location.assign('/chat')}
          />
        )}
      </div>
    </div>
  );
}
