'use client';

import { useEffect, useState } from 'react';
import { Badge, Button, Card, ErrorNote, Loaded } from '@/components/ui';
import {
  api,
  useAction,
  useApiData,
  type OAuthAuthorizationStart,
  type PersonAuthorizationView,
  type PersonAuthorizations,
} from '@/lib/api';
import { authorizationSummary } from '@/lib/format';

const SERVERS = '/account/connector-authorizations';

/** What the return from the authorization server says (ADR-174, ADR-175). */
const OUTCOMES = {
  ok: {
    failed: false,
    text: 'Serveur autorisé : l’agent l’utilise désormais pour vous, avec votre propre autorisation.',
  },
  echec: {
    failed: true,
    text: 'L’autorisation a échoué : rien n’a été gardé. Réessayez ; si l’échec persiste, votre administrateur peut vérifier la déclaration du serveur.',
  },
  'autre-compte': {
    failed: true,
    text: 'Le compte choisi chez le serveur d’autorisation n’est pas le vôtre : rien n’a été gardé. Autorisez de nouveau en vous connectant avec votre propre compte.',
  },
} as const;

type Outcome = keyof typeof OUTCOMES;

function isOutcome(value: string | null): value is Outcome {
  return value !== null && Object.hasOwn(OUTCOMES, value);
}

/**
 * The MCP servers the company has each person authorize for themself
 * (ADR-174): the agent uses them for the signed-in person with their own
 * authorization, never another's. Authorized at the server's authorization
 * server, where the browser goes and comes back here; its state, its
 * renewal and why the last one failed; withdrawn. Shown once such a server
 * is in their reach, or while they keep an authorization of one. A server
 * whose authorization server proves the account is theirs says so; the
 * others are not checked (ADR-175).
 */
export function ServerAuthorizations() {
  const state = useApiData<PersonAuthorizations>(SERVERS);
  const [outcome, setOutcome] = useState<Outcome | null>(null);

  useEffect(() => {
    const said = new URLSearchParams(window.location.search).get('serveur');
    if (!isOutcome(said)) return;
    setOutcome(said);
    // Said once: a reload of the page does not say it again.
    window.history.replaceState(null, '', window.location.pathname);
  }, []);

  return (
    <Loaded state={state}>
      {(data) =>
        data.servers.length === 0 && !outcome ? null : (
          <Card className="flex max-w-2xl flex-col gap-3 p-4">
            <h2 className="font-medium">Serveurs que vous autorisez</h2>
            <p className="text-sm text-muted">
              L’agent utilise ces serveurs pour vous avec votre propre autorisation. Connectez-vous
              chez leur serveur d’autorisation avec votre propre compte : un serveur marqué « Compte
              vérifié » refuse tout autre compte ; pour les autres, la plateforme ne vérifie pas
              lequel vous choisissez, et un compte partagé avec d’autres peut voir leur accord
              retirer le vôtre.
            </p>
            {outcome &&
              (OUTCOMES[outcome].failed ? (
                <ErrorNote message={OUTCOMES[outcome].text} />
              ) : (
                <p role="status" className="text-sm text-ok">
                  {OUTCOMES[outcome].text}
                </p>
              ))}
            {data.servers.map((server) => (
              <ServerRow
                key={server.connectorId}
                server={server}
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

function ServerRow({
  server,
  onChanged,
}: {
  server: PersonAuthorizationView;
  onChanged: () => void;
}) {
  const action = useAction();
  const authorization = server.authorization;
  const path = `${SERVERS}/${server.connectorId}`;

  async function authorize() {
    const started = await action.run(() => api.post<OAuthAuthorizationStart>(path, {}));
    // To the authorization server, which sends the browser back here.
    if (started) window.location.assign(started.redirectUrl);
  }

  async function withdraw() {
    const question = `Retirer votre autorisation de « ${server.name} » ? L’agent ne l’utilisera plus pour vous ; la plateforme demande à son serveur d’autorisation de révoquer vos jetons, s’il le permet : sinon, retirez aussi l’accord chez lui.`;
    if (!window.confirm(question)) return;
    if (await action.run(() => api.delete(path))) onChanged();
  }

  return (
    <section aria-label={server.name} className="flex flex-col gap-2 border-t border-line pt-3">
      <div className="flex flex-wrap items-center gap-2">
        <p className="flex-1 text-sm font-medium">{server.name}</p>
        {server.verifiesIdentity && <Badge tone="neutral">Compte vérifié</Badge>}
        <Badge tone={authorization ? 'ok' : 'neutral'}>
          {authorization ? 'Autorisé' : 'Non autorisé'}
        </Badge>
      </div>
      {authorization ? (
        <p className="text-sm text-muted">{authorizationSummary(authorization)}</p>
      ) : (
        <p className="text-sm text-muted">
          Autorisez ce serveur pour que l’agent l’utilise pour vous, avec vos propres droits chez
          lui : son serveur d’autorisation vous demande votre accord.
        </p>
      )}
      {authorization?.renewalFailure && (
        <ErrorNote message={`Dernier renouvellement échoué : ${authorization.renewalFailure}`} />
      )}
      {authorization && !server.available && (
        <p className="text-sm text-muted">
          Ce serveur n’est plus disponible pour vous (désactivé, déclaré autrement, ou vous
          n’utilisez plus les connecteurs) : l’agent ne l’utilise pas. Vous pouvez retirer votre
          autorisation.
        </p>
      )}
      {action.error && <ErrorNote message={action.error} />}
      <div className="flex flex-wrap gap-2">
        {server.available && (
          <Button
            variant={authorization ? 'secondary' : 'primary'}
            loading={action.pending}
            onClick={authorize}
          >
            {authorization ? 'Autoriser de nouveau' : 'Autoriser'}
          </Button>
        )}
        {authorization && (
          <Button variant="danger" disabled={action.pending} onClick={withdraw}>
            Retirer mon autorisation
          </Button>
        )}
      </div>
    </section>
  );
}
