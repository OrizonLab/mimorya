'use client';

import { useState, type FormEvent } from 'react';
import { Button, Checkbox, ErrorNote, Field, Input } from '@/components/ui';
import { api, useAction, type NewPolicyProposal, type Policies, type PolicyView } from '@/lib/api';
import { ConditionEditor, editorCondition, initialEditor } from './condition-editor';
import { PermissionPicker, toggled } from './permission-picker';

/**
 * Proposes an allow policy (ADR-168), a new one or `policy` changed, as it
 * will stand: another administrator approves it before it opens anything.
 */
export function GrantForm({
  data,
  policy,
  onDone,
}: {
  data: Policies;
  policy?: PolicyView;
  onDone: (proposed: boolean) => void;
}) {
  const [description, setDescription] = useState(policy?.description ?? '');
  const [granted, setGranted] = useState(new Set(policy?.permissions ?? []));
  const [editor, setEditor] = useState(() => initialEditor(policy?.condition));
  const [enabled, setEnabled] = useState(policy?.enabled ?? true);
  const [problem, setProblem] = useState<string | null>(null);
  const action = useAction();

  async function submit(event: FormEvent) {
    event.preventDefault();
    const { condition, error } = editorCondition(editor);
    setProblem(error ?? null);
    if (!condition) return;
    // The proposal states the whole policy, its condition included.
    const body = {
      policyId: policy?.id,
      // Changed since the form opened, the policy is not proposed on.
      policyUpdatedAt: policy?.updatedAt,
      description,
      permissions: [...granted],
      condition,
      enabled,
    } satisfies NewPolicyProposal;
    if (await action.run(() => api.post('/admin/policy-proposals', body))) onDone(true);
  }

  return (
    <form onSubmit={submit} className="flex flex-col gap-4 rounded-lg bg-surface-2 p-3">
      <Field label="Description (ce que la politique autorise, et à qui)">
        <Input
          required
          minLength={3}
          maxLength={300}
          value={description}
          onChange={(e) => setDescription(e.target.value)}
        />
      </Field>
      <PermissionPicker
        data={data}
        selected={granted}
        grant
        onToggle={(permission) => setGranted((current) => toggled(current, permission))}
      />
      <ConditionEditor value={editor} attributes={data.attributes} onChange={setEditor} />
      <label className="flex items-center gap-2 text-sm">
        <Checkbox checked={enabled} onChange={() => setEnabled((on) => !on)} />
        Active une fois approuvée
      </label>
      <p className="text-xs text-muted">
        Une autorisation ouvre, ressource par ressource, celles que sa condition désigne à qui
        détient déjà la permission sur une partie de l’entreprise ; elle n’accorde jamais la
        permission elle-même. Un refus et l’habilitation de la personne l’emportent sur elle. Elle
        ne s’applique qu’une fois approuvée par un autre administrateur des politiques, dans les
        sept jours, et seulement si la politique n’a pas changé entre-temps.
      </p>
      {problem && <ErrorNote message={problem} />}
      {action.error && <ErrorNote message={action.error} />}
      <div className="flex gap-2">
        <Button type="submit" loading={action.pending} disabled={granted.size === 0}>
          Proposer
        </Button>
        <Button
          type="button"
          variant="ghost"
          disabled={action.pending}
          onClick={() => onDone(false)}
        >
          Annuler
        </Button>
      </div>
    </form>
  );
}
