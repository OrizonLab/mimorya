'use client';

import { Plus } from 'lucide-react';
import { useState } from 'react';
import { Button, ErrorNote, Loaded } from '@/components/ui';
import {
  api,
  useAction,
  useApiData,
  type SkillActivation,
  type SkillDetail,
  type SkillRejection,
  type SkillView,
} from '@/lib/api';
import { VersionForm } from './version-form';
import { VersionItem } from './version-item';

/**
 * The versions of a skill, the latest first: one is made active here, and
 * a proposal of the agent's turned down (ADR-100, ADR-101).
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
    const body = { version } satisfies SkillActivation;
    if (await action.run(() => api.put(`/admin/skills/${skill.id}/activation`, body))) changed();
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
                initial={detail.versions.find((v) => !v.proposed)?.content ?? ''}
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
          </div>
        )}
      </Loaded>
    </div>
  );
}
