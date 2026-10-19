'use client';

import { useState, type FormEvent } from 'react';
import { Button, Card, ErrorNote, Field, Input } from '@/components/ui';
import { api, useAction, type ResetLinkRequest } from '@/lib/api';

/**
 * Asks for a reset link. The API answers the same whether the address
 * belongs to an account or not: the confirmation says so.
 */
export function ResetRequestForm() {
  const [email, setEmail] = useState('');
  const [asked, setAsked] = useState<string | null>(null);
  const action = useAction();

  async function submit(event: FormEvent) {
    event.preventDefault();
    const sent = await action.run(() =>
      api.post('/auth/password-reset', { email } satisfies ResetLinkRequest),
    );
    if (sent) setAsked(email.trim());
  }

  if (asked) {
    return (
      <Card className="flex flex-col gap-3 p-6 text-sm">
        <p className="font-medium">Demande enregistrée</p>
        <p className="text-muted">
          Si un compte actif correspond à {asked}, un lien pour choisir un nouveau mot de passe
          vient de lui être envoyé. Il est valable une heure et ne sert qu’une fois ; pensez à
          regarder dans les courriers indésirables.
        </p>
        <p className="text-muted">
          Votre compte n’est pas encore activé ? Utilisez votre lien d’invitation, ou demandez-en un
          nouveau à votre administrateur.
        </p>
      </Card>
    );
  }

  return (
    <Card className="p-6">
      <form onSubmit={submit} className="flex flex-col gap-4">
        <div className="text-sm">
          <p className="font-medium">Mot de passe oublié</p>
          <p className="text-muted">
            Indiquez l’adresse de votre compte : vous recevrez un lien pour choisir un nouveau mot
            de passe.
          </p>
        </div>
        <Field label="Adresse e-mail professionnelle">
          <Input
            type="email"
            required
            autoComplete="username"
            value={email}
            onChange={(e) => setEmail(e.target.value)}
          />
        </Field>
        {action.error && <ErrorNote message={action.error} />}
        <Button type="submit" loading={action.pending}>
          Recevoir un lien
        </Button>
      </form>
    </Card>
  );
}
