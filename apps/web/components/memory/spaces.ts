/** Memory spaces as shown to users, and the kinds of memories. */
export type MemoryScope = 'user' | 'org_unit' | 'tenant' | 'experience';

export interface MemoryItem {
  id: string;
  scope: MemoryScope;
  orgUnit: string | null;
  kind: string;
  title: string;
  content: string;
  status: 'active' | 'proposed' | 'archived';
  useCount: number;
  updatedAt: string;
  canManage: boolean;
}

export interface MemorySpaceInfo {
  scope: MemoryScope;
  title: string;
  description: string;
}

export const MEMORY_SPACES: MemorySpaceInfo[] = [
  {
    scope: 'user',
    title: 'Ma mémoire',
    description: 'Visible par vous seul (et par l’agent quand il travaille pour vous).',
  },
  {
    scope: 'org_unit',
    title: 'Équipes et services',
    description: 'Connaissances partagées de vos équipes.',
  },
  {
    scope: 'tenant',
    title: 'Entreprise',
    description: 'Connaissances communes à toute l’entreprise.',
  },
  {
    scope: 'experience',
    title: 'Expérience de l’agent',
    description: 'Leçons et procédures validées.',
  },
];

export const MEMORY_KINDS: Record<string, string> = {
  preference: 'Préférence',
  fact: 'Fait',
  procedure: 'Procédure',
  decision: 'Décision',
  lesson: 'Leçon',
  episode: 'Épisode',
};
