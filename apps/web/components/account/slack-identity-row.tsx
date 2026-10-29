'use client';

import { Badge, Button, ErrorNote } from '@/components/ui';
import { api, useAction, type AccountGatewayView } from '@/lib/api';
import { formatDate } from '@/lib/format';

/**
 * The signed-in person's Slack user on the company's Slack application
 * (ADR-125, ADR-126): linked from the link the application sent them in
 * Slack, and removed here.
 */
export function SlackIdentityRow({
  gateway,
  onChanged,
}: {
  gateway: AccountGatewayView;
  onChanged: () => void;
}) {
  const action = useAction();
  const linked = gateway.linked;

  async function unlink() {
    const question = `Retirer la liaison de votre compte Slack de l’espace « ${gateway.workspaceName} » ? Vous pourrez le relier de nouveau depuis le lien que l’application vous enverra.`;
    if (!window.confirm(question)) return;
    if (await action.run(() => api.delete(`/account/gateways/${gateway.gatewayId}`))) onChanged();
  }

  return (
    <section aria-label={gateway.name} className="flex flex-col gap-2 border-t border-line pt-3">
      <div className="flex flex-wrap items-center gap-2">
        <p className="flex-1 text-sm font-medium">{gateway.name}</p>
        <Badge tone={linked ? 'ok' : 'neutral'}>{linked ? 'Relié' : 'Non relié'}</Badge>
      </div>
      {linked ? (
        <p className="text-sm text-muted">
          Votre compte Slack <code>{linked.externalId}</code> ({linked.address}) de l’espace{' '}
          {`« ${gateway.workspaceName} »`} est relié depuis le {formatDate(linked.linkedAt)}. Vous y
          parlez à l’agent, qui vous y annonce les demandes attendant votre validation.
        </p>
      ) : gateway.available ? (
        <p className="text-sm text-muted">
          Pour relier votre compte Slack, écrivez à l’application de l’entreprise dans l’espace{' '}
          {`« ${gateway.workspaceName} »`} : elle vous répond par un lien à ouvrir ici, connecté.
          Slack doit donner pour votre compte l’adresse de votre compte.
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
