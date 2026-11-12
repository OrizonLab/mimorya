'use client';

import { Card, Loaded } from '@/components/ui';
import { useApiData, type SecondFactorStatus } from '@/lib/api';
import { formatDate } from '@/lib/format';
import { AccountEnrolment } from './account-enrolment';
import { AccountSecondFactorOn } from './account-second-factor-on';

/** One's second factor, from « Mon compte » (ADR-200, ADR-201). */
export function AccountSecondFactor() {
  const state = useApiData<SecondFactorStatus>('/account/second-factor');
  return (
    <Card className="max-w-2xl p-4">
      <section aria-labelledby="second-factor-heading" className="flex flex-col gap-3">
        <h2 id="second-factor-heading" className="text-sm font-medium">
          Second facteur
        </h2>
        <Loaded state={state}>
          {(status) => <SecondFactorState status={status} onChange={state.reload} />}
        </Loaded>
      </section>
    </Card>
  );
}

function SecondFactorState({
  status,
  onChange,
}: {
  status: SecondFactorStatus;
  onChange: () => void;
}) {
  if (!status.applicable) {
    return (
      <p className="text-sm text-muted">
        Vous vous connectez par le fournisseur d’identité de votre entreprise : c’est lui qui
        demande votre second facteur.
      </p>
    );
  }
  if (status.enabled) {
    return (
      <>
        <p role="status" className="text-sm">
          Activé{status.enabledAt ? ` le ${formatDate(status.enabledAt)}` : ''} ·{' '}
          {status.recoveryCodesLeft} code{status.recoveryCodesLeft > 1 ? 's' : ''} de secours
          restant{status.recoveryCodesLeft > 1 ? 's' : ''}
          {status.required ? ' · exigé par votre entreprise' : ''}
        </p>
        <AccountSecondFactorOn required={status.required} onChange={onChange} />
      </>
    );
  }
  if (!status.available) {
    return (
      <p className="text-sm text-muted">
        Ce serveur ne peut pas conserver de second facteur : son exploitant n’a pas configuré sa clé
        maîtresse.
      </p>
    );
  }
  return <AccountEnrolment onDone={onChange} />;
}
