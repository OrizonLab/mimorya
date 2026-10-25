import { Badge, LabelBadge } from '@/components/ui';
import type { CaseResultView } from '@/lib/api';
import { CASE_OUTCOME } from '@/lib/labels';

/**
 * How a case went for one version (ADR-104): its outcome, whether the
 * version was loaded, why no answer was judged; the answer, the judge's
 * issues and the tools called when shown, to whom the evaluation ran for
 * alone.
 */
export function CaseResult({ label, result }: { label: string; result: CaseResultView | null }) {
  if (!result) {
    return (
      <div className="text-xs text-muted">
        <span className="font-medium">{label}</span> : pas encore joué
      </div>
    );
  }
  return (
    <div className="flex flex-col gap-1 text-xs">
      <div className="flex flex-wrap items-center gap-2">
        <span className="font-medium">{label}</span>
        <LabelBadge labels={CASE_OUTCOME} value={result.outcome} />
        <Badge>{result.skillLoaded ? 'compétence chargée' : 'compétence non chargée'}</Badge>
      </div>
      {result.error && <p className="text-warn">{result.error}</p>}
      {result.issues && result.issues.length > 0 && (
        <ul className="list-disc pl-5 text-danger">
          {result.issues.map((issue, index) => (
            <li key={`${index} ${issue}`}>{issue}</li>
          ))}
        </ul>
      )}
      {result.answer && (
        <p className="whitespace-pre-wrap break-words rounded bg-surface p-2">{result.answer}</p>
      )}
      {result.tools && result.tools.length > 0 && (
        <ul className="text-muted">
          {result.tools.map((tool, index) => (
            <li key={`${index} ${tool.name}`}>
              <span className="font-mono">{tool.name}</span>
              {tool.isError ? ' (en erreur)' : ''} : {tool.summary}
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
