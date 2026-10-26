import { Badge, ErrorNote } from '@/components/ui';
import type { LlmSettingsView, LlmTestResult } from '@/lib/api';
import { formatDate } from '@/lib/format';
import { LLM_PROVIDERS } from '@/lib/labels';

export const providerLabel = (provider: string) => LLM_PROVIDERS[provider] ?? provider;

/**
 * A provider the company chose, for its conversations or for reviewing
 * their answers (ADR-108): where, whether its key is stored, whether its
 * costs are estimated, and why it cannot be used, `unusable` saying what
 * follows.
 */
export function ModelSummary({
  title,
  settings,
  unusable,
}: {
  title: string;
  settings: LlmSettingsView;
  unusable: string;
}) {
  return (
    <div className="flex flex-col gap-2 text-sm">
      <p>
        {title} : {providerLabel(settings.provider)} · <code>{settings.model}</code>
        {settings.baseUrl && <> · {settings.baseUrl}</>}
      </p>
      <div className="flex flex-wrap gap-2">
        <Badge tone={settings.keyStored ? 'ok' : 'neutral'}>
          {settings.keyStored ? 'Clé enregistrée' : 'Sans clé'}
        </Badge>
        {!settings.priced && (
          <Badge tone="warn">Coût non estimé : modèle absent de la grille</Badge>
        )}
        {!settings.usable && <Badge tone="danger">Inutilisable</Badge>}
      </div>
      {settings.problem && <ErrorNote message={`${unusable} : ${settings.problem}.`} />}
      <p className="text-xs text-muted">Modifié le {formatDate(settings.updatedAt)}.</p>
    </div>
  );
}

/** What a connection test answered. */
export function TestOutcome({ tested }: { tested: LlmTestResult }) {
  return (
    <p className={tested.ok ? 'text-sm text-ok' : 'text-sm text-danger'}>
      {tested.ok
        ? `Connexion réussie : ${tested.model} a répondu.`
        : `Échec de la connexion : ${tested.error ?? 'erreur inconnue'}`}
    </p>
  );
}
