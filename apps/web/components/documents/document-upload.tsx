'use client';

import { useState } from 'react';
import { Button, Card, ErrorNote, Field, Input, Select } from '@/components/ui';
import {
  api,
  ApiError,
  useAction,
  type DocumentCreated,
  type DocumentQuota,
  type DocumentSpaceOption,
  type DocumentUpload as Upload,
  type Sensitivity,
} from '@/lib/api';
import { formatBytes } from '@/lib/format';
import { CLEARANCE_LABELS } from '@/lib/labels';

/** The largest file the API takes (ADR-095): beyond, it is not even sent. */
const MAX_FILE_BYTES = 10 * 1024 * 1024;
const TOO_LARGE = 'Ce fichier dépasse 10 Mo : il n’est pas envoyé.';
/** The files the API reads. */
const ACCEPTED = '.pdf,.docx,.md,.markdown,.txt';
const SENSITIVITIES: Sensitivity[] = ['public', 'internal', 'confidential', 'restricted'];

/** The deposit's query string: its name, its space, its sensitivity. */
function query(upload: Upload): string {
  const params = new URLSearchParams({ name: upload.name });
  if (upload.unitId) params.set('unitId', upload.unitId);
  if (upload.sensitivity) params.set('sensitivity', upload.sensitivity);
  return params.toString();
}

/**
 * Deposits a file into a space the reader may write (ADR-095): the
 * company's, or a unit's, up to a sensitivity. The file goes as it is; the
 * API reads it, and says why when it cannot.
 */
export function DocumentUpload({
  spaces,
  quota,
  onDeposited,
}: {
  spaces: DocumentSpaceOption[];
  quota: DocumentQuota | null;
  onDeposited: () => void;
}) {
  const action = useAction();
  const [file, setFile] = useState<File | null>(null);
  // Cleared once deposited: a new input, empty.
  const [picker, setPicker] = useState(0);
  const [space, setSpace] = useState(spaces[0]?.unitId ?? '');
  const [sensitivity, setSensitivity] = useState<Sensitivity>('internal');
  const [deposited, setDeposited] = useState<string | null>(null);

  async function deposit() {
    if (!file) return;
    setDeposited(null);
    const upload = {
      name: file.name,
      unitId: space === '' ? undefined : space,
      sensitivity,
    } satisfies Upload;
    const created = await action.run(() => {
      if (file.size > MAX_FILE_BYTES) throw new ApiError(413, 'too_large', TOO_LARGE);
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
          PDF avec du texte, Word (.docx), Markdown ou texte, 10 Mo au plus. Le document est lu par
          ceux qui ont accès à son espace, jusqu’à sa sensibilité.
        </p>
        <div className="grid gap-3 sm:grid-cols-3">
          <Field label="Fichier" className="sm:col-span-3">
            <Input
              key={picker}
              type="file"
              accept={ACCEPTED}
              disabled={action.pending}
              onChange={(e) => setFile(e.target.files?.[0] ?? null)}
            />
          </Field>
          <Field label="Espace">
            <Select
              value={space}
              disabled={action.pending}
              onChange={(e) => setSpace(e.target.value)}
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
