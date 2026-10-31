'use client';

import { useState } from 'react';
import { Button, Card, ErrorNote, Field, Select } from '@/components/ui';
import {
  api,
  useAction,
  type DocumentCreated,
  type DocumentQuota,
  type DocumentSpaceOption,
  type DocumentUpload as Upload,
  type Sensitivity,
} from '@/lib/api';
import { formatBytes } from '@/lib/format';
import { CLEARANCE_LABELS, SENSITIVITIES } from '@/lib/labels';
import { FileDrop } from './file-drop';
import { refuseBeyond } from './file-rules';

/** The deposit's query string: its name, its space, its sensitivity. */
function query(upload: Upload): string {
  const params = new URLSearchParams({ name: upload.name });
  if (upload.unitId) params.set('unitId', upload.unitId);
  if (upload.sensitivity) params.set('sensitivity', upload.sensitivity);
  return params.toString();
}

/**
 * Deposits a file into a space the reader may write (ADR-095): the
 * company's, or a unit's, up to a sensitivity. The file, chosen or dropped
 * (ADR-151), goes as it is; the API reads it, and says why when it cannot.
 * A file beyond the largest the API takes, which it gives, is not even sent.
 */
export function DocumentUpload({
  spaces,
  quota,
  maxFileBytes,
  onDeposited,
}: {
  spaces: DocumentSpaceOption[];
  quota: DocumentQuota | null;
  maxFileBytes: number;
  onDeposited: () => void;
}) {
  const action = useAction();
  const [file, setFile] = useState<File | null>(null);
  // Cleared once deposited: a new input, empty.
  const [picker, setPicker] = useState(0);
  const [chosen, setChosen] = useState(spaces[0]?.unitId ?? '');
  const [sensitivity, setSensitivity] = useState<Sensitivity>('internal');
  const [deposited, setDeposited] = useState<string | null>(null);
  // The spaces come again with each reload: one no longer offered gives way
  // to the first.
  const space = spaces.some((option) => (option.unitId ?? '') === chosen)
    ? chosen
    : (spaces[0]?.unitId ?? '');
  const largest = formatBytes(maxFileBytes);

  async function deposit() {
    if (!file) return;
    setDeposited(null);
    const upload = {
      name: file.name,
      unitId: space === '' ? undefined : space,
      sensitivity,
    } satisfies Upload;
    const created = await action.run(() => {
      refuseBeyond(file, maxFileBytes);
      return api.upload<DocumentCreated>(`/documents?${query(upload)}`, file);
    });
    if (created === null) return;
    setDeposited(`« ${file.name} » est déposé : ${created.passages} passage(s) pour l’agent.`);
    setFile(null);
    setPicker((n) => n + 1);
    onDeposited();
  }

  return (
    <section aria-labelledby="new-document" className="mb-6">
      <Card className="flex flex-col gap-3 p-4">
        <h2 id="new-document" className="font-medium">
          Déposer un document
        </h2>
        <p className="text-xs text-muted">
          PDF avec du texte, Word (.docx), Markdown ou texte, {largest} au plus. Le document est lu
          par ceux qui ont accès à son espace, jusqu’à sa sensibilité.
        </p>
        <div className="grid gap-3 sm:grid-cols-3">
          <div className="sm:col-span-3">
            <FileDrop file={file} onFile={setFile} disabled={action.pending} reset={picker} />
          </div>
          <Field label="Espace">
            <Select
              value={space}
              disabled={action.pending}
              onChange={(e) => setChosen(e.target.value)}
            >
              {spaces.map((option) => (
                <option key={option.unitId ?? ''} value={option.unitId ?? ''}>
                  {option.name}
                </option>
              ))}
            </Select>
          </Field>
          <Field label="Sensibilité">
            <Select
              value={sensitivity}
              disabled={action.pending}
              onChange={(e) => setSensitivity(e.target.value as Sensitivity)}
            >
              {SENSITIVITIES.map((level) => (
                <option key={level} value={level}>
                  {CLEARANCE_LABELS[level]}
                </option>
              ))}
            </Select>
          </Field>
          <div className="flex items-end">
            <Button onClick={deposit} loading={action.pending} disabled={!file}>
              Déposer
            </Button>
          </div>
        </div>
        {action.error && <ErrorNote message={action.error} />}
        {deposited && (
          <p role="status" className="text-sm text-ok">
            {deposited}
          </p>
        )}
        {quota && (
          <p className="text-xs text-muted">
            Documents de l’entreprise : {formatBytes(quota.usedBytes)} sur{' '}
            {formatBytes(quota.quotaBytes)}.
          </p>
        )}
      </Card>
    </section>
  );
}
