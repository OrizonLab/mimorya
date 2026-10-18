/** Demo connector instance backed by the fictional interventions dataset. */
import { DEMO_TENANT, demoId } from '@mimorya/core/demo';
import type { Transaction } from '../client';
import { connectorInstances } from '../schema';

export const DEMO_CONNECTOR_ID = demoId(500);

export async function insertDemoConnectors(tx: Transaction): Promise<void> {
  await tx.insert(connectorInstances).values({
    id: DEMO_CONNECTOR_ID,
    tenantId: DEMO_TENANT.id,
    type: 'demo_interventions',
    name: 'Interventions (démonstration)',
    config: { dataset: 'neo-maintenance', note: 'Données fictives' },
  });
}
