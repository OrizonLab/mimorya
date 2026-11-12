'use client';

import { useState } from 'react';
import { api, useAction, type RecoveryCodes, type SecondFactorCodeInput } from '@/lib/api';
import { CodeForm } from './code-form';
import { RecoveryCodesNotice } from './recovery-codes-notice';

/**
 * One's second factor once on (ADR-200): new recovery codes, which replace
 * the others, and turning it off, each proven by a current code; turning it
 * off is not offered where the company requires one (ADR-201).
 */
export function AccountSecondFactorOn({
  required,
  onChange,
}: {
  required: boolean;
  onChange: () => void;
}) {
  const [codes, setCodes] = useState<string[] | null>(null);
  const renew = useAction();
  const off = useAction();

  async function renewCodes(code: string) {
    const renewed = await renew.run(() =>
      api.post<RecoveryCodes>('/account/second-factor/recovery-codes', {
        code,
      } satisfies SecondFactorCodeInput),
    );
    if (renewed) setCodes(renewed.codes);
    return renewed !== null;
  }

  async function turnOff(code: string) {
    const done = await off.run(() =>
      api.post('/account/second-factor/deactivation', { code } satisfies SecondFactorCodeInput),
    );
    if (done) onChange();
    return done !== null;
  }

  if (codes) {
    return (
      <RecoveryCodesNotice
        codes={codes}
        doneLabel="J’ai noté mes nouveaux codes"
        onDone={() => {
          setCodes(null);
          onChange();
        }}
      />
    );
  }
  return (
    <div className="flex flex-col gap-5">
      <div className="flex flex-col gap-2">
        <h3 className="text-sm font-medium">Nouveaux codes de secours</h3>
        <p className="text-sm text-muted">Les codes de secours précédents ne serviront plus.</p>
        <CodeForm
          label="Nouveaux codes de secours"
          submit="Remplacer mes codes de secours"
          pending={renew.pending}
          error={renew.error}
          onCode={renewCodes}
        />
      </div>
      {required ? (
        <p className="text-sm text-muted">
          Votre entreprise exige un second facteur : il ne peut pas être désactivé. Pour changer
          d’application, demandez à un administrateur de le réinitialiser.
        </p>
      ) : (
        <div className="flex flex-col gap-2">
          <h3 className="text-sm font-medium">Désactiver le second facteur</h3>
          <p className="text-sm text-muted">
            Votre mot de passe suffira de nouveau pour vous connecter.
          </p>
          <CodeForm
            label="Désactiver le second facteur"
            submit="Désactiver"
            pending={off.pending}
            error={off.error}
            onCode={turnOff}
          />
        </div>
      )}
    </div>
  );
}
