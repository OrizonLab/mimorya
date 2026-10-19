'use client';

import { ErrorNote, Field, Input } from '@/components/ui';

/** Shortest password the API accepts (`MIN_PASSWORD_CHARS`). */
export const MIN_PASSWORD_LENGTH = 12;

/** A password being chosen, typed twice. */
export interface NewPassword {
  password: string;
  confirmation: string;
}

export const EMPTY_NEW_PASSWORD: NewPassword = { password: '', confirmation: '' };

/** Whether the confirmation, once typed, differs from the password. */
export function passwordsDiffer({ password, confirmation }: NewPassword): boolean {
  return confirmation.length > 0 && confirmation !== password;
}

/** The new password and its confirmation, with the minimum length and a note when they differ. */
export function NewPasswordFields({
  value,
  onChange,
  label = 'Mot de passe',
}: {
  value: NewPassword;
  onChange: (value: NewPassword) => void;
  label?: string;
}) {
  return (
    <>
      <Field label={`${label} (${MIN_PASSWORD_LENGTH} caractères minimum)`}>
        <Input
          type="password"
          required
          minLength={MIN_PASSWORD_LENGTH}
          autoComplete="new-password"
          value={value.password}
          onChange={(e) => onChange({ ...value, password: e.target.value })}
        />
      </Field>
      <Field label="Confirmation">
        <Input
          type="password"
          required
          autoComplete="new-password"
          value={value.confirmation}
          onChange={(e) => onChange({ ...value, confirmation: e.target.value })}
        />
      </Field>
      {passwordsDiffer(value) && <ErrorNote message="Les deux saisies diffèrent." />}
    </>
  );
}
