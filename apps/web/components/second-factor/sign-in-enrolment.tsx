'use client';

import { useState } from 'react';
import { Button, ErrorNote } from '@/components/ui';
import {
  api,
  useAction,
  type RecoveryCodes,
  type SecondFactorCodeInput,
  type SecondFactorEnrolment,
} from '@/lib/api';
import { EnrolmentSteps } from './enrolment-steps';
import { RecoveryCodesNotice } from './recovery-codes-notice';

/**
 * The enrolment a sign-in waits for when the company requires a second
 * factor (ADR-201): its start, the app's QR code and first code, which open
 * the session, then the recovery codes, shown once; `onSignedIn` once they
 * are noted. The password was just checked: it is not asked again.
 */
export function SignInEnrolment({ onSignedIn }: { onSignedIn: () => void }) {
  const [enrolment, setEnrolment] = useState<SecondFactorEnrolment | null>(null);
  const [codes, setCodes] = useState<string[] | null>(null);
  const start = useAction();
  const confirm = useAction();

  async function begin() {
    const started = await start.run(() =>
      api.post<SecondFactorEnrolment>('/auth/second-factor/enrolment'),
    );
    if (started) setEnrolment(started);
  }

  async function activate(code: string) {
    const confirmed = await confirm.run(() =>
      api.post<RecoveryCodes>('/auth/second-factor/enrolment/confirmation', {
        code,
      } satisfies SecondFactorCodeInput),
    );
    if (confirmed) setCodes(confirmed.codes);
  }

  if (codes) {
    return (
      <RecoveryCodesNotice
        codes={codes}
        doneLabel="J’ai noté mes codes, continuer"
        onDone={onSignedIn}
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
    <div className="flex flex-col gap-4">
      <p className="text-sm">
        Votre entreprise exige un second facteur pour se connecter par mot de passe. Activez-le
        maintenant avec une application d’authentification : son code vous sera demandé à chaque
        connexion.
      </p>
      {start.error && <ErrorNote message={start.error} />}
      <Button type="button" loading={start.pending} onClick={begin}>
        Activer le second facteur
      </Button>
    </div>
  );
}
