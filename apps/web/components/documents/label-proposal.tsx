'use client';

import { Button } from '@/components/ui';
import type { DocumentItem, DocumentLabelsUpdate } from '@/lib/api';
import { CLEARANCE_LABELS, DOCUMENT_TYPES } from '@/lib/labels';

const SMALL = 'px-2.5 py-1 text-xs';

/**
 * What a document's classification by the decision service left (ADR-213,
 * D4), shown to whoever may write it: a type and a looser level proposed,
 * applied as proposed; a level the decision raised, kept or set back to
 * the one it held; or the whole dismissed, the document left as it is.
 * Each sends the version classified; none starts while the list sends.
 */
export function LabelProposal({
  document,
  busy,
  onSet,
  onDismiss,
}: {
  document: DocumentItem;
  busy: boolean;
  onSet: (labels: DocumentLabelsUpdate) => void;
  onDismiss: (version: number) => void;
}) {
  const proposal = document.labelProposal;
  if (!proposal) return null;
  const { version, documentType, sensitivity, raisedFrom } = proposal;
  const proposed = [
    documentType && `type « ${DOCUMENT_TYPES[documentType]} »`,
    sensitivity && `confidentialité « ${CLEARANCE_LABELS[sensitivity]} »`,
  ].filter(Boolean);
  return (
    <div
      role="group"
      aria-label={`Classement de « ${document.title} »`}
      className="mt-2 rounded-md bg-surface-2 p-2 text-xs"
    >
      {raisedFrom && (
        <p>
          Confidentialité relevée par le service de décision : elle était «{' '}
          {CLEARANCE_LABELS[raisedFrom]} ».
        </p>
      )}
      {proposed.length > 0 && <p>Proposé par le service de décision : {proposed.join(', ')}.</p>}
      <div className="mt-2 flex flex-wrap gap-2">
        {proposed.length > 0 && (
          <Button
            type="button"
            className={SMALL}
            disabled={busy}
            onClick={() =>
              onSet({
                version,
                documentType: documentType ?? document.documentType,
                sensitivity: sensitivity ?? document.sensitivity,
              } satisfies DocumentLabelsUpdate)
            }
          >
            Appliquer
          </Button>
        )}
        {raisedFrom && (
          <Button
            type="button"
            variant="secondary"
            className={SMALL}
            disabled={busy}
            onClick={() =>
              onSet({
                version,
                documentType: document.documentType,
                sensitivity: raisedFrom,
              } satisfies DocumentLabelsUpdate)
            }
          >
            Rétablir « {CLEARANCE_LABELS[raisedFrom]} »
          </Button>
        )}
        <Button
          type="button"
          variant="secondary"
          className={SMALL}
          disabled={busy}
          onClick={() => onDismiss(version)}
        >
          Ignorer
        </Button>
      </div>
    </div>
  );
}
