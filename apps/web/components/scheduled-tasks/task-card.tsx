'use client';

import { useState } from 'react';
import { Badge, Button, Card, ErrorNote } from '@/components/ui';
import { api, useAction, type ScheduledTask, type ScheduledTaskUpdate } from '@/lib/api';
import { formatDate } from '@/lib/format';
import { LastOccurrence } from './last-occurrence';
import { describeSchedule } from './schedule';
import { TaskForm, type TaskDraft } from './task-form';

/**
 * One of the reader's scheduled tasks: when it runs and runs next, what it
 * asks, what became of its last occurrence; changed, paused or resumed, and
 * deleted from here. Resuming one needs the right to use the agent; pausing
 * or deleting it does not (ADR-080).
 */
export function TaskCard({ task, onChanged }: { task: ScheduledTask; onChanged: () => void }) {
  const [editing, setEditing] = useState(false);
  const action = useAction();
  const url = `/scheduled-tasks/${task.id}`;

  async function change(body: ScheduledTaskUpdate) {
    if ((await action.run(() => api.patch(url, body))) !== null) onChanged();
  }

  async function remove() {
    if ((await action.run(() => api.delete(url))) !== null) onChanged();
  }

  async function save(draft: TaskDraft) {
    await api.patch(url, {
      title: draft.title,
      instruction: draft.instruction,
      weekdays: draft.weekdays,
      time: draft.time,
      timeZone: draft.timeZone,
    } satisfies ScheduledTaskUpdate);
    setEditing(false);
    onChanged();
  }

  return (
    // Named by its title: each task is found by it.
    <article aria-label={task.title}>
      <Card className="flex flex-col gap-2 p-4">
        <div className="flex flex-wrap items-center gap-2">
          <p className="font-medium">{task.title}</p>
          <Badge tone={task.enabled ? 'ok' : 'neutral'}>
            {task.enabled ? 'Active' : 'En pause'}
          </Badge>
        </div>
        <p className="text-sm">
          {describeSchedule(task.weekdays, task.time)}{' '}
          <span className="text-muted">({task.timeZone})</span>
        </p>
        {task.nextRunAt && (
          <p className="text-xs text-muted">Prochaine occurrence : {formatDate(task.nextRunAt)}</p>
        )}
        <p className="text-sm whitespace-pre-line text-muted">{task.instruction}</p>
        <LastOccurrence task={task} />
        {editing ? (
          <TaskForm
            initial={{
              title: task.title,
              instruction: task.instruction,
              weekdays: task.weekdays,
              time: task.time,
              timeZone: task.timeZone,
            }}
            submitLabel="Enregistrer"
            onSave={save}
            onCancel={() => setEditing(false)}
          />
        ) : (
          <div className="flex flex-wrap gap-2">
            <Button
              variant="secondary"
              aria-label={`Modifier « ${task.title} »`}
              onClick={() => setEditing(true)}
            >
              Modifier
            </Button>
            <Button
              variant="secondary"
              aria-label={`${task.enabled ? 'Mettre en pause' : 'Reprendre'} « ${task.title} »`}
              loading={action.pending}
              onClick={() => void change({ enabled: !task.enabled } satisfies ScheduledTaskUpdate)}
            >
              {task.enabled ? 'Mettre en pause' : 'Reprendre'}
            </Button>
            <Button
              variant="danger"
              aria-label={`Supprimer « ${task.title} »`}
              disabled={action.pending}
              onClick={() => void remove()}
            >
              Supprimer
            </Button>
          </div>
        )}
        {action.error && <ErrorNote message={action.error} />}
      </Card>
    </article>
  );
}
