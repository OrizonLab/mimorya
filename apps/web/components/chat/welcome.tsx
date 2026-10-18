import { ErrorNote } from '@/components/ui';
import { useMe } from '@/lib/me-context';

const SUGGESTIONS = [
  'Analyse les comptes rendus des interventions de cette semaine, identifie les interventions non terminées, prépare une synthèse et propose un planning de suivi pour mon équipe.',
  'Quelles interventions sont encore en cours et pourquoi ?',
  'Retiens que je préfère des synthèses courtes, sous forme de tableau.',
];

/** Greeting and suggested requests shown before the first message. */
export function Welcome({ onPick }: { onPick: (text: string) => void }) {
  const me = useMe();
  return (
    <div className="py-8">
      <h1 className="text-2xl font-semibold tracking-tight">
        Bonjour {me.user.displayName.split(' ')[0]},
      </h1>
      <p className="mt-2 text-muted">
        Confiez-moi une analyse, une synthèse ou une tâche. J’agis avec vos droits et je vous
        demande votre accord avant toute action sensible.
      </p>
      {!me.assistant.available && (
        <div className="mt-4">
          <ErrorNote message="Aucun modèle d’IA n’est configuré sur ce serveur : les conversations sont désactivées." />
        </div>
      )}
      <div className="mt-6 grid gap-2">
        {SUGGESTIONS.map((s) => (
          <button
            key={s}
            type="button"
            onClick={() => onPick(s)}
            className="rounded-xl border border-line bg-surface px-4 py-3 text-left text-sm hover:border-accent"
          >
            {s}
          </button>
        ))}
      </div>
    </div>
  );
}
