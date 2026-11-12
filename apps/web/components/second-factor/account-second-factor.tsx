'use client';

import { Card, Loaded } from '@/components/ui';
import { useApiData, type SecondFactorStatus } from '@/lib/api';
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
    return <AccountSecondFactorOn status={status} onChange={onChange} />;
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
