'use client';

import { useState, type FormEvent } from 'react';
import { Button, ErrorNote, Field, Input } from '@/components/ui';
import {
  api,
  useAction,
  type GatewayUpdate,
  type GatewayView,
  type NewTelegramGateway,
} from '@/lib/api';

/** A bot's token, as BotFather gives them and the server checks them. */
const BOT_TOKEN = '[0-9]{5,20}:[A-Za-z0-9_\\-]{30,100}';

/**
 * Declares the company's Telegram bot (ADR-133, ADR-134), its token tried
 * with Telegram before anything is kept and the bot told where to send its
 * updates, or changes its name and its token (of the same bot): left blank,
 * the token is kept.
 */
export function TelegramGatewayForm({
  gateway,
  onDone,
}: {
  gateway?: GatewayView;
  onDone: (changed: boolean) => void;
}) {
  const [name, setName] = useState(gateway?.name ?? 'Telegram');
  const [token, setToken] = useState('');
  const action = useAction();

  async function submit(event: FormEvent) {
    event.preventDefault();
    const saved = await action.run(() =>
      gateway
        ? api.patch(`/admin/gateways/${gateway.id}`, {
            name,
            // Left out when blank: the stored one is kept.
            botToken: token.trim() || undefined,
          } satisfies GatewayUpdate)
        : api.post('/admin/gateways/telegram', {
            name,
            botToken: token.trim(),
          } satisfies NewTelegramGateway),
    );
    if (saved) onDone(true);
  }

  return (
    <form aria-label="Bot Telegram" onSubmit={submit} className="flex flex-col gap-3">
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
        <Field
          label={`Jeton du bot${gateway ? ' (laisser vide pour conserver celui enregistré)' : ''}`}
        >
          <Input
            required={!gateway}
            type="password"
            autoComplete="off"
            pattern={BOT_TOKEN}
            title="Le jeton que BotFather donne au bot : des chiffres, deux-points, puis lettres et chiffres"
            maxLength={121}
            value={token}
            onChange={(e) => setToken(e.target.value)}
          />
        </Field>
      </div>
      <p className="text-xs text-muted">
        Le bot est celui que l’entreprise crée avec <code>@BotFather</code> dans Telegram (
        <code>/newbot</code>), qui lui donne son jeton. Le jeton est essayé auprès de Telegram avant
        d’être gardé ; la plateforme dit alors à Telegram où envoyer les messages du bot, avec un
        secret qu’elle seule connaît, et l’adresse de l’application doit être publique, en HTTPS. Le
        jeton et le secret sont chiffrés sur le serveur et jamais réaffichés ; un nouveau jeton doit
        être celui du même bot. Chacun relie ensuite son compte Telegram depuis « Mon compte » : un
        lien ouvre le bot, qui lui dit à quel compte il le relie ; il l’accepte dans Telegram, puis
        le confirme dans l’application. Une fois relié, il écrit au bot : l’agent lui répond dans sa
        conversation privée avec lui, avec ses droits, et les validations se donnent dans
        l’application.
      </p>
      {action.error && <ErrorNote message={action.error} />}
      <div className="flex gap-2">
        <Button type="submit" loading={action.pending}>
          {gateway ? 'Enregistrer' : 'Déclarer le bot'}
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
