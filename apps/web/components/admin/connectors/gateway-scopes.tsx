import type { GatewayView } from '@/lib/api';
import { formatDate } from '@/lib/format';

/**
 * What Slack said of the permissions of a gateway's token (ADR-132): those
 * it lacks, without which the gateway cannot answer, and those it has beyond
 * what is asked for, which make the stored token stronger than the platform
 * needs; or that Slack has not said them, to check them.
 */
export function GatewayScopes({ gateway }: { gateway: GatewayView }) {
  const { missingScopes: missing, extraScopes: extra, scopesCheckedAt: checkedAt } = gateway;
  if (missing === null) {
    return (
      <p className="text-sm text-warn">
        Permissions du jeton non vérifiées
        {checkedAt ? ` : Slack ne les a pas dites le ${formatDate(checkedAt)}` : ''}. Vérifiez-les
        auprès de Slack.
      </p>
    );
  }
  return (
    <div className="flex flex-col gap-1 text-sm">
      {missing.length > 0 ? (
        <p className="text-danger">
          Permissions manquantes : {missing.join(', ')}. Sans elles, la passerelle ne peut pas
          répondre : ajoutez-les dans « OAuth &amp; Permissions », réinstallez l’application, puis
          vérifiez de nouveau.
        </p>
      ) : (
        <p className="text-ok">
          Toutes les permissions demandées sont accordées (vérifié le {formatDate(checkedAt)}).
        </p>
      )}
      {extra !== null && extra.length > 0 && (
        <p className="text-warn">
          Permissions en plus de celles demandées : {extra.join(', ')}. La plateforme n’en a pas
          besoin : retirez-les si rien d’autre ne s’en sert.
        </p>
      )}
    </div>
  );
}
