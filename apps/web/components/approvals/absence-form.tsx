'use client';

import { useState, type FormEvent } from 'react';
import { Button, ErrorNote, Field, Input, Select } from '@/components/ui';
import { api, useAction, type Absence, type ApprovalPerson } from '@/lib/api';

/** Today, as a date field writes it, in the reader's time zone. */
function today(): string {
  const now = new Date();
  return new Date(now.getTime() - now.getTimezoneOffset() * 60_000).toISOString().slice(0, 10);
}

/**
 * Declares one's absence (ADR-071): the colleague who stands in, from which
 * day (today: from now), until the end of which day; at most 90 days, as
 * the API checks.
 */
export function AbsenceForm({
  candidates,
  onDeclared,
}: {
  candidates: ApprovalPerson[];
  onDeclared: () => void;
}) {
  const [delegateId, setDelegateId] = useState(candidates[0]?.id ?? '');
  const [from, setFrom] = useState(today());
  const [until, setUntil] = useState('');
  const action = useAction();

  async function submit(event: FormEvent) {
    event.preventDefault();
    const declared = await action.run(() =>
      api.put('/approval-delegation', {
        delegateId,
        startsAt: from > today() ? new Date(`${from}T00:00:00`).toISOString() : undefined,
        endsAt: new Date(`${until}T23:59:59`).toISOString(),
      } satisfies Absence),
    );
    if (declared) onDeclared();
  }

  if (candidates.length === 0) {
    return (
      <p className="text-sm text-muted">
        Aucun collègue ne détient le droit de valider des actions : personne ne peut vous remplacer.
      </p>
    );
  }
  return (
    <form onSubmit={submit} className="flex flex-col gap-3">
      <div className="grid gap-3 sm:grid-cols-3">
        <Field label="Remplaçant">
          <Select value={delegateId} onChange={(e) => setDelegateId(e.target.value)}>
            {candidates.map((c) => (
              <option key={c.id} value={c.id}>
                {c.displayName}
              </option>
            ))}
          </Select>
        </Field>
        <Field label="Du">
          <Input type="date" required value={from} onChange={(e) => setFrom(e.target.value)} />
        </Field>
        <Field label="Au (inclus)">
          <Input type="date" required value={until} onChange={(e) => setUntil(e.target.value)} />
        </Field>
      </div>
      {action.error && <ErrorNote message={action.error} />}
      <Button type="submit" className="w-fit" loading={action.pending}>
        Déclarer mon absence
      </Button>
    </form>
  );
}
