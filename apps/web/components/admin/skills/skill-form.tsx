'use client';

import { useState, type FormEvent } from 'react';
import { Button, ErrorNote, Field, Input, Textarea } from '@/components/ui';
import { api, useAction, type NewSkill } from '@/lib/api';

/** Writes a skill and its first version, both drafts. */
export function SkillForm({ onDone }: { onDone: (changed: boolean) => void }) {
  const [name, setName] = useState('');
  const [description, setDescription] = useState('');
  const [content, setContent] = useState('');
  const [criteria, setCriteria] = useState('');
  const [changelog, setChangelog] = useState('');
  const action = useAction();

  async function submit(event: FormEvent) {
    event.preventDefault();
    const body = {
      name,
      description,
      content,
      qualityCriteria: criteria.split('\n'),
      changelog,
    } satisfies NewSkill;
    if (await action.run(() => api.post('/admin/skills', body))) onDone(true);
  }

  return (
    <form onSubmit={submit} className="flex flex-col gap-3 rounded-lg bg-surface-2 p-3">
      <div className="grid gap-3 sm:grid-cols-2">
        <Field label="Nom (lettres minuscules, chiffres et tirets)">
          <Input
            required
            minLength={2}
            maxLength={64}
            pattern="[a-z0-9]+(-[a-z0-9]+)*"
            placeholder="relance-client"
            value={name}
            onChange={(e) => setName(e.target.value)}
          />
        </Field>
        <Field label="Description (une ligne, lue par l’agent)">
          <Input
            required
            minLength={10}
            maxLength={300}
            value={description}
            onChange={(e) => setDescription(e.target.value)}
          />
        </Field>
      </div>
      <SkillContentFields
        content={content}
        criteria={criteria}
        changelog={changelog}
        onContent={setContent}
        onCriteria={setCriteria}
        onChangelog={setChangelog}
      />
      {action.error && <ErrorNote message={action.error} />}
      <div className="flex gap-2">
        <Button type="submit" loading={action.pending}>
          Créer la compétence
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

/**
 * A version's content, the quality criteria any answer that follows it is
 * reviewed on (ADR-106), one per line, and its note: shared by the two forms
 * that write one.
 */
export function SkillContentFields({
  content,
  criteria,
  changelog,
  onContent,
  onCriteria,
  onChangelog,
}: {
  content: string;
  criteria: string;
  changelog: string;
  onContent: (content: string) => void;
  onCriteria: (criteria: string) => void;
  onChangelog: (changelog: string) => void;
}) {
  return (
    <>
      <Field label="Contenu (procédure en Markdown)">
        <Textarea
          required
          rows={12}
          maxLength={20000}
          className="font-mono"
          value={content}
          onChange={(e) => onContent(e.target.value)}
        />
      </Field>
      <Field label="Critères de qualité de la réponse (facultatifs, un par ligne, 8 au plus)">
        <Textarea rows={3} value={criteria} onChange={(e) => onCriteria(e.target.value)} />
      </Field>
      <p className="text-xs text-muted">
        Chaque réponse de l’assistant qui a suivi cette version est relue par un modèle sur ces
        critères, comme lors d’une évaluation : un manquement est corrigé, sinon signalé sous la
        réponse.
      </p>
      <Field label="Note de version (facultative)">
        <Input maxLength={300} value={changelog} onChange={(e) => onChangelog(e.target.value)} />
      </Field>
    </>
  );
}
