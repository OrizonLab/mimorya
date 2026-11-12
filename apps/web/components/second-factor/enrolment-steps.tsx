'use client';

import { useState, type FormEvent } from 'react';
import { Button, ErrorNote, Field, Input } from '@/components/ui';
import type { SecondFactorEnrolment } from '@/lib/api';

/** The secret as typed into an app by hand: groups of four characters. */
function grouped(secret: string): string {
  return secret.match(/.{1,4}/g)?.join(' ') ?? secret;
}

/**
 * What the authenticator app reads (ADR-200): its QR code, drawn by the
 * server (ADR-203), or the key to type by hand; then the first code it
 * shows, which turns the second factor on.
 */
export function EnrolmentSteps({
  enrolment,
  pending,
  error,
  onCode,
}: {
  enrolment: SecondFactorEnrolment;
  pending: boolean;
  error: string | null;
  onCode: (code: string) => void;
}) {
  const [code, setCode] = useState('');

  function submit(event: FormEvent) {
    event.preventDefault();
    onCode(code.trim());
  }

  return (
    <form
      onSubmit={submit}
      aria-label="Activation du second facteur"
      className="flex flex-col gap-3"
    >
      <p className="text-sm">
        1. Dans votre application d’authentification, ajoutez un compte en scannant ce QR code.
      </p>
      <img
        src={enrolment.qrCode}
        alt="QR code à scanner avec votre application d’authentification"
        width={180}
        height={180}
        className="rounded-md border border-line"
      />
      <p className="text-sm text-muted">
        Sans appareil photo, saisissez cette clé dans l’application :{' '}
        <code aria-label="Clé du second facteur" className="font-mono text-fg">
          {grouped(enrolment.secret)}
        </code>
      </p>
      <p className="text-sm">2. Saisissez le code à six chiffres qu’elle affiche.</p>
      <Field label="Code à six chiffres">
        <Input
          required
          inputMode="numeric"
          autoComplete="one-time-code"
          pattern="[0-9]{6}"
          maxLength={6}
          value={code}
          onChange={(e) => setCode(e.target.value)}
          className="w-32 font-mono"
        />
      </Field>
      {error && <ErrorNote message={error} />}
      <Button type="submit" className="w-fit" loading={pending}>
        Activer
      </Button>
    </form>
  );
}
