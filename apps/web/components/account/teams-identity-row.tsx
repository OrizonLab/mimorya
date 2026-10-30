'use client';

import { Badge, Button, ErrorNote } from '@/components/ui';
import { api, useAction, type AccountGatewayView } from '@/lib/api';
import { formatDate } from '@/lib/format';

/**
 * The signed-in person's Teams account on the company's Teams bot
 * (ADR-138, ADR-139): linked from the link the bot sent them in Teams,
 * and removed here. The agent is not talked with in Teams yet: the bot
 * sends a linked person to the application.
 */
export function TeamsIdentityRow({
  gateway,
  onChanged,
}: {
  gateway: AccountGatewayView;
  onChanged: () => void;
}) {
  const action = useAction();
  const linked = gateway.linked;

  async function unlink() {
    const question = `Retirer la liaison de votre compte Teams au bot « ${gateway.name} » ? Vous pourrez le relier de nouveau depuis le lien que le bot vous enverra.`;
    if (!window.confirm(question)) return;
    if (await action.run(() => api.delete(`/account/gateways/${gateway.gatewayId}`))) onChanged();
  }

  return (
    <section aria-label={gateway.name} className="flex flex-col gap-2 border-t border-line pt-3">
      <div className="flex flex-wrap items-center gap-2">
        <p className="flex-1 text-sm font-medium">{gateway.name}</p>
        <Badge>Teams</Badge>
        <Badge tone={linked ? 'ok' : 'neutral'}>{linked ? 'Relié' : 'Non relié'}</Badge>
      </div>
      {linked ? (
        <p className="text-sm text-muted">
          Votre compte Teams ({linked.address}) est relié depuis le {formatDate(linked.linkedAt)}.
          Les échanges avec l’agent dans Teams ne sont pas encore ouverts : le bot vous renvoie vers
          l’application.
        </p>
      ) : gateway.available ? (
        <p className="text-sm text-muted">
          Pour relier votre compte Teams, écrivez au bot de l’entreprise dans Teams : il vous répond
          par un lien à ouvrir ici, connecté. Teams doit donner pour votre compte l’adresse de votre
          compte.
        </p>
      ) : (
        <p className="text-sm text-muted">La passerelle est indisponible pour l’instant.</p>
      )}
      {linked && !gateway.available && (
        <p className="text-sm text-muted">
          La passerelle est désactivée, ou vous n’utilisez plus l’agent. Vous pouvez retirer la
          liaison.
        </p>
      )}
      {action.error && <ErrorNote message={action.error} />}
      {linked && (
        <div>
          <Button variant="danger" loading={action.pending} onClick={unlink}>
            Retirer la liaison
          </Button>
        </div>
      )}
    </section>
  );
}
