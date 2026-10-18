/**
 * Demo tenant provisioning (FICTIONAL data, see @mimorya/core/demo).
 * Idempotent: an existing demo tenant is left untouched unless `reset` is set.
 */
import { DEMO_TENANT } from '@mimorya/core/demo';
import { eq } from 'drizzle-orm';
import { withTenant, type Database } from '../client';
import { tenants } from '../schema';
import { insertDemoConnectors } from './connectors';
import { insertDemoMemories } from './memories';
import type { SeedOptions } from './options';
import { insertDemoOrganization } from './organization';
import { insertDemoSkills } from './skills';

export type { SeedOptions } from './options';
export { DEMO_CONNECTOR_ID } from './connectors';
export { DEMO_SKILL_ID } from './skills';

export async function seedDemoTenant(
  db: Database,
  options: SeedOptions,
): Promise<'created' | 'skipped'> {
  return withTenant(db, DEMO_TENANT.id, async (tx) => {
    const existing = await tx
      .select({ id: tenants.id })
      .from(tenants)
      .where(eq(tenants.id, DEMO_TENANT.id));
    if (existing.length > 0) {
      if (!options.reset) return 'skipped';
      await tx.delete(tenants).where(eq(tenants.id, DEMO_TENANT.id));
    }
    await insertDemoOrganization(tx, options);
    await insertDemoConnectors(tx);
    await insertDemoMemories(tx);
    await insertDemoSkills(tx);
    return 'created';
  });
}
