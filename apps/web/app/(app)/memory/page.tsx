'use client';

import { Trash2 } from 'lucide-react';
import { useCallback, useEffect, useState, type FormEvent } from 'react';
import { Badge, Button, Card, EmptyState, ErrorNote, PageHeader, Spinner } from '@/components/ui';
import { api, ApiError, formatDate } from '@/lib/api';

interface MemoryItem {
  id: string;
  scope: 'user' | 'org_unit' | 'tenant' | 'experience';
  orgUnit: string | null;
  kind: string;
  title: string;
  content: string;
  status: 'active' | 'proposed' | 'archived';
  useCount: number;
  updatedAt: string;
  canManage: boolean;
}

const SPACES: Array<{ scope: MemoryItem['scope']; title: string; description: string }> = [
  {
    scope: 'user',
    title: 'Ma mémoire',
    description: 'Visible par vous seul (et par l’agent quand il travaille pour vous).',
  },
  {
    scope: 'org_unit',
    title: 'Équipes et services',
    description: 'Connaissances partagées de vos équipes.',
  },
  {
    scope: 'tenant',
    title: 'Entreprise',
    description: 'Connaissances communes à toute l’entreprise.',
  },
  {
    scope: 'experience',
    title: 'Expérience de l’agent',
    description: 'Leçons et procédures validées.',
  },
];

const KINDS: Record<string, string> = {
  preference: 'Préférence',
  fact: 'Fait',
  procedure: 'Procédure',
  decision: 'Décision',
  lesson: 'Leçon',
  episode: 'Épisode',
};

export default function MemoryPage() {
  const [items, setItems] = useState<MemoryItem[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [title, setTitle] = useState('');
  const [content, setContent] = useState('');
  const [saving, setSaving] = useState(false);

  const load = useCallback(() => {
    api
      .get<{ items: MemoryItem[] }>('/memory')
      .then((r) => setItems(r.items))
      .catch(() => setItems([]));
  }, []);
  useEffect(load, [load]);

  async function run(action: () => Promise<unknown>) {
    setError(null);
    try {
      await action();
      load();
    } catch (e) {
      setError(e instanceof ApiError ? e.message : 'Action impossible');
    }
  }

  async function add(event: FormEvent) {
    event.preventDefault();
    setSaving(true);
    await run(() => api.post('/memory', { space: 'personal', kind: 'preference', title, content }));
    setTitle('');
    setContent('');
    setSaving(false);
  }

  const proposals = items?.filter((m) => m.status === 'proposed') ?? [];

  return (
    <div className="mx-auto w-full max-w-4xl px-4 py-8">
      <PageHeader
        title="Mémoire"
        description="Ce que l’agent sait et peut utiliser pour vous aider. Vous pouvez consulter, ajouter et supprimer vos informations à tout moment."
      />
      {error && (
        <div className="mb-4">
          <ErrorNote message={error} />
        </div>
      )}
      {items === null && <Spinner />}

      {proposals.length > 0 && (
        <Card className="mb-6 p-4">
          <h2 className="text-sm font-semibold">Propositions à valider</h2>
          <p className="text-xs text-muted">
            Connaissances proposées par l’agent pour un espace partagé dont vous êtes responsable.
          </p>
          <ul className="mt-3 flex flex-col gap-3">
            {proposals.map((m) => (
              <li key={m.id} className="rounded-lg border border-line p-3">
                <p className="text-sm font-medium">{m.title}</p>
                <p className="mt-1 text-sm text-muted">{m.content}</p>
                <div className="mt-2 flex gap-2">
                  <Button
                    onClick={() =>
                      run(() => api.post(`/memory/${m.id}/review`, { decision: 'approve' }))
                    }
                  >
                    Valider
                  </Button>
                  <Button
                    variant="secondary"
                    onClick={() =>
                      run(() => api.post(`/memory/${m.id}/review`, { decision: 'reject' }))
                    }
                  >
                    Rejeter
                  </Button>
                </div>
              </li>
            ))}
          </ul>
        </Card>
      )}

      {items &&
        SPACES.map((space) => {
          const list = items.filter((m) => m.scope === space.scope && m.status === 'active');
          return (
            <section key={space.scope} className="mb-6">
              <h2 className="text-sm font-semibold">{space.title}</h2>
              <p className="mb-2 text-xs text-muted">{space.description}</p>
              {list.length === 0 ? (
                <EmptyState title="Rien pour le moment" />
              ) : (
                <Card className="divide-y divide-line">
                  {list.map((m) => (
                    <div key={m.id} className="flex items-start gap-3 px-4 py-3">
                      <div className="min-w-0 flex-1">
                        <div className="flex flex-wrap items-center gap-2">
                          <p className="text-sm font-medium">{m.title}</p>
                          <Badge>{KINDS[m.kind] ?? m.kind}</Badge>
                          {m.orgUnit && <Badge tone="accent">{m.orgUnit}</Badge>}
                        </div>
                        <p className="mt-1 text-sm text-muted">{m.content}</p>
                        <p className="mt-1 text-xs text-muted">
                          Mis à jour le {formatDate(m.updatedAt)} · utilisé {m.useCount} fois
                        </p>
                      </div>
                      {m.canManage && (
                        <button
                          type="button"
                          aria-label={`Supprimer « ${m.title} »`}
                          onClick={() => run(() => api.delete(`/memory/${m.id}`))}
                          className="rounded-md p-1.5 text-muted hover:bg-danger-soft hover:text-danger"
                        >
                          <Trash2 className="size-4" />
                        </button>
                      )}
                    </div>
                  ))}
                </Card>
              )}
              {space.scope === 'user' && (
                <form onSubmit={add} className="mt-3 flex flex-col gap-2 sm:flex-row">
                  <input
                    required
                    minLength={3}
                    value={title}
                    onChange={(e) => setTitle(e.target.value)}
                    placeholder="Intitulé (ex. Format préféré)"
                    className="rounded-lg border border-line bg-surface px-3 py-2 text-sm outline-none focus:border-accent sm:w-56"
                  />
                  <input
                    required
                    minLength={3}
                    value={content}
                    onChange={(e) => setContent(e.target.value)}
                    placeholder="Ce que l’agent doit retenir"
                    className="flex-1 rounded-lg border border-line bg-surface px-3 py-2 text-sm outline-none focus:border-accent"
                  />
                  <Button type="submit" variant="secondary" loading={saving}>
                    Ajouter
                  </Button>
                </form>
              )}
            </section>
          );
        })}
    </div>
  );
}
