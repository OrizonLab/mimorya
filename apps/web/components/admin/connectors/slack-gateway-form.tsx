'use client';

import { useState, type FormEvent } from 'react';
import { Button, ErrorNote, Field, Input } from '@/components/ui';
import {
  api,
  useAction,
  type GatewayUpdate,
  type GatewayView,
  type NewSlackGateway,
} from '@/lib/api';

/** A bot's token, as Slack issues them and the server checks them. */
const BOT_TOKEN = 'xoxb-[A-Za-z0-9\\-]{10,250}';
/** A signing secret, as Slack shows it. */
const SIGNING_SECRET = '[A-Za-z0-9]{16,128}';

/**
 * Declares the company's Slack application (ADR-125, ADR-126), its token
 * tried with Slack before anything is kept, or changes its name, its token
 * (of the same workspace) and its signing secret: left blank, they are
 * kept.
 */
export function SlackGatewayForm({
  gateway,
  onDone,
}: {
  gateway?: GatewayView;
  onDone: (changed: boolean) => void;
}) {
  const [name, setName] = useState(gateway?.name ?? 'Slack');
  const [token, setToken] = useState('');
  const [secret, setSecret] = useState('');
  const action = useAction();
  const kept = gateway ? ' (laisser vide pour conserver celui enregistré)' : '';

  async function submit(event: FormEvent) {
    event.preventDefault();
    const saved = await action.run(() =>
      gateway
        ? api.patch(`/admin/gateways/${gateway.id}`, {
            name,
            // Left out when blank: the stored ones are kept.
            botToken: token.trim() || undefined,
            signingSecret: secret.trim() || undefined,
          } satisfies GatewayUpdate)
        : api.post('/admin/gateways/slack', {
            name,
            botToken: token.trim(),
            signingSecret: secret.trim(),
          } satisfies NewSlackGateway),
    );
    if (saved) onDone(true);
  }

  return (
    <form aria-label="Application Slack" onSubmit={submit} className="flex flex-col gap-3">
      <div className="grid gap-3 sm:grid-cols-2">
        <Field label="Nom">
          <Input
            required
            minLength={2}
            maxLength={80}
            value={name}
            onChange={(e) => setName(e.target.value)}
          />
        </Field>
        <Field label={`Jeton du bot${kept}`}>
          <Input
            required={!gateway}
            type="password"
            autoComplete="off"
            pattern={BOT_TOKEN}
            title="Le « Bot User OAuth Token » de l’application, commençant par xoxb-"
            maxLength={260}
            value={token}
            onChange={(e) => setToken(e.target.value)}
          />
        </Field>
        <Field label={`Secret de signature${kept}`}>
          <Input
            required={!gateway}
            type="password"
            autoComplete="off"
            pattern={SIGNING_SECRET}
            title="Le « Signing Secret » de l’application : lettres et chiffres"
            maxLength={128}
            value={secret}
            onChange={(e) => setSecret(e.target.value)}
          />
        </Field>
      </div>
      <p className="text-xs text-muted">
        L’application est celle que l’entreprise crée sur api.slack.com et installe dans son espace
        de travail : permissions du bot <code>chat:write</code>, <code>im:history</code>,{' '}
        <code>users:read</code> et <code>users:read.email</code>, rien d’autre ; onglet « Messages »
        de « App Home » ouvert, pour que chacun lui écrive. Une fois la passerelle déclarée,
        abonnez-la à l’événement <code>message.im</code> à l’adresse que montre sa carte, et donnez
        la même adresse à « Interactivity &amp; Shortcuts » pour les boutons de validation. Le jeton
        est essayé auprès de Slack avant d’être gardé ; le jeton et le secret sont chiffrés sur le
        serveur et jamais réaffichés. Chacun relie ensuite son compte Slack depuis le lien que
        l’application lui envoie, Slack prouvant que c’est le sien ; il parle alors à l’agent dans
        Slack, et y valide ou refuse les actions qui attendent sa validation.
      </p>
      {action.error && <ErrorNote message={action.error} />}
      <div className="flex gap-2">
        <Button type="submit" loading={action.pending}>
          {gateway ? 'Enregistrer' : 'Déclarer l’application'}
        </Button>
        <Button
          type="button"
          variant="ghost"
          // Closed before its change is answered, it could be reopened from the
          // former resource, and that change overwritten.
          disabled={action.pending}
          onClick={() => onDone(false)}
        >
          Annuler
        </Button>
      </div>
    </form>
  );
}
