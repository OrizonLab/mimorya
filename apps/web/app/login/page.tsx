'use client';

import { useRouter } from 'next/navigation';
import { useState, type FormEvent } from 'react';
import { Button, Card, ErrorNote } from '@/components/ui';
import { api, ApiError } from '@/lib/api';

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
  const demo = process.env.NEXT_PUBLIC_DEMO_MODE === 'true';

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
    <div className="flex min-h-dvh items-center justify-center px-4 py-10">
      <div className="w-full max-w-sm">
        <div className="mb-6 text-center">
          <h1 className="text-2xl font-semibold tracking-tight">Mimorya</h1>
          <p className="mt-1 text-sm text-muted">Votre collaborateur IA d’entreprise</p>
        </div>
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
      </div>
    </div>
  );
}
