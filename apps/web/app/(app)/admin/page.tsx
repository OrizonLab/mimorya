'use client';

import { useEffect, useMemo, useState, type ReactNode } from 'react';
import { Badge, Card, cx, EmptyState, PageHeader, Spinner } from '@/components/ui';
import { api, formatCost, formatDate } from '@/lib/api';
import { useMe } from '@/lib/me-context';

interface Tab {
  key: string;
  label: string;
  permission: string;
  render: () => ReactNode;
}

export default function AdminPage() {
  const me = useMe();
  const tabs = useMemo<Tab[]>(
    () =>
      [
        {
          key: 'org',
          label: 'Organisation',
          permission: 'org:read',
          render: () => <Organization />,
        },
        { key: 'users', label: 'Utilisateurs', permission: 'user:read', render: () => <Users /> },
        { key: 'roles', label: 'Rôles', permission: 'role:manage', render: () => <Roles /> },
        {
          key: 'audit',
          label: 'Journal d’audit',
          permission: 'audit:read',
          render: () => <Audit />,
        },
        { key: 'usage', label: 'Consommation', permission: 'usage:read', render: () => <Usage /> },
        {
          key: 'connectors',
          label: 'Connecteurs',
          permission: 'connector:manage',
          render: () => <Connectors />,
        },
        { key: 'skills', label: 'Compétences', permission: 'skill:read', render: () => <Skills /> },
      ].filter((t) => me.permissions.includes(t.permission)),
    [me.permissions],
  );
  const [active, setActive] = useState<string | null>(null);
  const current = tabs.find((t) => t.key === active) ?? tabs[0];

  return (
    <div className="mx-auto w-full max-w-6xl px-4 py-8">
      <PageHeader
        title="Administration"
        description="Organisation, habilitations, traçabilité et consommation de votre espace. Les écrans affichés dépendent de vos permissions."
      />
      <div className="mb-6 flex gap-1 overflow-x-auto border-b border-line" role="tablist">
        {tabs.map((t) => (
          <button
            key={t.key}
            type="button"
            role="tab"
            aria-selected={current?.key === t.key}
            onClick={() => setActive(t.key)}
            className={cx(
              '-mb-px border-b-2 px-3 py-2 text-sm whitespace-nowrap',
              current?.key === t.key
                ? 'border-accent font-medium text-accent'
                : 'border-transparent text-muted hover:text-fg',
            )}
          >
            {t.label}
          </button>
        ))}
      </div>
      {current ? current.render() : <EmptyState title="Aucun écran d’administration accessible" />}
    </div>
  );
}

function useData<T>(path: string): T | null {
  const [data, setData] = useState<T | null>(null);
  useEffect(() => {
    api
      .get<T>(path)
      .then(setData)
      .catch(() => undefined);
  }, [path]);
  return data;
}

interface OrgUnitView {
  id: string;
  parentId: string | null;
  name: string;
  type: string;
  leadUserId: string | null;
  members: string[];
}

const UNIT_TYPES: Record<string, string> = {
  company: 'Entreprise',
  site: 'Établissement',
  direction: 'Direction',
  department: 'Service',
  service: 'Service',
  team: 'Équipe',
};

function Organization() {
  const data = useData<{
    units: OrgUnitView[];
    users: Array<{ id: string; displayName: string; jobTitle: string | null }>;
  }>('/admin/organization');
  if (!data) return <Spinner />;
  const users = new Map(data.users.map((u) => [u.id, u]));
  const render = (parentId: string | null, depth: number): ReactNode =>
    data.units
      .filter((u) => u.parentId === parentId)
      .map((unit) => (
        <li key={unit.id} style={{ marginLeft: depth * 16 }} className="py-1.5">
          <div className="flex flex-wrap items-center gap-2">
            <span className="font-medium">{unit.name}</span>
            <Badge>{UNIT_TYPES[unit.type] ?? unit.type}</Badge>
            {unit.leadUserId && (
              <span className="text-xs text-muted">
                Responsable : {users.get(unit.leadUserId)?.displayName}
              </span>
            )}
          </div>
          {unit.members.length > 0 && (
            <p className="mt-0.5 text-xs text-muted">
              {unit.members.map((m) => users.get(m)?.displayName).join(' · ')}
            </p>
          )}
          <ul>{render(unit.id, depth + 1)}</ul>
        </li>
      ));
  return (
    <Card className="p-4">
      <ul className="text-sm">{render(null, 0)}</ul>
      <p className="mt-4 text-xs text-muted">
        Le responsable d’une unité est une information organisationnelle : il n’accorde aucun droit
        d’accès. Les accès proviennent uniquement des rôles attribués sur un périmètre.
      </p>
    </Card>
  );
}

