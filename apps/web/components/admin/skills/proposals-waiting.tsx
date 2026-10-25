import type { SkillView } from '@/lib/api';

/** Which skills have proposals of the agent's awaiting a decision (ADR-100). */
export function ProposalsWaiting({ skills }: { skills: SkillView[] }) {
  const waiting = skills.filter((s) => s.proposals > 0);
  if (waiting.length === 0) return null;
  const count = waiting.reduce((total, s) => total + s.proposals, 0);
  return (
    <p role="status" className="rounded-lg bg-accent-soft px-3 py-2 text-sm">
      {count === 1
        ? 'Une proposition de l’agent attend votre décision'
        : `${count} propositions de l’agent attendent votre décision`}{' '}
      : {waiting.map((s) => `« ${s.name} »`).join(', ')}.
    </p>
  );
}
