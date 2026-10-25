'use client';

import { Plus } from 'lucide-react';
import { useState } from 'react';
import { Button, ErrorNote, LabelBadge, Loaded } from '@/components/ui';
import {
  api,
  useAction,
  useApiData,
  type SkillActivation,
  type SkillDetail,
  type SkillVersionView,
  type SkillView,
} from '@/lib/api';
import { formatDate } from '@/lib/format';
import { SKILL_VERSION_STATUS } from '@/lib/labels';
import { VersionForm } from './version-form';

/** The versions of a skill, the latest first: one is made active here. */
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

  return (
    <div className="mt-3 border-t border-line pt-3">
      <Loaded state={state}>
        {(detail) => (
          <div className="flex flex-col gap-3">
            {writing ? (
              <VersionForm
                skillId={skill.id}
                // From the latest version an administrator wrote: a new one is
                // most often a change of it, and a proposal of the agent's is
                // read as such before anyone builds on it (ADR-100).
                initial={
                  (detail.versions.find((v) => !v.proposed) ?? detail.versions[0])?.content ?? ''
                }
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
                />
              ))}
            </ol>
          </div>
        )}
      </Loaded>
    </div>
  );
}

function VersionItem({
  version,
  activeVersion,
  pending,
  onActivate,
}: {
  version: SkillVersionView;
  activeVersion: number | null;
  pending: boolean;
  onActivate: () => void;
}) {
  // An older version made active again is the way back.
  const back = activeVersion !== null && version.version < activeVersion;
  return (
    <li className="rounded-lg bg-surface-2 p-3 text-sm">
      <div className="flex flex-wrap items-center gap-2">
        <span className="font-medium">Version {version.version}</span>
        <LabelBadge labels={SKILL_VERSION_STATUS} value={version.status} />
        <span className="text-xs text-muted">
          écrite par {version.createdBy ?? 'un compte supprimé'} le {formatDate(version.createdAt)}
          {version.approvedAt &&
            ` · activée par ${version.approvedBy ?? 'un compte supprimé'} le ${formatDate(version.approvedAt)}`}
        </span>
        {version.status !== 'active' && (
          <Button
            variant="secondary"
            className="ml-auto px-2 py-1"
            loading={pending}
            onClick={onActivate}
          >
            {back ? 'Revenir à' : 'Activer'} la version {version.version}
          </Button>
        )}
      </div>
      {version.changelog && <p className="mt-1 text-xs text-muted">{version.changelog}</p>}
      <details className="mt-2">
        <summary className="cursor-pointer text-xs text-muted">Contenu</summary>
        <pre className="mt-2 whitespace-pre-wrap break-words font-mono text-xs">
          {version.content}
        </pre>
      </details>
    </li>
  );
}
