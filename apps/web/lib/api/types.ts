/** Shapes of the API responses shared across screens. */

export interface Me {
  user: {
    id: string;
    email: string;
    displayName: string;
    jobTitle: string | null;
    orgUnit: { id: string; path: string } | null;
  };
  tenant: { id: string; name: string; demo: boolean };
  grants: string[];
  permissions: string[];
  assistant: { name: string; available: boolean };
}

export interface ConversationSummary {
  id: string;
  title: string;
  updatedAt: string;
}

export interface Citation {
  type: string;
  id: string;
  label?: string;
}

export interface Message {
  id: string;
  role: 'user' | 'assistant';
  content: string;
  runId: string | null;
  createdAt: string;
  metadata: {
    status?: string;
    evidence?: Citation[];
    verification?: Verification;
  };
}

export type RunStatus =
  'queued' | 'running' | 'awaiting_approval' | 'completed' | 'failed' | 'cancelled';

export interface Approval {
  id: string;
  runId: string;
  toolName: string;
  actionSummary: string;
  payload: unknown;
  risk: string;
  reason: string;
  status: string;
  decisionComment: string | null;
  createdAt: string;
  decidedAt: string | null;
}

export interface RunUsage {
  steps: number;
  toolCalls: number;
  inputTokens: number;
  outputTokens: number;
  costMicros: number;
}

export interface Verification {
  passed: boolean;
  issues: string[];
  judged: boolean;
}

export interface ConversationDetail {
  conversation: ConversationSummary;
  messages: Message[];
  runs: Array<{ id: string; status: RunStatus; createdAt: string }>;
}

export interface RunDetail {
  run: {
    id: string;
    conversationId: string | null;
    objective: string;
    status: RunStatus;
    autonomyLevel: string;
    usage: RunUsage;
    verification: Verification | null;
    error: string | null;
    createdAt: string;
    finishedAt: string | null;
  };
  tools: ToolInvocation[];
  approvals: Approval[];
}

export interface ToolInvocation {
  id: string;
  toolName: string;
  decision: string;
  decisionReason: string | null;
  status: string;
  outputSummary: string | null;
  durationMs: number | null;
  createdAt: string;
}