function Users() {
  const data = useData<{
    users: Array<{
      id: string;
      displayName: string;
      jobTitle: string | null;
      orgUnit: string | null;
      email?: string;
      lastLoginAt?: string | null;
      grants?: string[];
    }>;
  }>('/admin/users');
  if (!data) return <Spinner />;
  return (
    <Card className="overflow-x-auto">
      <table className="w-full text-sm">
        <thead className="bg-surface-2 text-left text-xs text-muted">
          <tr>
            <th className="px-4 py-2 font-medium">Collaborateur</th>
            <th className="px-4 py-2 font-medium">Unité</th>
            <th className="px-4 py-2 font-medium">Habilitations</th>
            <th className="px-4 py-2 font-medium">Dernière connexion</th>
          </tr>
        </thead>
        <tbody className="divide-y divide-line">
          {data.users.map((u) => (
            <tr key={u.id} className="align-top">
              <td className="px-4 py-2">
                <p className="font-medium">{u.displayName}</p>
                <p className="text-xs text-muted">{u.jobTitle}</p>
                {u.email && <p className="text-xs text-muted">{u.email}</p>}
              </td>
              <td className="px-4 py-2 text-muted">{u.orgUnit ?? '—'}</td>
              <td className="px-4 py-2">
                <ul className="flex flex-col gap-1">
                  {(u.grants ?? []).map((g) => (
                    <li key={g} className="text-xs">
                      {g}
                    </li>
                  ))}
                </ul>
              </td>
              <td className="px-4 py-2 text-xs whitespace-nowrap text-muted">
                {formatDate(u.lastLoginAt)}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </Card>
  );
}

function Roles() {
  const data = useData<{
    roles: Array<{
      id: string;
      name: string;
      description: string;
      isSystem: boolean;
      permissions: string[];
    }>;
    catalog: Array<{ permission: string; label: string; delegable: boolean }>;
  }>('/admin/roles');
  if (!data) return <Spinner />;
  const labels = new Map(data.catalog.map((p) => [p.permission, p]));
  return (
    <div className="grid gap-4 md:grid-cols-2">
      {data.roles.map((r) => (
        <Card key={r.id} className="p-4">
          <div className="flex items-center gap-2">
            <p className="font-medium">{r.name}</p>
            {r.isSystem && <Badge>Rôle plateforme</Badge>}
          </div>
          <p className="mt-1 text-xs text-muted">{r.description}</p>
          <ul className="mt-3 flex flex-wrap gap-1">
            {r.permissions.map((p) => (
              <li key={p}>
                <Badge tone={labels.get(p)?.delegable === false ? 'warn' : 'neutral'}>
                  {labels.get(p)?.label ?? p}
                </Badge>
              </li>
            ))}
          </ul>
        </Card>
      ))}
      <p className="text-xs text-muted md:col-span-2">
        Les permissions en orange ne peuvent jamais être exercées par l’agent, quels que soient les
        droits de l’utilisateur.
      </p>
    </div>
  );
}

const OUTCOMES: Record<string, string> = { success: 'Succès', denied: 'Refusé', failure: 'Échec' };

function Audit() {
  const data = useData<{
    entries: Array<{
      id: number;
      occurredAt: string;
      actorType: string;
      action: string;
      outcome: string;
      resourceType: string | null;
      resourceId: string | null;
      reason: string | null;
    }>;
  }>('/admin/audit?limit=200');
  if (!data) return <Spinner />;
  return (
    <Card className="overflow-x-auto">
      <table className="w-full text-sm">
        <thead className="bg-surface-2 text-left text-xs text-muted">
          <tr>
            <th className="px-4 py-2 font-medium">Date</th>
            <th className="px-4 py-2 font-medium">Acteur</th>
            <th className="px-4 py-2 font-medium">Action</th>
            <th className="px-4 py-2 font-medium">Résultat</th>
            <th className="px-4 py-2 font-medium">Détail</th>
          </tr>
        </thead>
        <tbody className="divide-y divide-line">
          {data.entries.map((e) => (
            <tr key={e.id}>
              <td className="px-4 py-2 text-xs whitespace-nowrap text-muted">
                {formatDate(e.occurredAt)}
              </td>
              <td className="px-4 py-2">
                <Badge tone={e.actorType === 'agent' ? 'accent' : 'neutral'}>
                  {e.actorType === 'agent'
                    ? 'Agent'
                    : e.actorType === 'user'
                      ? 'Utilisateur'
                      : 'Système'}
                </Badge>
              </td>
              <td className="px-4 py-2 font-mono text-xs">{e.action}</td>
              <td className="px-4 py-2">
                <Badge
                  tone={e.outcome === 'success' ? 'ok' : e.outcome === 'denied' ? 'danger' : 'warn'}
                >
                  {OUTCOMES[e.outcome] ?? e.outcome}
                </Badge>
              </td>
              <td className="px-4 py-2 text-xs text-muted">
                {[e.resourceType, e.resourceId].filter(Boolean).join(' · ')}
                {e.reason && <span className="block">{e.reason}</span>}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </Card>
  );
}

function Usage() {
  const data = useData<{
    since: string;
    rows: Array<{
      provider: string;
      model: string;
      kind: string;
      calls: number;
      inputTokens: number;
      outputTokens: number;
      costMicros: number;
    }>;
  }>('/admin/usage?days=30');
  if (!data) return <Spinner />;
  const total = data.rows.reduce((sum, r) => sum + r.costMicros, 0);
  return (
    <div className="flex flex-col gap-4">
      <Card className="p-4">
        <p className="text-xs text-muted">Coût estimé sur 30 jours</p>
        <p className="mt-1 text-2xl font-semibold">{formatCost(total)}</p>
        <p className="mt-1 text-xs text-muted">
          Estimation à partir des prix publics des modèles ; les modèles non tarifés comptent pour
          0.
        </p>
      </Card>
      {data.rows.length === 0 ? (
        <EmptyState title="Aucune consommation sur la période" />
      ) : (
        <Card className="overflow-x-auto">
          <table className="w-full text-sm">
            <thead className="bg-surface-2 text-left text-xs text-muted">
              <tr>
                <th className="px-4 py-2 font-medium">Modèle</th>
                <th className="px-4 py-2 font-medium">Appels</th>
                <th className="px-4 py-2 font-medium">Tokens entrée</th>
                <th className="px-4 py-2 font-medium">Tokens sortie</th>
                <th className="px-4 py-2 font-medium">Coût</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-line">
              {data.rows.map((r) => (
                <tr key={`${r.provider}:${r.model}:${r.kind}`}>
                  <td className="px-4 py-2">
                    {r.model} <span className="text-xs text-muted">({r.provider})</span>
                  </td>
                  <td className="px-4 py-2">{r.calls}</td>
                  <td className="px-4 py-2">{r.inputTokens.toLocaleString('fr-FR')}</td>
                  <td className="px-4 py-2">{r.outputTokens.toLocaleString('fr-FR')}</td>
                  <td className="px-4 py-2">{formatCost(r.costMicros)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </Card>
      )}
    </div>
  );
}

function Connectors() {
  const data = useData<{
    instances: Array<{
      id: string;
      type: string;
      name: string;
      enabled: boolean;
      status: string;
      updatedAt: string;
    }>;
    types: Array<{ type: string; name: string; description: string; demo: boolean }>;
  }>('/admin/connectors');
  if (!data) return <Spinner />;
  return (
    <div className="flex flex-col gap-6">
      <section>
        <h2 className="mb-2 text-sm font-semibold">Connecteurs actifs</h2>
        <Card className="divide-y divide-line">
          {data.instances.map((i) => (
            <div key={i.id} className="flex flex-wrap items-center gap-2 px-4 py-3 text-sm">
              <span className="flex-1 font-medium">{i.name}</span>
              <Badge>{i.type}</Badge>
              <Badge tone={i.enabled ? 'ok' : 'neutral'}>
                {i.enabled ? 'Activé' : 'Désactivé'}
              </Badge>
            </div>
          ))}
          {data.instances.length === 0 && (
            <p className="px-4 py-3 text-sm text-muted">Aucun connecteur.</p>
          )}
        </Card>
      </section>
      <section>
        <h2 className="mb-2 text-sm font-semibold">Catalogue</h2>
        <div className="grid gap-3 md:grid-cols-2">
          {data.types.map((t) => (
            <Card key={t.type} className="p-4">
              <div className="flex items-center gap-2">
                <p className="font-medium">{t.name}</p>
                {t.demo && <Badge tone="warn">Démonstration</Badge>}
              </div>
              <p className="mt-1 text-sm text-muted">{t.description}</p>
            </Card>
          ))}
        </div>
      </section>
    </div>
  );
}

function Skills() {
  const data = useData<{
    skills: Array<{
      id: string;
      name: string;
      description: string;
      status: string;
      activeVersion: number | null;
      updatedAt: string;
    }>;
  }>('/admin/skills');
  if (!data) return <Spinner />;
  if (data.skills.length === 0) return <EmptyState title="Aucune compétence" />;
  return (
    <Card className="divide-y divide-line">
      {data.skills.map((s) => (
        <div key={s.id} className="px-4 py-3">
          <div className="flex flex-wrap items-center gap-2">
            <p className="font-mono text-sm">{s.name}</p>
            <Badge tone={s.status === 'active' ? 'ok' : 'neutral'}>
              {s.status === 'active' ? 'Active' : s.status}
            </Badge>
            {s.activeVersion && <Badge>v{s.activeVersion}</Badge>}
          </div>
          <p className="mt-1 text-sm text-muted">{s.description}</p>
        </div>
      ))}
    </Card>
  );
}
