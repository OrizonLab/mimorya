/**
 * In-process fan-out of run events to SSE subscribers. Single-instance only;
 * multi-instance deployments will relay through PostgreSQL LISTEN/NOTIFY
 * (see docs/ARCHITECTURE.md §8).
 */
export interface BusEvent {
  /** Persisted sequence number; null for transient events (text deltas). */
  seq: number | null;
  type: string;
  data: Record<string, unknown>;
}

type Listener = (event: BusEvent) => void;

export class RunEventBus {
  private readonly listeners = new Map<string, Set<Listener>>();

  publish(runId: string, event: BusEvent): void {
    for (const listener of this.listeners.get(runId) ?? []) listener(event);
  }

  subscribe(runId: string, listener: Listener): () => void {
    const set = this.listeners.get(runId) ?? new Set();
    set.add(listener);
    this.listeners.set(runId, set);
    return () => {
      set.delete(listener);
      if (set.size === 0) this.listeners.delete(runId);
    };
  }
}
