'use client';

import { useState, type FormEvent } from 'react';
import { Button, ErrorNote, Field, Input } from '@/components/ui';
import {
  api,
  useAction,
  type RecoveryCodes,
  type SecondFactorCodeInput,
  type SecondFactorEnrolment,
  type SecondFactorEnrolmentRequest,
} from '@/lib/api';
import { EnrolmentSteps } from './enrolment-steps';
import { RecoveryCodesNotice } from './recovery-codes-notice';

/**
 * Turns one's second factor on from « Mon compte » (ADR-200): the password
 * typed again, then the app's QR code and its first code, then the recovery
 * codes, shown once; `onDone` once they are noted. The account's other
 * sessions end.
 */
export function AccountEnrolment({ onDone }: { onDone: () => void }) {
  const [password, setPassword] = useState('');
  const [enrolment, setEnrolment] = useState<SecondFactorEnrolment | null>(null);
  const [codes, setCodes] = useState<string[] | null>(null);
  const start = useAction();
  const confirm = useAction();

  async function begin(event: FormEvent) {
    event.preventDefault();
    const started = await start.run(() =>
      api.post<SecondFactorEnrolment>('/account/second-factor/enrolment', {
        password,
      } satisfies SecondFactorEnrolmentRequest),
    );
    if (started) {
      setPassword('');
      setEnrolment(started);
    }
  }

  async function activate(code: string) {
    const confirmed = await confirm.run(() =>
      api.post<RecoveryCodes>('/account/second-factor/confirmation', {
        code,
      } satisfies SecondFactorCodeInput),
    );
    if (confirmed) setCodes(confirmed.codes);
  }

  if (codes) {
    return (
      <RecoveryCodesNotice
        codes={codes}
        doneLabel="J’ai noté mes codes de secours"
        onDone={onDone}
      />
    );
  }
  if (enrolment) {
    return (
      <EnrolmentSteps
        enrolment={enrolment}
        pending={confirm.pending}
        error={confirm.error}
        onCode={activate}
      />
    );
  }
  return (
    <form onSubmit={begin} aria-label="Activer le second facteur" className="flex flex-col gap-3">
      <p className="text-sm text-muted">
        Après votre mot de passe, un code de votre application d’authentification vous sera demandé
        à chaque connexion. Vos autres sessions seront fermées.
      </p>
      <Field label="Mot de passe actuel">
        <Input
          type="password"
          required
          autoComplete="current-password"
          value={password}
          onChange={(e) => setPassword(e.target.value)}
        />
      </Field>
      {start.error && <ErrorNote message={start.error} />}
      <Button type="submit" className="w-fit" loading={start.pending}>
        Activer le second facteur
      </Button>
    </form>
  );
}
