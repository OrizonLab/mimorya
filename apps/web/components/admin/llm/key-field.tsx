'use client';

import { Field, Input } from '@/components/ui';

/** What is stored of a company's provider, as far as its key goes. */
type StoredChoice = { keyStored: boolean; provider: string; baseUrl: string | null };

/** Whether the stored key will serve `provider` at `baseUrl` (empty for a
 * hosted one): a key only ever serves the same provider at the same address. */
export function keepsStoredKey(
  current: StoredChoice | null,
  provider: string,
  baseUrl: string,
): boolean {
  return (
    current?.keyStored === true &&
    current.provider === provider &&
    (current.baseUrl ?? '') === baseUrl
  );
}

/** The write-only API key of a provider, `of` naming whose (« d’embeddings »). */
export function KeyField({
  of,
  required,
  keeps,
  value,
  onChange,
}: {
  of?: string;
  required: boolean;
  keeps: boolean;
  value: string;
  onChange: (value: string) => void;
}) {
  const name = of ? `Clé d’API ${of}` : 'Clé d’API';
  const label = keeps
    ? `${name} (laisser vide pour conserver la clé enregistrée)`
    : required
      ? name
      : `${name} (facultative)`;
  return (
    <Field label={label}>
      <Input
        type="password"
        autoComplete="off"
        required={required && !keeps}
        maxLength={500}
        value={value}
        onChange={(e) => onChange(e.target.value)}
      />
    </Field>
  );
}
