'use client';

import Link from 'next/link';
import { useRouter } from 'next/navigation';
import { useEffect, useState, type FormEvent } from 'react';
import { AuthFrame } from '@/components/account';
import { Button, Card, ErrorNote } from '@/components/ui';
import { api, ApiError, type AuthOptions } from '@/lib/api';

// Fictional demo accounts (seeded by `pnpm db:seed`), shown only in demo mode.
const DEMO_ACCOUNTS = [
  { email: 'claire.martin@neo-maintenance.example', label: 'Claire Martin — Direction' },
  { email: 'marc.lefevre@neo-maintenance.example', label: 'Marc Lefèvre — Responsable technique' },
  { email: 'julien.moreau@neo-maintenance.example', label: 'Julien Moreau — Chef d’équipe Nord' },
  { email: 'lucas.petit@neo-maintenance.example', label: 'Lucas Petit — Technicien' },
  { email: 'isabelle.laurent@neo-maintenance.example', label: 'Isabelle Laurent — Administratif' },
  { email: 'paul.renaud@neo-maintenance.example', label: 'Paul Renaud — Administrateur technique' },
];

export default function LoginPage() {
  const router = useRouter();
  const [email, setEmail] = useState('');
  const [password, setPassword] = useState('');
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  // A reset link can be asked for only when the server sends e-mails.
  const [passwordReset, setPasswordReset] = useState(false);
  const demo = process.env.NEXT_PUBLIC_DEMO_MODE === 'true';

  useEffect(() => {
    api
      .get<AuthOptions>('/auth/options')
      .then((options) => setPasswordReset(options.passwordReset))
      .catch(() => setPasswordReset(false));
  }, []);

  async function submit(event: FormEvent) {
    event.preventDefault();
    setLoading(true);
    setError(null);
    try {
      await api.post('/auth/login', { email, password });
      router.replace('/chat');
    } catch (e) {
      setError(e instanceof ApiError ? e.message : 'Connexion impossible');
    } finally {
      setLoading(false);
    }
  }

  return (
    <AuthFrame>
      <Card className="p-6">
        <form onSubmit={submit} className="flex flex-col gap-4">
          <label className="flex flex-col gap-1.5 text-sm">
            <span className="font-medium">Adresse e-mail professionnelle</span>
            <input
              type="email"
              required
              autoComplete="username"
              value={email}
              onChange={(e) => setEmail(e.target.value)}
              className="rounded-lg border border-line bg-surface px-3 py-2 outline-none focus:border-accent"
            />
          </label>
          <label className="flex flex-col gap-1.5 text-sm">
            <span className="font-medium">Mot de passe</span>
            <input
              type="password"
              required
              autoComplete="current-password"
              value={password}
              onChange={(e) => setPassword(e.target.value)}
              className="rounded-lg border border-line bg-surface px-3 py-2 outline-none focus:border-accent"
            />
          </label>
          {error && <ErrorNote message={error} />}
          <Button type="submit" loading={loading}>
            Se connecter
          </Button>
          {passwordReset && (
            <Link href="/forgot-password" className="text-center text-sm text-muted hover:text-fg">
              Mot de passe oublié ?
            </Link>
          )}
        </form>
      </Card>
      {demo && (
        <Card className="mt-4 p-4">
          <p className="text-sm font-medium">Comptes de démonstration (entreprise fictive)</p>
          <p className="mb-2 text-xs text-muted">Mot de passe : valeur de DEMO_PASSWORD.</p>
          <ul className="flex flex-col gap-1">
            {DEMO_ACCOUNTS.map((a) => (
              <li key={a.email}>
                <button
                  type="button"
                  onClick={() => setEmail(a.email)}
                  className="w-full rounded-md px-2 py-1 text-left text-sm text-muted hover:bg-surface-2 hover:text-fg"
                >
                  {a.label}
                </button>
              </li>
            ))}
          </ul>
        </Card>
      )}
    </AuthFrame>
  );
}
