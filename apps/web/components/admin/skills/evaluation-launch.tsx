'use client';

import { useState, type FormEvent } from 'react';
import { Button, ErrorNote, Field, Select } from '@/components/ui';
import { api, useAction, type EvaluationRequest, type SkillVersionView } from '@/lib/api';

/**
 * Asks for a version to be tried on the skill's cases (ADR-104), the
 * latest one not active by default. A proposal of the agent's is read
 * before it is played: the assistant follows it with the rights of whom
 * asked for the evaluation, reading only.
 */
export function EvaluationLaunch({
  skillId,
  versions,
  activeVersion,
  underWay,
  onLaunched,
}: {
  skillId: string;
  versions: SkillVersionView[];
  activeVersion: number | null;
  underWay: boolean;
  onLaunched: () => void;
}) {
  const fresh = versions.find((v) => v.version !== activeVersion && v.status !== 'rejected');
  const [version, setVersion] = useState(fresh?.version ?? versions[0]?.version ?? 1);
  const action = useAction();
  const chosen = versions.find((v) => v.version === version);
  const proposal = chosen?.proposed === true && chosen.status === 'candidate';

  async function submit(event: FormEvent) {
    event.preventDefault();
    if (
      proposal &&
      !window.confirm(
        `La version ${version} est une proposition de l’agent. L’évaluer la fait suivre à l’assistant avec vos droits, en lecture seule. Avez-vous relu son contenu ?`,
      )
    ) {
      return;
    }
    const body = { version } satisfies EvaluationRequest;
    if (await action.run(() => api.post(`/admin/skills/${skillId}/evaluations`, body))) {
      onLaunched();
    }
  }

  return (
    <form onSubmit={submit} className="flex flex-col gap-2">
      <div className="flex flex-wrap items-end gap-2">
        <Field label="Version à évaluer">
          <Select value={version} onChange={(e) => setVersion(Number(e.target.value))}>
            {versions.map((v) => (
              <option key={v.version} value={v.version}>
                Version {v.version}
                {v.version === activeVersion ? ' (active)' : ''}
                {v.proposed && v.status === 'candidate' ? ' (proposée par l’agent)' : ''}
              </option>
            ))}
          </Select>
        </Field>
        <Button type="submit" variant="secondary" loading={action.pending} disabled={underWay}>
          Lancer l’évaluation
        </Button>
      </div>
      {proposal && (
        <p className="text-xs text-warn">
          Proposition de l’agent : relisez son contenu avant de l’évaluer.
        </p>
      )}
      {action.error && <ErrorNote message={action.error} />}
    </form>
  );
}
