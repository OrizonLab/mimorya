/** FICTIONAL demo company "Néo Maintenance" — example ABAC policy. */
import type { AbacPolicy } from '../authz/types';
import { demoId } from './ids';

/** Example ABAC policy: administrative staff only sees completed interventions (billing). */
export const DEMO_POLICIES: readonly AbacPolicy[] = [
  {
    id: demoId(400),
    description: 'Le service administratif ne consulte que les interventions terminées',
    effect: 'deny',
    permissions: ['intervention:read'],
    condition: {
      all: [
        {
          op: 'eq',
          left: { attr: 'subject.attributes.department' },
          right: { value: 'administratif' },
        },
        { op: 'eq', left: { attr: 'resource.type' }, right: { value: 'intervention' } },
        { op: 'neq', left: { attr: 'resource.attributes.status' }, right: { value: 'completed' } },
      ],
    },
  },
];
