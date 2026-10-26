/** French labels of the statuses and levels shown in the interface. */
import type { ToneLabel } from '@/components/ui';
import type {
  AccountStatus,
  AgentProfile,
  CaseOutcomeView,
  DeliveryOutcome,
  DocumentKind,
  EvaluationStatus,
  MissionStatus,
  OccurrenceOutcome,
  OrgUnitType,
  Sensitivity,
  SkillStatus,
  SkillVersionStatus,
  ToolRisk,
} from '@/lib/api';

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

export const SKILL_VERSION_STATUS: Record<SkillVersionStatus, ToneLabel> = {
  draft: { label: 'Brouillon', tone: 'neutral' },
  candidate: { label: 'Proposée', tone: 'accent' },
  active: { label: 'Active', tone: 'ok' },
  rejected: { label: 'Rejetée', tone: 'warn' },
  retired: { label: 'Retirée', tone: 'neutral' },
};

export const EVALUATION_STATUS: Record<EvaluationStatus, ToneLabel> = {
  queued: { label: 'En file', tone: 'neutral' },
  running: { label: 'En cours', tone: 'accent' },
  completed: { label: 'Terminée', tone: 'ok' },
  failed: { label: 'Échouée', tone: 'danger' },
};

export const CASE_OUTCOME: Record<CaseOutcomeView, ToneLabel> = {
  passed: { label: 'Réussi', tone: 'ok' },
  failed: { label: 'Manqué', tone: 'danger' },
  error: { label: 'En erreur', tone: 'warn' },
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
  document: 'Documents',
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

/** Identity providers a company can declare (ADR-063). */
export const IDENTITY_PROVIDER_KINDS: Record<string, string> = {
  microsoft: 'Microsoft 365 (Entra ID)',
  google: 'Google Workspace',
  oidc: 'OpenID Connect (Okta, Keycloak…)',
  saml: 'SAML 2.0 (AD FS, Okta, Shibboleth…)',
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

/** Embedding providers a company may choose (ADR-074). */
export const EMBEDDING_PROVIDERS: Record<string, string> = {
  mistral: 'Mistral AI',
  openai: 'OpenAI',
  ollama: 'Ollama (serveur de l’exploitant)',
  openai_compatible: 'Compatible OpenAI (serveur de l’exploitant)',
};

/** What became of a scheduled task's last occurrence (ADR-080). */
export const OCCURRENCE_OUTCOME: Record<OccurrenceOutcome, ToneLabel> = {
  started: { label: 'Lancée', tone: 'ok' },
  still_running: { label: 'Précédente non terminée', tone: 'warn' },
  missed: { label: 'Non rattrapée', tone: 'neutral' },
  refused: { label: 'Refusée', tone: 'danger' },
  failed: { label: 'Échec', tone: 'danger' },
};

/** What became of an inbound hook's last delivery (ADR-109). */
export const DELIVERY_OUTCOME: Record<DeliveryOutcome, ToneLabel> = {
  started: { label: 'Lancé', tone: 'ok' },
  limited: { label: 'Limite horaire atteinte', tone: 'warn' },
  refused: { label: 'Refusé', tone: 'danger' },
  failed: { label: 'Échec', tone: 'danger' },
};

/** How a mission stands (ADR-089). */
export const MISSION_STATUS: Record<MissionStatus, ToneLabel> = {
  in_progress: { label: 'En cours', tone: 'accent' },
  awaiting_approval: { label: 'En attente de validation', tone: 'warn' },
  done: { label: 'Terminée', tone: 'ok' },
  failed: { label: 'Inachevée', tone: 'danger' },
  cancelled: { label: 'Arrêtée', tone: 'neutral' },
};

/** What a deposited document is (ADR-095). */
export const DOCUMENT_KINDS: Record<DocumentKind, string> = {
  pdf: 'PDF',
  docx: 'Word',
  markdown: 'Markdown',
  text: 'Texte',
};

/** Who works a run: the agent, or one of its sub-agents (ADR-098). */
export const AGENT_PROFILES: Record<AgentProfile, string> = {
  main: 'Agent principal',
  research: 'Recherche',
  analysis: 'Analyse',
  planning: 'Planification',
  quality: 'Contrôle qualité',
};
