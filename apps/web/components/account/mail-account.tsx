'use client';

import { useState, type FormEvent } from 'react';
import { Badge, Button, Card, ErrorNote, Field, Input, Loaded } from '@/components/ui';
import {
  api,
  useAction,
  useApiData,
  type AccountMail,
  type MailAccountLink,
  type MailAccountView,
} from '@/lib/api';
import { formatDate } from '@/lib/format';

const MAIL = '/account/mail';

/**
 * The person's account at their company's standard mail (ADR-205): the
 * servers their password goes to, the identifier they linked and when;
 * linking it with their identifier and password, checked there first and
 * sealed for them alone, never shown again; and removing it. Nothing shows
 * where the company declared no standard mail.
 */
export function MailAccount() {
  const state = useApiData<AccountMail>(MAIL);
  return (
    <Loaded state={state}>
      {(data) => data.mail && <MailAccountCard mail={data.mail} onChanged={state.reload} />}
    </Loaded>
  );
}

function MailAccountCard({ mail, onChanged }: { mail: MailAccountView; onChanged: () => void }) {
  const [username, setUsername] = useState(mail.linked?.username ?? '');
  const [password, setPassword] = useState('');
  const [linked, setLinked] = useState(false);
  const action = useAction();

  async function link(event: FormEvent) {
    event.preventDefault();
    setLinked(false);
    const done = await action.run(() =>
      api.put(MAIL, { username, password } satisfies MailAccountLink),
    );
    // The password leaves the page, whatever the answer.
    setPassword('');
    if (done) {
      setLinked(true);
      onChanged();
    }
  }

  async function remove() {
    if (
      !window.confirm(
        `Retirer votre compte de « ${mail.name} » ? Son mot de passe chiffré est effacé : l’agent ne lit plus votre boîte ni votre agenda.`,
      )
    ) {
      return;
    }
    setLinked(false);
    if (await action.run(() => api.delete(MAIL))) onChanged();
  }

  return (
    <Card className="p-4">
      <section aria-label={mail.name} className="flex flex-col gap-3">
        <div className="flex flex-wrap items-center gap-2">
          <h2 className="flex-1 font-medium">{mail.name}</h2>
          <Badge tone={mail.linked ? 'ok' : 'neutral'}>{mail.linked ? 'Relié' : 'Non relié'}</Badge>
        </div>
        {mail.linked && (
          <p className="text-sm">
            Compte <code>{mail.linked.username}</code>, relié le {formatDate(mail.linked.linkedAt)}.
          </p>
        )}
        {linked && (
          <p role="status" className="text-sm text-ok">
            Votre compte est relié : l’agent lit votre boîte avec lui et n’y envoie rien sans
            validation.
          </p>
        )}
        {mail.available ? (
          <form aria-label="Compte de messagerie" onSubmit={link} className="flex flex-col gap-3">
            <div className="grid gap-3 sm:grid-cols-2">
              <Field label="Identifiant de messagerie">
                <Input
                  required
                  maxLength={320}
                  autoComplete="off"
                  value={username}
                  onChange={(e) => setUsername(e.target.value)}
                />
              </Field>
              <Field label="Mot de passe de messagerie">
                <Input
                  required
                  type="password"
                  maxLength={1024}
                  autoComplete="off"
                  value={password}
                  onChange={(e) => setPassword(e.target.value)}
                />
              </Field>
            </div>
            <p className="text-xs text-muted">
              Votre mot de passe part vers {[...new Set(mail.hosts)].join(', ')}, toujours chiffré :
              il y est vérifié avant d’être gardé, scellé pour vous seul, et n’est jamais réaffiché.
              Il est effacé quand vous retirez votre compte.
            </p>
            <div className="flex flex-wrap gap-2">
              <Button type="submit" loading={action.pending}>
                {mail.linked ? 'Relier de nouveau' : 'Relier mon compte'}
              </Button>
              {mail.linked && (
                <Button type="button" variant="danger" disabled={action.pending} onClick={remove}>
                  Retirer mon compte
                </Button>
              )}
            </div>
          </form>
        ) : (
          <p className="text-sm text-muted">
            La messagerie est désactivée, ou vous n’avez pas la permission d’utiliser les
            connecteurs : demandez à votre administrateur.
            {mail.linked && ' Vous pouvez retirer votre compte.'}
          </p>
        )}
        {!mail.available && mail.linked && (
          <div>
            <Button variant="danger" disabled={action.pending} onClick={remove}>
              Retirer mon compte
            </Button>
          </div>
        )}
        {action.error && <ErrorNote message={action.error} />}
      </section>
    </Card>
  );
}
