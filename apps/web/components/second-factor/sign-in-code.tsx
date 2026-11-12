'use client';

import { useState, type FormEvent } from 'react';
import { Button, ErrorNote, Field, Input } from '@/components/ui';
import { api, useAction, type SecondFactorCodeInput, type SignIn } from '@/lib/api';

/**
 * The second step of a sign-in (ADR-200): the six digits of the app, or a
 * recovery code for who no longer has it; `onSignedIn` once the session is
 * open.
 */
export function SignInCode({ onSignedIn }: { onSignedIn: () => void }) {
  const [recovery, setRecovery] = useState(false);
  const [code, setCode] = useState('');
  const action = useAction();

  async function submit(event: FormEvent) {
    event.preventDefault();
    const signed = await action.run(() =>
      api.post<SignIn>('/auth/second-factor', {
        code: code.trim(),
      } satisfies SecondFactorCodeInput),
    );
    if (signed) onSignedIn();
  }

  return (
    <form onSubmit={submit} aria-label="Second facteur" className="flex flex-col gap-4">
      <p className="text-sm text-muted">
        {recovery
          ? 'Saisissez l’un de vos codes de secours : il ne servira qu’une fois.'
          : 'Saisissez le code à six chiffres affiché par votre application d’authentification.'}
      </p>
      {recovery ? (
        <Field label="Code de secours">
          <Input
            required
            autoComplete="off"
            maxLength={64}
            value={code}
            onChange={(e) => setCode(e.target.value)}
            className="font-mono"
          />
        </Field>
      ) : (
        <Field label="Code à six chiffres">
          <Input
            required
            inputMode="numeric"
            autoComplete="one-time-code"
            pattern="[0-9]{6}"
            maxLength={6}
            value={code}
            onChange={(e) => setCode(e.target.value)}
            className="font-mono"
          />
        </Field>
      )}
      {action.error && <ErrorNote message={action.error} />}
      <Button type="submit" loading={action.pending}>
        Se connecter
      </Button>
      <button
        type="button"
        onClick={() => {
          setRecovery(!recovery);
          setCode('');
        }}
        className="text-center text-sm text-muted hover:text-fg"
      >
        {recovery ? 'Utiliser mon application' : 'Utiliser un code de secours'}
      </button>
    </form>
  );
}
