'use client';

import { useState, type FormEvent } from 'react';
import { Button, Card, ErrorNote, Loaded, Radio } from '@/components/ui';
import {
  api,
  useAction,
  useApiData,
  type SecondFactorPolicyUpdate,
  type SecondFactorPolicyView,
  type SecondFactorRequirement,
} from '@/lib/api';
import { useSyncedState } from '@/lib/use-synced-state';

const POLICY = '/admin/second-factor';

/** Each requirement, as the administrator chooses it. */
const CHOICES: { value: SecondFactorRequirement; label: string; detail: string }[] = [
  {
    value: 'optional',
    label: 'Personne',
    detail: 'Chacun choisit d’activer le sien depuis « Mon compte ».',
  },
  {
    value: 'administrators',
    label: 'Les administrateurs',
    detail:
      'Les comptes qui détiennent une permission d’administration, sur un périmètre quelconque (valider les actions de son équipe ou déposer des documents n’en est pas une).',
  },
  {
    value: 'everyone',
    label: 'Tous les comptes à mot de passe',
    detail: 'Chaque compte qui se connecte par mot de passe.',
  },
];

/**
 * Who must have a second factor to sign in with a password (ADR-201): set
 * by who manages the company's settings, where the server can keep one.
 * A required account enrols at its next sign-in, before any session; the
 * accounts that sign in through the company's identity provider have
 * theirs there.
 */
export function SecondFactorPolicy() {
  const state = useApiData<SecondFactorPolicyView>(POLICY);
  return (
    <Card className="p-4">
      <section aria-labelledby="second-factor-policy-heading" className="flex flex-col gap-3">
        <h2 id="second-factor-policy-heading" className="font-medium">
          Second facteur
        </h2>
        <Loaded state={state}>{(view) => <PolicyForm view={view} onSaved={state.reload} />}</Loaded>
      </section>
    </Card>
  );
}

function PolicyForm({ view, onSaved }: { view: SecondFactorPolicyView; onSaved: () => void }) {
  // Started again from each reloaded policy (ADR-041).
  const [choice, setChoice] = useSyncedState(view.requirement);
  const [done, setDone] = useState(false);
  const action = useAction();

  async function submit(event: FormEvent) {
    event.preventDefault();
    setDone(false);
    const body = { requirement: choice } satisfies SecondFactorPolicyUpdate;
    if (await action.run(() => api.put(POLICY, body))) {
      setDone(true);
      onSaved();
    }
  }

  return (
    <form onSubmit={submit} aria-label="Second facteur exigé" className="flex flex-col gap-3">
      <p className="text-sm text-muted">
        Après son mot de passe, un code de son application d’authentification est demandé à chaque
        connexion. Un compte à qui il est exigé s’enrôle à sa prochaine connexion, avant toute
        session, et ne peut plus le désactiver ; les sessions déjà ouvertes restent.
      </p>
      {!view.secretsAvailable && (
        <ErrorNote message="Ce serveur ne peut pas conserver de second facteur : l’exploitant n’a pas configuré la clé maîtresse (MIMORYA_MASTER_KEY). Il ne peut donc être exigé de personne." />
      )}
      <fieldset className="flex flex-col gap-2 text-sm">
        <legend className="mb-1 font-medium">Exigé de</legend>
        {CHOICES.map((option) => (
          <label key={option.value} className="flex items-start gap-2">
            <Radio
              name="second-factor-requirement"
              className="mt-0.5"
              checked={choice === option.value}
              disabled={!view.secretsAvailable && option.value !== 'optional'}
              onChange={() => {
                setChoice(option.value);
                setDone(false);
              }}
            />
            <span>
              <span className="font-medium">{option.label}</span>
              <span className="block text-xs text-muted">{option.detail}</span>
            </span>
          </label>
        ))}
      </fieldset>
      {action.error && <ErrorNote message={action.error} />}
      {done && (
        <p role="status" className="text-sm text-ok">
          Enregistré. L’exigence s’applique à la prochaine connexion de chacun.
        </p>
      )}
      <Button
        type="submit"
        className="w-fit"
        loading={action.pending}
        disabled={choice === view.requirement}
      >
        Enregistrer l’exigence
      </Button>
    </form>
  );
}
