'use client';

import { Fragment, useEffect, useRef, useState, type ReactNode } from 'react';
import { Button, Card, Cell, DataTable, ErrorNote, Field, Input, Loaded } from '@/components/ui';
import {
  api,
  useAction,
  useApiData,
  type BearerTokenCreated,
  type BearerTokenView,
  type Done,
  type NewBearerToken,
} from '@/lib/api';
import { formatDate } from '@/lib/format';

/** What a list of tokens says, as the API gives it. */
interface TokenList {
  tokens: BearerTokenView[];
  maxTokens: number;
  nameLength: number;
}

/** The words of a kind of token, as the section says them. */
export interface BearerTokenWords {
  title: string;
  about: string;
  /** What the empty list says. */
  none: string;
  /** Who will hold a new token, as its name field asks it. */
  holder: string;
  /** What is lost when the token named `name` goes, asked before. */
  removal: (name: string) => string;
}

/**
 * The tokens another system holds of the company (ADR-181, ADR-183), read
 * at `path`: listed by name, when made and last used, never their secret;
 * made with a name, the secret shown this once (`notice`); removed, after a
 * confirmation. What each kind says of its use comes first (`details`). The
 * limits a new token meets are the API's. The list is read again after any
 * attempt, refused or not; the focus goes back to the section's title when
 * the notice closes, and when what had it goes with the list read again (a
 * removed row, the form the limit replaces), even after a refusal. The
 * notice stands outside the list, never locked by its reading. `onChanged`
 * reloads what traces the change, when it is shown.
 */
export function BearerTokens<T extends TokenList>({
  path,
  words,
  details,
  notice,
  onChanged,
}: {
  path: string;
  words: BearerTokenWords;
  details: (data: T) => ReactNode;
  notice: (made: BearerTokenCreated, data: T, onClose: () => void) => ReactNode;
  onChanged?: () => void;
}) {
  const state = useApiData<T>(path);
  const [name, setName] = useState('');
  const [made, setMade] = useState<BearerTokenCreated | null>(null);
  const action = useAction();
  const titleRef = useRef<HTMLHeadingElement>(null);
  // What had the focus when an attempt began, until the list is read again.
  const focusedBefore = useRef<Element | null>(null);

  // The list read again after an attempt: if what had the focus went with
  // it, and nothing else took it, the title takes it rather than the page.
  // A control kept only locked by the reading gets it back on its own
  // (`keepFocusAcrossDisabling`).
  useEffect(() => {
    const before = focusedBefore.current;
    if (state.loading || before === null) return;
    focusedBefore.current = null;
    const active = document.activeElement;
    if (!before.isConnected && (active === null || active === document.body)) {
      titleRef.current?.focus();
    }
  }, [state.loading]);

  /** Reads the list again after an attempt that began with `before` focused. */
  function readAgain(before: Element | null) {
    focusedBefore.current = before;
    state.reload();
  }

  async function create() {
    const before = document.activeElement;
    const body = { name } satisfies NewBearerToken;
    const created = await action.run(() => api.post<BearerTokenCreated>(path, body));
    readAgain(before);
    if (created) {
      setMade(created);
      setName('');
      onChanged?.();
    }
  }

  async function remove(token: BearerTokenView) {
    if (!window.confirm(words.removal(token.name))) return;
    const before = document.activeElement;
    const done = await action.run(() => api.delete<Done>(`${path}/${token.id}`));
    readAgain(before);
    if (done) {
      if (made?.token.id === token.id) setMade(null);
      onChanged?.();
    }
  }

  function closeNotice() {
    setMade(null);
    titleRef.current?.focus();
  }

  return (
    <Card className="flex flex-col gap-3 p-4">
      <h2 ref={titleRef} tabIndex={-1} className="font-medium outline-none">
        {words.title}
      </h2>
      <p className="text-sm text-muted">{words.about}</p>
      {made && state.data && (
        <Fragment key={made.token.id}>{notice(made, state.data, closeNotice)}</Fragment>
      )}
      <Loaded state={state}>
        {(data) => (
          <>
            {details(data)}
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
                    {words.none}
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
                <Field label={`Nom du nouveau jeton : ${words.holder}`} className="grow">
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
