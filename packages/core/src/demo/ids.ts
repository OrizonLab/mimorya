/**
 * FICTIONAL demo company used by the seed script, tests and the reference
 * scenario ("PME de services techniques"). All names and data are invented.
 * Identifiers are deterministic so that tests and seeds stay reproducible.
 */

export function demoId(n: number): string {
  return `00000000-0000-4000-8000-${n.toString().padStart(12, '0')}`;
}

export const DEMO_TENANT = {
  id: demoId(1),
  slug: 'neo-maintenance',
  name: 'Néo Maintenance (démo)',
} as const;

export const DEMO_UNITS = {
  company: demoId(100),
  direction: demoId(101),
  technical: demoId(102),
  teamNorth: demoId(103),
  teamSouth: demoId(104),
  admin: demoId(105),
} as const;

export const DEMO_USERS = {
  claire: demoId(200),
  marc: demoId(201),
  julien: demoId(202),
  lucas: demoId(203),
  emma: demoId(204),
  nadia: demoId(205),
  hugo: demoId(206),
  lea: demoId(207),
  isabelle: demoId(208),
  paul: demoId(209),
} as const;

export const DEMO_ROLES = {
  member: demoId(300),
  tenantAdmin: demoId(301),
  knowledgeManager: demoId(302),
  technician: demoId(310),
  teamLead: demoId(311),
  director: demoId(312),
  adminStaff: demoId(313),
} as const;
