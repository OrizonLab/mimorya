'use client';

import { useState, type FormEvent } from 'react';
import { Button, ErrorNote, Field, Input } from '@/components/ui';

/**
 * A code of one's second factor (ADR-200): the six digits of the app or one
 * of the recovery codes, which the server tells apart. `label` names the
 * form for assistive technologies, `submit` its button.
 */
export function CodeForm({
  label,
  submit,
  pending,
  error,
  onCode,
}: {
  label: string;
  submit: string;
  pending: boolean;
  error: string | null;
  onCode: (code: string) => Promise<boolean>;
}) {
  const [code, setCode] = useState('');

  async function send(event: FormEvent) {
    event.preventDefault();
    if (await onCode(code.trim())) setCode('');
  }

  return (
    <form onSubmit={send} aria-label={label} className="flex flex-col gap-3">
      <Field label="Code de l’application ou code de secours">
        <Input
          required
          autoComplete="one-time-code"
          maxLength={64}
          value={code}
          onChange={(e) => setCode(e.target.value)}
          className="w-56 font-mono"
        />
      </Field>
      {error && <ErrorNote message={error} />}
      <Button type="submit" className="w-fit" loading={pending}>
        {submit}
      </Button>
    </form>
  );
}
