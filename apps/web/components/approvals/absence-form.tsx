'use client';

import { useState, type FormEvent } from 'react';
import { Button, ErrorNote, Field, Input, Select } from '@/components/ui';
import { useAction, type Absence, type ApprovalPerson } from '@/lib/api';

/** The day `days` after `day` (today when absent), as a date field writes it, in the reader's time zone. */
function dayAfter(days: number, day?: string): string {
  const start = day ? new Date(`${day}T12:00:00`) : new Date();
  const then = new Date(start.getTime() + days * 86_400_000);
  return new Date(then.getTime() - then.getTimezoneOffset() * 60_000).toISOString().slice(0, 10);
}

function today(): string {
  return dayAfter(0);
}

/**
 * Declares an absence (ADR-071): the colleague who stands in, from which
 * day (today: from now), until the end of which day; at most 90 days, as
 * the API checks, to the hour a clock change adds. One's own, or a
 * collaborator's by their account administrator (ADR-078): `declare` sends
 * it, `onDeclared` receives the answer.
 */
export function AbsenceForm<T>({
  candidates,
  submitLabel,
  emptyNote,
  declare,
  onDeclared,
}: {
  candidates: ApprovalPerson[];
  submitLabel: string;
  /** Said when nobody may stand in. */
  emptyNote: string;
  declare: (body: Absence) => Promise<T>;
  onDeclared: (answer: T) => void;
}) {
  const [delegateId, setDelegateId] = useState(candidates[0]?.id ?? '');
  const [from, setFrom] = useState(today());
  const [until, setUntil] = useState('');
  const action = useAction();

  async function submit(event: FormEvent) {
    event.preventDefault();
    const declared = await action.run(() =>
      declare({
        delegateId,
        startsAt: from > today() ? new Date(`${from}T00:00:00`).toISOString() : undefined,
        endsAt: new Date(`${until}T23:59:59`).toISOString(),
      } satisfies Absence),
    );
    if (declared !== null) onDeclared(declared);
  }

  if (candidates.length === 0) {
    return <p className="text-sm text-muted">{emptyNote}</p>;
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
          <Input
            type="date"
            required
            min={today()}
            value={from}
            onChange={(e) => setFrom(e.target.value)}
          />
        </Field>
        {/* Ninety days at most, the first included. */}
        <Field label="Au (inclus)">
          <Input
            type="date"
            required
            min={from}
            max={dayAfter(89, from)}
            value={until}
            onChange={(e) => setUntil(e.target.value)}
          />
        </Field>
      </div>
      {action.error && <ErrorNote message={action.error} />}
      <Button type="submit" className="w-fit" loading={action.pending}>
        {submitLabel}
      </Button>
    </form>
  );
}
