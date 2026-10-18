/** FICTIONAL demo company "Néo Maintenance" — organisation chart. */
import type { OrgUnitType } from '../authz/org-tree';
import { DEMO_UNITS, DEMO_USERS } from './ids';

export interface DemoOrgUnit {
  id: string;
  parentId: string | null;
  name: string;
  type: OrgUnitType;
  leadUserId: string | null;
}

export const DEMO_ORG_UNITS: readonly DemoOrgUnit[] = [
  {
    id: DEMO_UNITS.company,
    parentId: null,
    name: 'Néo Maintenance',
    type: 'company',
    leadUserId: DEMO_USERS.claire,
  },
  {
    id: DEMO_UNITS.direction,
    parentId: DEMO_UNITS.company,
    name: 'Direction',
    type: 'direction',
    leadUserId: DEMO_USERS.claire,
  },
  {
    id: DEMO_UNITS.technical,
    parentId: DEMO_UNITS.company,
    name: 'Service technique',
    type: 'department',
    leadUserId: DEMO_USERS.marc,
  },
  {
    id: DEMO_UNITS.teamNorth,
    parentId: DEMO_UNITS.technical,
    name: 'Équipe Nord',
    type: 'team',
    leadUserId: DEMO_USERS.julien,
  },
  {
    id: DEMO_UNITS.teamSouth,
    parentId: DEMO_UNITS.technical,
    name: 'Équipe Sud',
    type: 'team',
    leadUserId: DEMO_USERS.nadia,
  },
  {
    id: DEMO_UNITS.admin,
    parentId: DEMO_UNITS.company,
    name: 'Service administratif',
    type: 'department',
    leadUserId: DEMO_USERS.isabelle,
  },
];
