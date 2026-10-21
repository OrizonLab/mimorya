'use client';

import { useState } from 'react';
import { Button, Checkbox, ErrorNote, Select, Textarea } from '@/components/ui';
import {
  api,
  useAction,
  type ConnectorUpdate,
  type Connectors,
  type TestedTool,
  type ToolPolicyChange,
  type ToolRisk,
} from '@/lib/api';
import { RISK_LABELS } from '@/lib/labels';

const ALWAYS = 'connector:use';

/** The longest description a server may declare, or an administrator write. */
const MAX_DESCRIPTION = 1000;

/**
 * Maps each tool of a server to a risk and the permissions it requires, and
 * may replace what the model reads of it (ADR-057).
 */
export function ToolPolicies({
  connectorId,
  data,
  tools,
  onSaved,
}: {
  connectorId: string;
  data: Connectors;
  tools: TestedTool[];
  onSaved: () => void;
}) {
  const [policies, setPolicies] = useState(() =>
    Object.fromEntries(tools.map((t) => [t.name, { ...t }])),
  );
  const action = useAction();
  const choices = data.permissions.filter((p) => p.permission !== ALWAYS);

  function change(name: string, patch: Partial<TestedTool>) {
    setPolicies((current) => ({ ...current, [name]: { ...current[name]!, ...patch } }));
  }

  function toggle(name: string, permission: string) {
    const held = policies[name]!.permissions;
    change(name, {
      permissions: held.includes(permission)
        ? held.filter((p) => p !== permission)
        : [...held, permission],
    });
  }

  async function save() {
    const toolPolicies = Object.fromEntries(
      Object.values(policies).map((p) => [
        p.name,
        {
          enabled: p.enabled,
          risk: p.risk,
          permissions: p.permissions.filter((x) => x !== ALWAYS),
          // Blank: the server's own.
          description: p.replacedDescription?.trim() || undefined,
        } satisfies ToolPolicyChange,
      ]),
    );
    const update = { toolPolicies } satisfies ConnectorUpdate;
    if (await action.run(() => api.patch(`/admin/connectors/${connectorId}`, update))) {
      onSaved();
    }
  }

  return (
    <div className="flex flex-col gap-3">
      {Object.values(policies).map((tool) => (
        <div key={tool.name} className="flex flex-col gap-2 rounded-lg bg-surface-2 p-3 text-sm">
          <label className="flex items-start gap-2">
            <Checkbox
              className="mt-0.5"
              checked={tool.enabled}
              onChange={(e) => change(tool.name, { enabled: e.target.checked })}
            />
            <span>
              <span className="font-medium">{tool.title}</span>{' '}
              <code className="text-xs text-muted">{tool.name}</code>
              <span className="block text-xs text-muted">{tool.description}</span>
            </span>
          </label>
          <div className="grid gap-2 sm:grid-cols-[12rem_1fr]">
            <Select
              className="self-start"
              aria-label={`Risque de ${tool.name}`}
              value={tool.risk}
              onChange={(e) => change(tool.name, { risk: e.target.value as ToolRisk })}
            >
              {data.risks.map((risk) => (
                <option key={risk} value={risk}>
                  {RISK_LABELS[risk]?.label ?? risk}
                </option>
              ))}
            </Select>
            <fieldset className="flex flex-wrap gap-x-4 gap-y-1">
              <legend className="sr-only">Permissions requises pour {tool.name}</legend>
              <label className="flex items-center gap-1 text-muted">
                <Checkbox checked disabled /> Utiliser les connecteurs
              </label>
              {choices.map((p) => (
                <label key={p.permission} className="flex items-center gap-1">
                  <Checkbox
                    checked={tool.permissions.includes(p.permission)}
                    onChange={() => toggle(tool.name, p.permission)}
                  />
                  {p.label}
                </label>
              ))}
            </fieldset>
          </div>
          <label className="flex flex-col gap-1 text-xs text-muted">
            Description lue par le modèle (vide : celle du serveur)
            <Textarea
              aria-label={`Description de ${tool.name} lue par le modèle`}
              rows={2}
              maxLength={MAX_DESCRIPTION}
              placeholder={tool.description}
              value={tool.replacedDescription ?? ''}
              onChange={(e) => change(tool.name, { replacedDescription: e.target.value })}
            />
          </label>
        </div>
      ))}
      <p className="text-xs text-muted">
        Un outil non associé demande une validation humaine (risque « Modification ») et la seule
        permission d’utiliser les connecteurs. Les risques « Lecture » et « Brouillon » s’exécutent
        sans validation selon le niveau d’autonomie de l’entreprise. Une description remplacée est
        ce que lit le modèle pour choisir l’outil, toujours précédée du nom du serveur : elle ne
        change ni son risque ni ses permissions.
      </p>
      {action.error && <ErrorNote message={action.error} />}
      <Button className="w-fit" loading={action.pending} onClick={save}>
        Enregistrer les règles des outils
      </Button>
    </div>
  );
}
