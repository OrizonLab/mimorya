'use client';

import { Checkbox } from '@/components/ui';
import type { Policies } from '@/lib/api';
import { PERMISSION_GROUPS } from '@/lib/labels';

/**
 * The permissions a policy names, grouped by domain. A deny policy may name a
 * whole domain; an allow policy only permissions checked resource by
 * resource, one by one (`grant`, ADR-168): on any other it would never apply.
 */
export function PermissionPicker({
  data,
  selected,
  onToggle,
  grant = false,
}: {
  data: Policies;
  selected: Set<string>;
  onToggle: (permission: string) => void;
  grant?: boolean;
}) {
  const offered = grant ? data.permissions.filter((p) => p.perResource) : data.permissions;
  // Stored before a rule refused them, or written by hand: shown so that they
  // can be unchecked, never offered.
  const shown = new Set(offered.map((p) => p.permission));
  const kept = [...selected].filter(
    (p) => !shown.has(p) && !(p.endsWith(':*') && !grant && data.domains.includes(p.slice(0, -2))),
  );
  const byDomain = new Map<string, Policies['permissions']>();
  for (const p of offered) {
    const domain = p.permission.split(':')[0] ?? p.permission;
    byDomain.set(domain, [...(byDomain.get(domain) ?? []), p]);
  }
  return (
    <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
      {[...byDomain.entries()].map(([domain, permissions]) => {
        const whole = `${domain}:*`;
        return (
          <fieldset key={domain} className="flex flex-col gap-1 text-sm">
            <legend className="mb-1 text-xs font-medium text-muted">
              {PERMISSION_GROUPS[domain] ?? domain}
            </legend>
            {!grant && data.domains.includes(domain) && (
              <label className="flex items-start gap-2">
                <Checkbox
                  className="mt-0.5"
                  checked={selected.has(whole)}
                  onChange={() => onToggle(whole)}
                />
                <span>Toutes les permissions de ce domaine</span>
              </label>
            )}
            {permissions.map((p) => (
              <label key={p.permission} className="flex items-start gap-2">
                <Checkbox
                  className="mt-0.5"
                  checked={selected.has(p.permission)}
                  onChange={() => onToggle(p.permission)}
                />
                <span>
                  {p.label}
                  {p.perResource && !grant && <span className="text-muted"> · par ressource</span>}
                </span>
              </label>
            ))}
          </fieldset>
        );
      })}
      {kept.length > 0 && (
        <fieldset className="flex flex-col gap-1 text-sm">
          <legend className="mb-1 text-xs font-medium text-muted">
            Enregistrées, plus acceptées ici : à décocher
          </legend>
          {kept.map((permission) => (
            <label key={permission} className="flex items-start gap-2">
              <Checkbox className="mt-0.5" checked onChange={() => onToggle(permission)} />
              <span>{permission}</span>
            </label>
          ))}
        </fieldset>
      )}
    </div>
  );
}

/** A set of permissions with `permission` switched. */
export function toggled(current: Set<string>, permission: string): Set<string> {
  const next = new Set(current);
  if (next.has(permission)) next.delete(permission);
  else next.add(permission);
  return next;
}
