'use client';

import { useState, type FormEvent } from 'react';
import { Button, ErrorNote, Field, Input } from '@/components/ui';
import {
  api,
  useAction,
  type GatewayUpdate,
  type GatewayView,
  type NewTeamsGateway,
} from '@/lib/api';
import { GUID_PATTERN } from '@/lib/input-patterns';

/** A client secret, as Entra gives them and the server checks them. */
const CLIENT_SECRET = '[\\x21-\\x7E]{16,256}';

/**
 * Declares the company's Teams bot (ADR-138, ADR-139), its client secret
 * tried with Microsoft Entra before anything is kept, or changes its name
 * and its client secret (of the same application): left blank, the secret
 * is kept. The application and the directory never change.
 */
export function TeamsGatewayForm({
  gateway,
  onDone,
}: {
  gateway?: GatewayView;
  onDone: (changed: boolean) => void;
}) {
  const [name, setName] = useState(gateway?.name ?? 'Teams');
  const [appId, setAppId] = useState('');
  const [directoryId, setDirectoryId] = useState('');
  const [secret, setSecret] = useState('');
  const action = useAction();

  async function submit(event: FormEvent) {
    event.preventDefault();
    const saved = await action.run(() =>
      gateway
        ? api.patch(`/admin/gateways/${gateway.id}`, {
            name,
            // Left out when blank: the stored one is kept.
            clientSecret: secret.trim() || undefined,
          } satisfies GatewayUpdate)
        : api.post('/admin/gateways/teams', {
            name,
            appId: appId.trim(),
            directoryId: directoryId.trim(),
            clientSecret: secret.trim(),
          } satisfies NewTeamsGateway),
    );
    if (saved) onDone(true);
  }

  return (
    <form aria-label="Bot Teams" onSubmit={submit} className="flex flex-col gap-3">
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
        {!gateway && (
          <>
            <Field label="ID d’application (Microsoft App ID)">
              <Input
                required
                autoComplete="off"
                pattern={GUID_PATTERN}
                title="L’identifiant de l’application du bot : un GUID"
                value={appId}
                onChange={(e) => setAppId(e.target.value)}
              />
            </Field>
            <Field label="ID de l’annuaire (locataire Microsoft Entra)">
              <Input
                required
                autoComplete="off"
                pattern={GUID_PATTERN}
                title="L’identifiant de l’annuaire de l’entreprise : un GUID"
                value={directoryId}
                onChange={(e) => setDirectoryId(e.target.value)}
              />
            </Field>
          </>
        )}
        <Field
          label={`Secret client${gateway ? ' (laisser vide pour conserver celui enregistré)' : ''}`}
        >
          <Input
            required={!gateway}
            type="password"
            autoComplete="off"
            pattern={CLIENT_SECRET}
            title="La valeur du secret client de l’application : de 16 à 256 caractères, sans espace"
            maxLength={256}
            value={secret}
            onChange={(e) => setSecret(e.target.value)}
          />
        </Field>
      </div>
      <p className="text-xs text-muted">
        Le bot est le bot Azure (« Azure Bot ») que l’entreprise crée pour un seul locataire, dans
        l’annuaire Microsoft Entra de ses collaborateurs : son application donne l’ID d’application,
        et un secret client créé dans « Certificats et secrets ». Le secret est essayé auprès de
        Microsoft Entra avant d’être gardé, chiffré sur le serveur et jamais réaffiché ; un nouveau
        secret doit être celui de la même application. Une fois le bot déclaré, sa carte donne le
        point de terminaison de messagerie à saisir dans le bot Azure, dont le canal Microsoft Teams
        est à activer ; l’application du bot est ensuite à publier dans Teams pour l’entreprise.
        Chacun relie son compte Teams en écrivant au bot : il lui répond par un lien à ouvrir dans
        l’application, connecté, et Teams doit donner pour son compte l’adresse de son compte. Une
        fois relié, il écrit au bot : l’agent lui répond dans sa conversation avec le bot, avec ses
        droits, une conversation à la fois (/nouvelle en ouvre une autre). Il y valide ou refuse,
        avec les boutons d’une carte, ses propres actions sans effet hors de l’entreprise et
        réversibles ; les autres se valident dans l’application. Un responsable relié, ou qui le
        remplace, y est aussi prévenu des demandes qui attendent sa validation, et les valide de
        même, dans la conversation où le bot l’a vu en dernier.
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
