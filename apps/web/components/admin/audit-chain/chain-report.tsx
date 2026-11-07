import { Badge } from '@/components/ui';
import type { AuditAnchorFound, AuditChainBreak, AuditChainVerification } from '@/lib/api';
import { formatDate } from '@/lib/format';

const FOUND: Record<AuditAnchorFound, string> = {
  held: 'tenue',
  differs: 'sceau différent : la chaîne a été scellée à nouveau depuis cette place',
  missing: 'absente : la chaîne s’arrête avant cette place, ou elle y manque',
};

function told(at: AuditChainBreak): string {
  switch (at.kind) {
    case 'missing':
      return at.from === at.to
        ? `Place ${at.from} : entrée supprimée.`
        : `Places ${at.from} à ${at.to} : entrées supprimées.`;
    case 'altered':
      return `Place ${at.place} : l’entrée n° ${at.entryId} n’est plus celle qui a été scellée.`;
  }
}

/** What the verification of the chain found (ADR-179). */
export function ChainReport({ report }: { report: AuditChainVerification }) {
  return (
    <div
      role="status"
      aria-label="Résultat de la vérification"
      className="flex flex-col gap-2 text-sm"
    >
      <div>
        <Badge tone={report.holds ? 'ok' : 'danger'}>
          {report.holds
            ? 'Journal intact'
            : report.broken > 0
              ? 'Journal rompu'
              : 'Ancre non tenue'}
        </Badge>
      </div>
      <p>
        {report.verified} entrée(s) vérifiée(s) · {report.unsealed} en attente de scellement,
        normalement scellée(s) dans la minute : un nombre qui ne baisse pas est à signaler à
        l’exploitant de la plateforme.
      </p>
      {report.last && report.holds && (
        <p className="flex flex-col gap-1">
          <span>Dernière place scellée : n° {report.last.place}. Ancre à conserver :</span>
          <code aria-label="Ancre à conserver" className="font-mono text-xs break-all select-all">
            {report.last.anchor}
          </code>
        </p>
      )}
      {report.last && !report.holds && (
        <p className="text-danger">
          Dernière place lue : n° {report.last.place}. La chaîne n’étant pas tenue, gardez les
          ancres que vous aviez : n’en prenez pas une nouvelle avant que l’exploitant n’ait examiné
          le journal.
        </p>
      )}
      {!report.last && <p>Aucune entrée n’est encore scellée.</p>}
      {report.broken > 0 && (
        <div>
          <p className="text-danger">{report.broken} rupture(s) :</p>
          <ul className="list-disc pl-5">
            {report.breaks.map((at) => (
              <li key={at.kind === 'missing' ? `m${at.from}` : `a${at.entryId}`}>{told(at)}</li>
            ))}
          </ul>
        </div>
      )}
      {report.anchors.length > 0 && (
        <ul className="list-disc pl-5">
          {report.anchors.map((anchor, index) => (
            <li key={index} className={anchor.found === 'held' ? '' : 'text-danger'}>
              Ancre de la place {anchor.place} : {FOUND[anchor.found]}.
            </li>
          ))}
        </ul>
      )}
      <p className="text-xs text-muted">Vérifié le {formatDate(report.verifiedAt)}.</p>
    </div>
  );
}
