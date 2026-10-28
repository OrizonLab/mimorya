'use client';

import { useEffect, useState } from 'react';
import { Badge, Button, Card, ErrorNote, Loaded } from '@/components/ui';
import {
  api,
  useAction,
  useApiData,
  type AccountLinkStart,
  type AccountLinkView,
  type AccountLinks,
} from '@/lib/api';
import { formatDate } from '@/lib/format';
import { ACCOUNT_PERMISSIONS, ACCOUNT_PROVIDERS } from '@/lib/labels';

const LINKS = '/account/links';

/**
 * What the return from the provider says (ADR-116), from the page's
 * address: the same for each provider, which the address does not name
 * (ADR-124).
 */
const OUTCOMES = {
  ok: {
    failed: false,
    text: 'Votre compte est relié : l’agent y agit avec vos propres droits, selon ce que vous avez accordé et ce que l’application permet, et n’y envoie rien sans validation.',
  },
  'autre-compte': {
    failed: true,
    text: 'Le compte choisi n’est pas celui de votre adresse : rien n’a été relié. Recommencez en vous connectant avec l’adresse de votre compte.',
  },
  // Also a personal Google account, not a Workspace one (ADR-121).
  echec: {
    failed: true,
    text: 'La liaison a échoué : rien n’a été relié. Réessayez avec le compte de votre adresse (chez Google, votre compte Workspace de l’entreprise, non un compte personnel) ; si l’échec persiste, votre administrateur peut vérifier l’application déclarée.',
  },
} as const;

type Outcome = keyof typeof OUTCOMES;

function isOutcome(value: string | null): value is Outcome {
  return value !== null && Object.hasOwn(OUTCOMES, value);
}

/**
 * What these permissions, as the server names them, let the agent do, in
 * words (ADR-120); another is shown by its name.
 */
function inWords(scopes: string[]): string[] {
  return scopes.map(
    (scope) => (Object.hasOwn(ACCOUNT_PERMISSIONS, scope) && ACCOUNT_PERMISSIONS[scope]) || scope,
  );
}

/** Who asks the person's consent for a link of this type, and its product. */
function providerOf(type: string) {
  return (
    (Object.hasOwn(ACCOUNT_PROVIDERS, type) && ACCOUNT_PROVIDERS[type]) || {
      provider: 'le fournisseur',
      product: type,
    }
  );
}

/**
 * The accounts the signed-in person links at their company's providers
 * (ADR-116, ADR-118, ADR-124): their Microsoft 365 or Google Workspace
 * mailbox and calendar, which the agent then reads with their own rights.
 * Shown once the company declared its application, or while they keep a
 * link there.
 */
export function LinkedAccounts() {
  const state = useApiData<AccountLinks>(LINKS);
  const [outcome, setOutcome] = useState<Outcome | null>(null);

  useEffect(() => {
    const said = new URLSearchParams(window.location.search).get('liaison');
    if (!isOutcome(said)) return;
    setOutcome(said);
    // Said once: a reload of the page does not say it again.
    window.history.replaceState(null, '', window.location.pathname);
  }, []);

  return (
    <Loaded state={state}>
      {(data) =>
        data.connectors.length === 0 && !outcome ? null : (
          <Card className="flex max-w-2xl flex-col gap-3 p-4">
            <h2 className="font-medium">Comptes reliés</h2>
            {outcome &&
              (OUTCOMES[outcome].failed ? (
                <ErrorNote message={OUTCOMES[outcome].text} />
              ) : (
                <p role="status" className="text-sm text-ok">
                  {OUTCOMES[outcome].text}
                </p>
              ))}
            {data.connectors.map((connector) => (
              <LinkRow
                key={connector.connectorId}
                connector={connector}
                onChanged={() => {
                  setOutcome(null);
                  state.reload();
                }}
              />
            ))}
          </Card>
        )
      }
    </Loaded>
  );
}

function LinkRow({ connector, onChanged }: { connector: AccountLinkView; onChanged: () => void }) {
  const action = useAction();
  const linked = connector.linked;
  const path = `${LINKS}/${connector.connectorId}`;
  // Asked since the person linked: granted by linking again (ADR-120).
  const toGrant = connector.available && linked ? linked.toGrant : [];
  const { provider, product } = providerOf(connector.type);

  async function link() {
    const started = await action.run(() => api.post<AccountLinkStart>(path));
    // To the provider, which sends the browser back to « Mon compte ».
    if (started) window.location.assign(started.redirectUrl);
  }

  async function unlink() {
    const question = `Retirer la liaison de votre compte ${product} ? L’agent ne pourra plus y lire ni y écrire. L’accord donné à l’application reste chez ${provider} : retirez-le aussi dans votre compte ${provider}.`;
    if (!window.confirm(question)) return;
    if (await action.run(() => api.delete(path))) onChanged();
  }

  return (
    <section aria-label={connector.name} className="flex flex-col gap-2 border-t border-line pt-3">
      <div className="flex flex-wrap items-center gap-2">
        <p className="flex-1 text-sm font-medium">{connector.name}</p>
        <Badge tone={linked ? 'ok' : 'neutral'}>{linked ? 'Relié' : 'Non relié'}</Badge>
      </div>
      {linked ? (
        <p className="text-sm text-muted">
          Relié à {linked.address} le {formatDate(linked.linkedAt)}.
          {/* What the tools may do now, as the server says it (ADR-120). */}
          {linked.can.length > 0 && ` L’agent peut : ${inWords(linked.can).join(', ')}.`}
        </p>
      ) : (
        <p className="text-sm text-muted">
          Reliez votre compte pour que l’agent lise vos e-mails et votre agenda
          {connector.writeTools
            ? ', et envoie des e-mails ou crée des rendez-vous en votre nom, chaque fois après validation'
            : ''}
          , avec vos propres droits : {provider} vous demande votre accord.
        </p>
      )}
      {toGrant.length > 0 && (
        <p className="text-sm">
          Depuis votre liaison, l’application demande aussi votre accord pour :{' '}
          {inWords(toGrant).join(', ')}. Reliez de nouveau votre compte pour l’accorder ; rien n’est
          envoyé ni créé sans validation.
        </p>
      )}
      {linked && !connector.available && (
        <p className="text-sm text-muted">
          L’application est désactivée, ou vous n’utilisez plus les connecteurs : l’agent n’y lit
          pas. Vous pouvez retirer la liaison.
        </p>
      )}
      {action.error && <ErrorNote message={action.error} />}
      <div className="flex flex-wrap gap-2">
        {connector.available && (
          <Button
            variant={linked && toGrant.length === 0 ? 'secondary' : 'primary'}
            loading={action.pending}
            onClick={link}
          >
            {linked ? 'Relier de nouveau' : 'Relier mon compte'}
          </Button>
        )}
        {linked && (
          <Button variant="danger" disabled={action.pending} onClick={unlink}>
            Retirer la liaison
          </Button>
        )}
      </div>
    </section>
  );
}
