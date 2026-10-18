/**
 * Runs without progress for a while belong to a crashed or stuck process:
 * they are marked as failed at startup and then every minute.
 */
import { failStaleRuns, type Database } from '@mimorya/db';
import type { FastifyInstance } from 'fastify';

const STALE_AFTER_MS = 15 * 60_000;
const SWEEP_INTERVAL_MS = 60_000;

export function registerStaleRunSweeper(app: FastifyInstance, db: Database): void {
  const sweep = async () => {
    const failed = await failStaleRuns(db, new Date(Date.now() - STALE_AFTER_MS)).catch(
      (err: unknown) => {
        app.log.warn({ err }, 'Stale run sweep failed');
        return 0;
      },
    );
    if (failed > 0) app.log.warn({ failed }, 'Stale runs marked as failed');
  };
  const sweeper = setInterval(() => void sweep(), SWEEP_INTERVAL_MS);
  sweeper.unref();
  app.addHook('onReady', sweep);
  app.addHook('onClose', async () => clearInterval(sweeper));
}
