import { Badge, Button, Card } from '@/components/ui';
import type { MemoryItem, ReviewDecision } from '@/lib/api';
import { MEMORY_KINDS, MEMORY_SPACES } from './spaces';

/** The space a proposal would join, as the page names it. */
function destination(m: MemoryItem): string {
  if (m.scope === 'org_unit' && m.orgUnit) return m.orgUnit;
  return MEMORY_SPACES.find((space) => space.scope === m.scope)?.title ?? m.scope;
}

/** Knowledge proposed by the agent: a lesson for the user's own memory
 * (ADR-076), or for a shared space the user is responsible for. */
export function MemoryProposals({
  items,
  onReview,
}: {
  items: MemoryItem[];
  onReview: (id: string, decision: ReviewDecision) => void;
}) {
  return (
    <Card className="mb-6 p-4">
      <h2 className="text-sm font-semibold">Propositions à valider</h2>
      <p className="text-xs text-muted">
        Connaissances proposées par l’agent pour votre mémoire, tirées de vos conversations, ou pour
        un espace partagé dont vous êtes responsable. Rien n’est utilisé avant d’être validé.
      </p>
      <ul className="mt-3 flex flex-col gap-3">
        {items.map((m) => (
          <li key={m.id} className="rounded-lg border border-line p-3">
            <div className="flex flex-wrap items-center gap-2">
              <p className="text-sm font-medium">{m.title}</p>
              <Badge>{MEMORY_KINDS[m.kind] ?? m.kind}</Badge>
              <Badge tone="accent">{destination(m)}</Badge>
            </div>
            <p className="mt-1 text-sm text-muted">{m.content}</p>
            <div className="mt-2 flex gap-2">
              <Button
                aria-label={`Valider « ${m.title} »`}
                onClick={() => onReview(m.id, 'approve')}
              >
                Valider
              </Button>
              <Button
                variant="secondary"
                aria-label={`Rejeter « ${m.title} »`}
                onClick={() => onReview(m.id, 'reject')}
              >
                Rejeter
              </Button>
            </div>
          </li>
        ))}
      </ul>
    </Card>
  );
}
