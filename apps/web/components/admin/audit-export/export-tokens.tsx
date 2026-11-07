'use client';

import { useRef, useState } from 'react';
import { Button, Card, Cell, DataTable, ErrorNote, Field, Input, Loaded } from '@/components/ui';
import {
  api,
  useAction,
  useApiData,
  type AuditExportTokenCreated,
  type AuditExportTokens,
  type AuditExportTokenView,
  type Done,
  type NewAuditExportToken,
} from '@/lib/api';
import { formatDate } from '@/lib/format';
import { ExportTokenNotice } from './export-token-notice';

const TOKENS = '/admin/audit/export-tokens';

/**
 * The tokens with which the company's SIEM reads its audit log (ADR-181,
 * ADR-182), for who may export it (`audit:export`): listed by name, when
 * made and last used, never their secret; made with a name, the secret shown
 * this once; removed, after a confirmation. The limits a new token meets are
 * the API's. The list is read again after any attempt, refused or not, and
 * the focus goes back to the section's title when the notice or a removed
 * token's row goes. `onChanged` reloads what traces the change, the audit
 * log, when it is shown.
 */
export function ExportTokens({ onChanged }: { onChanged?: () => void }) {
  const state = useApiData<AuditExportTokens>(TOKENS);
  const [name, setName] = useState('');
  const [made, setMade] = useState<AuditExportTokenCreated | null>(null);
  const action = useAction();
  const titleRef = useRef<HTMLHeadingElement>(null);

  async function create() {
    const body = { name } satisfies NewAuditExportToken;
    const created = await action.run(() => api.post<AuditExportTokenCreated>(TOKENS, body));
    state.reload();
    if (created) {
      setMade(created);
      setName('');
      onChanged?.();
    }
  }

  async function remove(token: AuditExportTokenView) {
    const asked = `Retirer le jeton « ${token.name} » ? Le SIEM qui le détient ne lira plus rien.`;
    if (!window.confirm(asked)) return;
    const done = await action.run(() => api.delete<Done>(`${TOKENS}/${token.id}`));
    state.reload();
    if (done) {
      if (made?.token.id === token.id) setMade(null);
      onChanged?.();
      // Its row, and the button that had the focus, are gone.
      titleRef.current?.focus();
    }
  }

  function closeNotice() {
    setMade(null);
    titleRef.current?.focus();
  }

  return (
    <Card className="flex flex-col gap-3 p-4">
      <h2 ref={titleRef} tabIndex={-1} className="font-medium outline-none">
        Export vers un SIEM
      </h2>
      <p className="text-sm text-muted">
        Votre outil de sécurité (SIEM) lit le journal scellé avec un jeton, page après page, dans
        l’ordre de la chaîne. Chaque entrée porte sa place et son sceau : il reprend après la
        dernière place lue, en donnant son sceau, et garde les sceaux comme ancres à vérifier.
      </p>
      <Loaded state={state}>
        {(data) => (
          <>
            {!data.sealing && (
              <p role="note" className="text-sm text-warn">
                Ce serveur ne scelle pas le journal d’audit : aucune clé maître n’y est configurée,
                rien n’en est exporté. Adressez-vous à l’exploitant de la plateforme.
              </p>
            )}
            <p className="text-sm">
              Adresse à lire (méthode GET) :{' '}
              <code className="rounded bg-surface-2 px-1 text-xs break-all select-all">
                {data.exportUrl}
              </code>
            </p>
            {made && (
              <ExportTokenNotice
                key={made.token.id}
                made={made}
                exportUrl={data.exportUrl}
                onClose={closeNotice}
              />
            )}
            <DataTable columns={['Jeton', 'Créé le', 'Dernier usage', '']}>
              {data.tokens.map((token) => (
                <tr key={token.id}>
                  <Cell>{token.name}</Cell>
                  <Cell className="text-xs whitespace-nowrap text-muted">
                    {formatDate(token.createdAt)}
                  </Cell>
                  <Cell className="text-xs whitespace-nowrap text-muted">
                    {token.lastUsedAt ? formatDate(token.lastUsedAt) : 'Jamais'}
                  </Cell>
                  <Cell className="text-right">
                    <Button
                      variant="secondary"
                      aria-label={`Retirer le jeton ${token.name}`}
                      onClick={() => remove(token)}
                    >
                      Retirer
                    </Button>
                  </Cell>
                </tr>
              ))}
              {data.tokens.length === 0 && (
                <tr>
                  <Cell colSpan={4} className="py-4 text-center text-muted">
                    Aucun jeton : le journal n’est lu par aucun SIEM.
                  </Cell>
                </tr>
              )}
            </DataTable>
            {data.tokens.length < data.maxTokens ? (
              <form
                className="flex flex-wrap items-end gap-2"
                onSubmit={(e) => {
                  e.preventDefault();
                  void create();
                }}
              >
                <Field label="Nom du nouveau jeton : le SIEM qui le détiendra" className="grow">
                  <Input
                    value={name}
                    maxLength={data.nameLength}
                    onChange={(e) => setName(e.target.value)}
                  />
                </Field>
                <Button type="submit" loading={action.pending} disabled={name.trim() === ''}>
                  Créer un jeton
                </Button>
              </form>
            ) : (
              <p className="text-sm text-muted">
                L’entreprise a déjà {data.maxTokens} jetons : retirez-en un pour en créer un autre.
              </p>
            )}
          </>
        )}
      </Loaded>
      {action.error && <ErrorNote message={action.error} />}
    </Card>
  );
}
