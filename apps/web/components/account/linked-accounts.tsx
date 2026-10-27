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
import { MICROSOFT365_PERMISSIONS } from '@/lib/labels';

const LINKS = '/account/links';

/** What the return from Microsoft says (ADR-116), from the page's address. */
const OUTCOMES = {
  ok: {
    failed: false,
    text: 'Votre compte Microsoft 365 est relié : l’agent peut y lire vos e-mails et votre agenda, avec vos propres droits.',
  },
  'autre-compte': {
    failed: true,
    text: 'Le compte choisi chez Microsoft n’est pas celui de votre adresse : rien n’a été relié. Recommencez en vous connectant chez Microsoft avec l’adresse de votre compte.',
  },
  echec: {
    failed: true,
    text: 'La liaison a échoué : rien n’a été relié. Réessayez ; si l’échec persiste, votre administrateur peut vérifier l’application déclarée.',
  },
} as const;

type Outcome = keyof typeof OUTCOMES;

function isOutcome(value: string | null): value is Outcome {
  return value !== null && Object.hasOwn(OUTCOMES, value);
}

/**
 * What these permissions let the agent do, in words: those it knows, each
 * once (Microsoft may name a permission by its address, ADR-120).
 */
function inWords(scopes: string[]): string[] {
  const said = scopes.flatMap((scope) => {
    const name = (scope.split('/').pop() ?? scope).toLowerCase();
    const known = Object.entries(MICROSOFT365_PERMISSIONS).find(
      ([permission]) => permission.toLowerCase() === name,
    );
    return known ? [known[1]] : [];
  });
  return [...new Set(said)];
}

/**
 * The accounts the signed-in person links at their company's providers
 * (ADR-116, ADR-118): their Microsoft 365 mailbox and calendar, which the
 * agent then reads with their own rights. Shown once the company declared
 * its application, or while they keep a link there.
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

  async function link() {
    const started = await action.run(() => api.post<AccountLinkStart>(path));
    // To Microsoft, which sends the browser back to « Mon compte ».
    if (started) window.location.assign(started.redirectUrl);
  }

  async function unlink() {
    const question =
      'Retirer la liaison de votre compte Microsoft 365 ? L’agent ne pourra plus y lire. L’accord donné à l’application reste chez Microsoft : retirez-le aussi dans votre compte Microsoft.';
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
          Relié à {linked.address} le {formatDate(linked.linkedAt)}. L’agent peut :{' '}
          {inWords(linked.scopes).join(', ') || '—'}.
        </p>
      ) : (
        <p className="text-sm text-muted">
          Reliez votre compte pour que l’agent lise vos e-mails et votre agenda
          {connector.writeTools
            ? ', et envoie des e-mails ou crée des rendez-vous en votre nom, chaque fois après validation'
            : ''}
          , avec vos propres droits : Microsoft vous demande votre accord.
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
