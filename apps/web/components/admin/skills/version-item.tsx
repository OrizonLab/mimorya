import { Button, LabelBadge } from '@/components/ui';
import type { SkillVersionView } from '@/lib/api';
import { formatDate } from '@/lib/format';
import { SKILL_VERSION_STATUS } from '@/lib/labels';

/**
 * One version of a skill: who wrote it (or for whom the agent proposed it,
 * ADR-100), its note and its content, unfolded while it awaits a decision;
 * made active here, or, a proposal awaiting a decision, turned down
 * (ADR-101).
 */
export function VersionItem({
  version,
  activeVersion,
  pending,
  onActivate,
  onReject,
}: {
  version: SkillVersionView;
  activeVersion: number | null;
  pending: boolean;
  onActivate: () => void;
  onReject: () => void;
}) {
  // An older version made active again is the way back.
  const back = activeVersion !== null && version.version < activeVersion;
  const author = version.createdBy ?? 'un compte supprimé';
  return (
    <li className="rounded-lg bg-surface-2 p-3 text-sm">
      <div className="flex flex-wrap items-center gap-2">
        <span className="font-medium">Version {version.version}</span>
        <LabelBadge labels={SKILL_VERSION_STATUS} value={version.status} />
        <span className="text-xs text-muted">
          {version.proposed ? `proposée par l’agent pour ${author}` : `écrite par ${author}`} le{' '}
          {formatDate(version.createdAt)}
          {version.approvedAt &&
            ` · activée par ${version.approvedBy ?? 'un compte supprimé'} le ${formatDate(version.approvedAt)}`}
        </span>
        <span className="ml-auto flex flex-wrap gap-1">
          {version.status === 'candidate' && (
            <Button variant="ghost" className="px-2 py-1" loading={pending} onClick={onReject}>
              Rejeter la version {version.version}
            </Button>
          )}
          {version.status !== 'active' && (
            <Button
              variant="secondary"
              className="px-2 py-1"
              loading={pending}
              onClick={onActivate}
            >
              {back ? 'Revenir à' : 'Activer'} la version {version.version}
            </Button>
          )}
        </span>
      </div>
      {version.changelog && <p className="mt-1 text-xs text-muted">{version.changelog}</p>}
      {version.proposedDescription && (
        <p className="mt-1 text-xs">Description proposée : {version.proposedDescription}</p>
      )}
      <details className="mt-2" open={version.status === 'candidate'}>
        <summary className="cursor-pointer text-xs text-muted">Contenu</summary>
        <pre className="mt-2 whitespace-pre-wrap break-words font-mono text-xs">
          {version.content}
        </pre>
      </details>
    </li>
  );
}
