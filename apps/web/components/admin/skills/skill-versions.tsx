'use client';

import { Plus } from 'lucide-react';
import { useState } from 'react';
import { Button, ErrorNote, Loaded } from '@/components/ui';
import {
  api,
  ApiError,
  useAction,
  useApiData,
  type SkillActivation,
  type SkillDetail,
  type SkillRejection,
  type SkillView,
} from '@/lib/api';
import { EvalCases } from './eval-cases';
import { EvaluationsSection } from './evaluations-section';
import { VersionForm } from './version-form';
import { VersionItem } from './version-item';

/**
 * The versions of a skill, the latest first: one is made active here, once
 * confirmed against the regressions its last evaluation found (ADR-104),
 * and a proposal of the agent's turned down (ADR-100, ADR-101); then the
 * cases they are tried on and their evaluations (ADR-103, ADR-104).
 */
export function SkillVersions({ skill, onChanged }: { skill: SkillView; onChanged: () => void }) {
  const state = useApiData<SkillDetail>(`/admin/skills/${skill.id}`);
  const [writing, setWriting] = useState(false);
  const action = useAction();

  function changed() {
    state.reload();
    onChanged();
  }

  async function activate(version: number) {
    const path = `/admin/skills/${skill.id}/activation`;
    const activated = await action.run(async () => {
      try {
        return await api.put(path, { version } satisfies SkillActivation);
      } catch (e: unknown) {
        // Its last evaluation found regressions: made active once confirmed.
        if (!(e instanceof ApiError) || e.status !== 409) throw e;
        if (!window.confirm(`${e.message}\n\nActiver quand même la version ${version} ?`)) {
          return null;
        }
        return api.put(path, { version, confirmed: true } satisfies SkillActivation);
      }
    });
    if (activated) changed();
  }

  async function reject(version: number) {
    const body = { version } satisfies SkillRejection;
    if (await action.run(() => api.post(`/admin/skills/${skill.id}/rejection`, body))) changed();
  }

  return (
    <div className="mt-3 border-t border-line pt-3">
      <Loaded state={state}>
        {(detail) => (
          <div className="flex flex-col gap-3">
            {writing ? (
              <VersionForm
                skillId={skill.id}
                // From the latest version an administrator wrote, empty when
                // none did: a new one is most often a change of it, and a
                // proposal of the agent's is read as such before anyone
                // builds on it (ADR-100, ADR-101).
                from={detail.versions.find((v) => !v.proposed)}
                onDone={(saved) => {
                  setWriting(false);
                  if (saved) changed();
                }}
              />
            ) : (
              <div>
                <Button variant="secondary" onClick={() => setWriting(true)}>
                  <Plus className="size-4" aria-hidden /> Nouvelle version
                </Button>
              </div>
            )}
            {action.error && <ErrorNote message={action.error} />}
            <ol className="flex flex-col gap-2">
              {detail.versions.map((version) => (
                <VersionItem
                  key={version.version}
                  version={version}
                  activeVersion={detail.skill.activeVersion}
                  pending={action.pending}
                  onActivate={() => activate(version.version)}
                  onReject={() => reject(version.version)}
                />
              ))}
            </ol>
            <EvalCases skillId={skill.id} cases={detail.cases} onChanged={state.reload} />
            <EvaluationsSection
              skillId={skill.id}
              versions={detail.versions}
              activeVersion={detail.skill.activeVersion}
              cases={detail.cases.length}
            />
          </div>
        )}
      </Loaded>
    </div>
  );
}
