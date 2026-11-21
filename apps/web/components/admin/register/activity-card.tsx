import { Card, Cell, DataTable } from '@/components/ui';
import type { RegisterActivity } from '@/lib/api';

/** One processing activity of the register (ADR-217): what for, about
 * whom, which data, who receives them and how long they are kept. */
export function ActivityCard({ activity }: { activity: RegisterActivity }) {
  const titleId = `register-${activity.key}`;
  return (
    <Card className="p-4">
      <section aria-labelledby={titleId} className="flex flex-col gap-3">
        <h3 id={titleId} className="font-medium">
          {activity.name}
        </h3>
        <p className="text-sm">{activity.purpose}</p>
        <div className="grid gap-3 text-sm md:grid-cols-2">
          <Facts title="Personnes concernées" lines={activity.subjects} />
          <Facts title="Données" lines={activity.data} />
        </div>
        {activity.sensitive && (
          <p className="rounded-lg bg-warn-soft px-3 py-2 text-sm text-warn">
            Données sensibles : {activity.sensitive}
          </p>
        )}
        {activity.recipients.length > 0 ? (
          <DataTable
            columns={['Destinataire', 'Reçoit les données pour', 'Localisation', 'Choisi par']}
          >
            {activity.recipients.map((recipient) => (
              <tr key={`${recipient.name} ${recipient.role}`}>
                <Cell className="font-medium">{recipient.name}</Cell>
                <Cell>{recipient.role}</Cell>
                <Cell>{recipient.placeLabel}</Cell>
                <Cell>{recipient.chosenByLabel}</Cell>
              </tr>
            ))}
          </DataTable>
        ) : (
          <p className="text-sm text-muted">Aucun destinataire hors de la plateforme.</p>
        )}
        <Facts title="Conservation" lines={activity.retention} />
        {activity.notes.length > 0 && <Facts title="Remarques" lines={activity.notes} />}
      </section>
    </Card>
  );
}

function Facts({ title, lines }: { title: string; lines: string[] }) {
  return (
    <div className="text-sm">
      <p className="text-xs font-medium text-muted">{title}</p>
      <ul className="list-disc pl-5">
        {lines.map((line) => (
          <li key={line}>{line}</li>
        ))}
      </ul>
    </div>
  );
}
