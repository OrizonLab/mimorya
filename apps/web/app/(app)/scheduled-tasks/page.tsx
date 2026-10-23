'use client';

import { readerTimeZone, TaskCard, TaskForm, type TaskDraft } from '@/components/scheduled-tasks';
import { Card, EmptyState, Loaded, PageHeader } from '@/components/ui';
import { api, useApiData, type NewScheduledTask, type ScheduledTaskList } from '@/lib/api';
import { useCan } from '@/lib/me-context';

/** A new task: weekdays at 8:00, in the reader's time zone. */
function blank(): TaskDraft {
  return {
    title: '',
    instruction: '',
    weekdays: [1, 2, 3, 4, 5],
    time: '08:00',
    timeZone: readerTimeZone(),
  };
}

export default function ScheduledTasksPage() {
  const state = useApiData<ScheduledTaskList>('/scheduled-tasks');
  const mayUseAgent = useCan('agent:use');

  async function create(draft: TaskDraft) {
    await api.post('/scheduled-tasks', {
      title: draft.title,
      instruction: draft.instruction,
      weekdays: draft.weekdays,
      time: draft.time,
      timeZone: draft.timeZone,
    } satisfies NewScheduledTask);
    state.reload();
  }

  return (
    <div className="mx-auto w-full max-w-4xl px-4 py-8">
      <PageHeader
        title="Tâches planifiées"
        description="Des demandes que l’agent traite pour vous, les jours et à l’heure choisis : chacune part dans une nouvelle conversation, avec vos droits du moment. Les actions qui demandent une validation vous attendent dans « Validations »."
      />
      <Loaded state={state}>
        {({ tasks, maxTasks }) => (
          <>
            <section aria-labelledby="new-task" className="mb-6">
              <Card className="flex flex-col gap-3 p-4">
                <h2 id="new-task" className="font-medium">
                  Nouvelle tâche
                </h2>
                {!mayUseAgent ? (
                  <p className="text-sm text-muted">
                    Vous n’avez pas le droit d’utiliser l’agent : vous ne pouvez pas planifier de
                    tâche.
                  </p>
                ) : tasks.length >= maxTasks ? (
                  <p className="text-sm text-muted">
                    Vous avez {maxTasks} tâches planifiées, le maximum : supprimez-en une d’abord.
                  </p>
                ) : (
                  <TaskForm initial={blank()} submitLabel="Planifier" clearOnSave onSave={create} />
                )}
              </Card>
            </section>
            {tasks.length === 0 ? (
              <EmptyState title="Aucune tâche planifiée" />
            ) : (
              <div className="flex flex-col gap-3">
                {tasks.map((task) => (
                  <TaskCard key={task.id} task={task} onChanged={state.reload} />
                ))}
              </div>
            )}
          </>
        )}
      </Loaded>
    </div>
  );
}
