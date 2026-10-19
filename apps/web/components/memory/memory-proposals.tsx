import { Button, Card } from '@/components/ui';
import type { MemoryItem, ReviewDecision } from '@/lib/api';

/** Knowledge proposed by the agent for a shared space the user is responsible for. */
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
        Connaissances proposées par l’agent pour un espace partagé dont vous êtes responsable.
      </p>
      <ul className="mt-3 flex flex-col gap-3">
        {items.map((m) => (
          <li key={m.id} className="rounded-lg border border-line p-3">
            <p className="text-sm font-medium">{m.title}</p>
            <p className="mt-1 text-sm text-muted">{m.content}</p>
            <div className="mt-2 flex gap-2">
              <Button onClick={() => onReview(m.id, 'approve')}>Valider</Button>
              <Button variant="secondary" onClick={() => onReview(m.id, 'reject')}>
                Rejeter
              </Button>
            </div>
          </li>
        ))}
      </ul>
    </Card>
  );
}
