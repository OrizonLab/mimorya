'use client';

import { useState, type FormEvent } from 'react';
import { Button, ErrorNote, Field, Input } from '@/components/ui';
import {
  api,
  useAction,
  type NewPolicy,
  type Policies,
  type PolicyUpdate,
  type PolicyView,
} from '@/lib/api';
import { ConditionEditor, editorCondition, initialEditor } from './condition-editor';
import { PermissionPicker, toggled } from './permission-picker';

/** Creates a deny policy, or edits `policy`: description, permissions, condition. */
export function PolicyForm({
  data,
  policy,
  onDone,
}: {
  data: Policies;
  policy?: PolicyView;
  onDone: (changed: boolean) => void;
}) {
  const [description, setDescription] = useState(policy?.description ?? '');
  const [restricted, setRestricted] = useState(new Set(policy?.permissions ?? []));
  const [editor, setEditor] = useState(() => initialEditor(policy?.condition));
  // An edit sends the condition only when it was changed.
  const [conditionChanged, setConditionChanged] = useState(false);
  const [problem, setProblem] = useState<string | null>(null);
  const action = useAction();

  async function submit(event: FormEvent) {
    event.preventDefault();
    const { condition, error } = editorCondition(editor);
    setProblem(error ?? null);
    if (!condition) return;
    const permissions = [...restricted];
    const saved = await action.run(() =>
      policy
        ? api.patch(`/admin/policies/${policy.id}`, {
            description,
            permissions,
            condition: conditionChanged ? condition : undefined,
          } satisfies PolicyUpdate)
        : api.post('/admin/policies', { description, permissions, condition } satisfies NewPolicy),
    );
    if (saved) onDone(true);
  }

  return (
    <form onSubmit={submit} className="flex flex-col gap-4 rounded-lg bg-surface-2 p-3">
      <Field label="Description (ce que la politique interdit, et à qui)">
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
        selected={restricted}
        onToggle={(permission) => setRestricted((current) => toggled(current, permission))}
      />
      <ConditionEditor
        value={editor}
        attributes={data.attributes}
        onChange={(state) => {
          setEditor(state);
          setConditionChanged(true);
        }}
      />
      <p className="text-xs text-muted">
        Une politique ne fait que refuser : quand sa condition est remplie, elle retire la
        permission même si un rôle l’accorde. Une condition sur le collaborateur ou le contexte (
        <code>subject.</code>, <code>context.</code>) vaut partout où la permission est vérifiée,
        outils de l’agent compris ; une condition sur la ressource (<code>resource.</code>) ne
        s’accepte que pour les permissions vérifiées « par ressource ». Valeurs : <code>12</code>{' '}
        est un nombre, <code>"12"</code> un texte.
      </p>
      <p className="text-xs text-muted">
        Attention : une condition remplie par tout le monde (par exemple « Identifiant du
        collaborateur est renseigné ») retire la permission à toute l’entreprise, administrateurs
        compris.
      </p>
      {problem && <ErrorNote message={problem} />}
      {action.error && <ErrorNote message={action.error} />}
      <div className="flex gap-2">
        <Button type="submit" loading={action.pending} disabled={restricted.size === 0}>
          {policy ? 'Enregistrer' : 'Créer la politique'}
        </Button>
        <Button
          type="button"
          variant="ghost"
          // Closed before its change is answered, it could be reopened from the
          // former resource, and that change overwritten.
          disabled={action.pending}
          onClick={() => onDone(false)}
        >
          Annuler
        </Button>
      </div>
    </form>
  );
}
