import { Send } from 'lucide-react';
import type { FormEvent, KeyboardEvent } from 'react';
import { Button } from '@/components/ui';

const MAX_ROWS = 6;

/** Message input: Enter sends, Shift+Enter adds a line. */
export function Composer({
  value,
  onChange,
  onSend,
  busy,
  sending,
  disabled,
}: {
  value: string;
  onChange: (value: string) => void;
  onSend: () => void;
  busy: boolean;
  sending: boolean;
  disabled: boolean;
}) {
  function onSubmit(event: FormEvent) {
    event.preventDefault();
    onSend();
  }

  function onKeyDown(event: KeyboardEvent<HTMLTextAreaElement>) {
    if (event.key === 'Enter' && !event.shiftKey) {
      event.preventDefault();
      onSend();
    }
  }

  return (
    <form onSubmit={onSubmit} className="border-t border-line bg-surface px-4 py-3">
      <div className="mx-auto flex max-w-3xl items-end gap-2">
        <label htmlFor="composer" className="sr-only">
          Votre message
        </label>
        <textarea
          id="composer"
          rows={Math.min(MAX_ROWS, Math.max(1, value.split('\n').length))}
          value={value}
          onChange={(e) => onChange(e.target.value)}
          onKeyDown={onKeyDown}
          placeholder={busy ? 'Traitement en cours…' : 'Écrivez votre demande…'}
          disabled={busy || disabled}
          className="min-h-[2.75rem] flex-1 resize-none rounded-xl border border-line bg-bg px-3 py-2.5 text-[0.95rem] outline-none focus:border-accent disabled:opacity-60"
        />
        <Button
          type="submit"
          loading={sending}
          disabled={busy || !value.trim()}
          aria-label="Envoyer"
        >
          <Send className="size-4" aria-hidden />
        </Button>
      </div>
    </form>
  );
}
