import { Button } from './button';
import { ErrorNote } from './feedback';

/** A load that failed, and a way to try it again (ADR-044). */
export function Failure({ message, onRetry }: { message: string; onRetry: () => void }) {
  return (
    <div className="flex flex-col items-start gap-2">
      <ErrorNote message={message} />
      <Button type="button" variant="secondary" onClick={onRetry}>
        Réessayer
      </Button>
    </div>
  );
}
