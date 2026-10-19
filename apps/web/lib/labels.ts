/** French labels of the statuses and levels shown in the interface. */
import type { ToneLabel } from '@/components/ui';
import type { AccountStatus, OrgUnitType, Sensitivity, SkillStatus, ToolRisk } from '@/lib/api';

export const RISK_LABELS: Record<ToolRisk, ToneLabel> = {
  read: { label: 'Lecture', tone: 'neutral' },
  draft: { label: 'Brouillon', tone: 'accent' },
  write: { label: 'Modification', tone: 'warn' },
  external: { label: 'Impact externe', tone: 'danger' },
  irreversible: { label: 'Irréversible', tone: 'danger' },
};

export const RUN_STATUS: Record<string, ToneLabel> = {
  queued: { label: 'En file', tone: 'neutral' },
  running: { label: 'En cours', tone: 'accent' },
  awaiting_approval: { label: 'Validation requise', tone: 'warn' },
  completed: { label: 'Terminée', tone: 'ok' },
  failed: { label: 'Échec', tone: 'danger' },
  cancelled: { label: 'Annulée', tone: 'neutral' },
};

export const APPROVAL_STATUS: Record<string, ToneLabel> = {
  pending: { label: 'En attente', tone: 'warn' },
  approved: { label: 'Validée', tone: 'ok' },
  rejected: { label: 'Refusée', tone: 'danger' },
  expired: { label: 'Expirée', tone: 'neutral' },
  cancelled: { label: 'Annulée', tone: 'neutral' },
};

export const TOOL_STATUS: Record<string, ToneLabel> = {
  succeeded: { label: 'Réussi', tone: 'ok' },
  failed: { label: 'Échec', tone: 'danger' },
  denied: { label: 'Refusé', tone: 'danger' },
  awaiting_approval: { label: 'Validation demandée', tone: 'warn' },
  rejected: { label: 'Rejeté', tone: 'neutral' },
};

export const AUTONOMY_LABELS: Record<string, string> = {
  read_only: 'Lecture seule',
  propose: 'Proposition',
  execute_with_approval: 'Exécution après validation',
  autonomous_limited: 'Autonomie limitée',
};

export const CLEARANCE_LABELS: Record<Sensitivity, string> = {
  public: 'Public',
  internal: 'Interne',
  confidential: 'Confidentiel',
  restricted: 'Restreint',
};

export const ACCOUNT_STATUS: Record<AccountStatus, ToneLabel> = {
  active: { label: 'Actif', tone: 'ok' },
  invited: { label: 'Invité', tone: 'neutral' },
  disabled: { label: 'Désactivé', tone: 'danger' },
};

export const SKILL_STATUS: Record<SkillStatus, ToneLabel> = {
  draft: { label: 'Brouillon', tone: 'neutral' },
  active: { label: 'Active', tone: 'ok' },
  retired: { label: 'Retirée', tone: 'neutral' },
};

export const UNIT_TYPES: Record<OrgUnitType, string> = {
  company: 'Entreprise',
  site: 'Établissement',
  direction: 'Direction',
  department: 'Département',
  service: 'Service',
  team: 'Équipe',
};

/** Permission groups, by the resource before the colon (`planning:apply`). */
export const PERMISSION_GROUPS: Record<string, string> = {
  agent: 'Agent',
  org: 'Organisation',
  user: 'Comptes',
  role: 'Rôles',
  policy: 'Politiques d’accès',
  memory: 'Mémoire',
  mission: 'Missions',
  approval: 'Validations',
  skill: 'Compétences',
  connector: 'Connecteurs',
  audit: 'Audit',
  settings: 'Paramètres',
  usage: 'Consommation',
  intervention: 'Interventions',
  planning: 'Planning',
};

/** Model providers a company can choose (ADR-024). */
export const LLM_PROVIDERS: Record<string, string> = {
  anthropic: 'Anthropic (Claude)',
  openai: 'OpenAI',
  openrouter: 'OpenRouter',
  gemini: 'Google Gemini',
  ollama: 'Ollama (serveur de l’exploitant)',
  openai_compatible: 'Compatible OpenAI (serveur de l’exploitant)',
};
