/**
 * In-memory implementations of the engine ports, for tests and local
 * experiments. Not for production use.
 */
import { randomUUID } from 'node:crypto';
import {
  sensitivityRank,
  type Deliverable,
  type DeliverableInput,
  type DeliverableService,
  type MemoryKind,
  type MemoryReadFilter,
  type MemoryScope,
  type Sensitivity,
} from '@mimorya/core';
import type {
  ApprovalRequest,
  AuditEntry,
  EnginePorts,
  MemoryHit,
  NewMemory,
  RunEvent,
  SkillSummary,
  ToolInvocationRecord,
  UsageRecord,
} from '../runtime/ports';
import type { RunCheckpoint } from '../runtime/state';

export interface StoredMemory extends NewMemory {
  id: string;
  sensitivity: Sensitivity;
  updatedAt: Date;
  useCount: number;
}

export interface MemoryFixture {
  scope: MemoryScope;
  kind: MemoryKind;
  title: string;
  content: string;
  ownerUserId?: string;
  orgUnitId?: string;
  sensitivity?: Sensitivity;
  status?: 'active' | 'proposed';
}

export interface SkillFixture {
  name: string;
  description: string;
  content: string;
}

export class InMemoryPorts implements EnginePorts {
  readonly emitted: RunEvent[] = [];
  readonly auditEntries: AuditEntry[] = [];
  readonly toolRecords: ToolInvocationRecord[] = [];
  readonly approvalRequests: Array<ApprovalRequest & { id: string }> = [];
  readonly usageRecords: UsageRecord[] = [];
  readonly memories: StoredMemory[] = [];
  readonly checkpoints: RunCheckpoint[] = [];
  readonly deliverableItems = new Map<string, Deliverable>();

  constructor(
    private readonly context: { userId: string; runId: string },
    fixtures: { memories?: MemoryFixture[]; skills?: SkillFixture[] } = {},
  ) {
    for (const m of fixtures.memories ?? []) {
      this.memories.push({
        id: randomUUID(),
        scope: m.scope,
        kind: m.kind,
        title: m.title,
        content: m.content,
        ownerUserId: m.ownerUserId ?? null,
        orgUnitId: m.orgUnitId ?? null,
        sensitivity: m.sensitivity ?? 'internal',
        status: m.status ?? 'active',
        source: { type: 'manual' },
        updatedAt: new Date('2026-01-01T00:00:00Z'),
        useCount: 0,
      });
    }
    this.skillFixtures = fixtures.skills ?? [];
  }

  private readonly skillFixtures: SkillFixture[];

  events = { emit: async (event: RunEvent) => void this.emitted.push(event) };
  audit = { record: async (entry: AuditEntry) => void this.auditEntries.push(entry) };
  toolLog = { record: async (entry: ToolInvocationRecord) => void this.toolRecords.push(entry) };
  usage = { record: async (entry: UsageRecord) => void this.usageRecords.push(entry) };
  runs = {
    checkpoint: async (state: RunCheckpoint) => void this.checkpoints.push(structuredClone(state)),
  };

  approvals = {
    create: async (request: ApprovalRequest) => {
      const id = randomUUID();
      this.approvalRequests.push({ ...request, id });
      return { id };
    },
  };

  memory = {
    search: async (
      filter: MemoryReadFilter,
      query: string,
      options: { limit: number; clearanceRank: number },
    ): Promise<MemoryHit[]> => {
      const terms = tokenize(query);
      return this.memories
        .filter((m) => m.status === 'active' && isVisible(m, filter))
        .filter(
          (m) =>
            m.ownerUserId === filter.ownerUserId ||
            sensitivityRank(m.sensitivity) <= options.clearanceRank,
        )
        .map((m) => ({ m, score: score(terms, `${m.title} ${m.content}`) }))
        .filter((x) => x.score > 0)
        .sort((a, b) => b.score - a.score)
        .slice(0, options.limit)
        .map(({ m, score: s }) => ({
          id: m.id,
          scope: m.scope,
          kind: m.kind,
          title: m.title,
          content: m.content,
          orgUnitId: m.orgUnitId ?? null,
          updatedAt: m.updatedAt,
          score: s,
        }));
    },
    create: async (memory: NewMemory) => {
      const id = randomUUID();
      this.memories.push({
        ...memory,
        id,
        sensitivity: memory.sensitivity ?? 'internal',
        updatedAt: new Date(),
        useCount: 0,
      });
      return { id };
    },
    markUsed: async (ids: string[]) => {
      for (const m of this.memories) if (ids.includes(m.id)) m.useCount += 1;
    },
  };

  skills = {
    listActive: async (): Promise<SkillSummary[]> =>
      this.skillFixtures.map((s) => ({ name: s.name, description: s.description })),
    getActive: async (name: string) => {
      const skill = this.skillFixtures.find((s) => s.name === name);
      return skill ? { name: skill.name, version: 1, content: skill.content } : null;
    },
  };

  deliverables: DeliverableService = {
    create: async (input: DeliverableInput) => {
      const deliverable: Deliverable = {
        ...input,
        id: randomUUID(),
        ownerUserId: this.context.userId,
        runId: this.context.runId,
        status: 'draft',
      };
      this.deliverableItems.set(deliverable.id, deliverable);
      return deliverable;
    },
    get: async (id: string) => this.deliverableItems.get(id) ?? null,
    setStatus: async (id: string, status: Deliverable['status']) => {
      const d = this.deliverableItems.get(id);
      if (d) d.status = status;
    },
  };

  eventsOfType<T extends RunEvent['type']>(type: T): Extract<RunEvent, { type: T }>[] {
    return this.emitted.filter((e): e is Extract<RunEvent, { type: T }> => e.type === type);
  }
}

function isVisible(m: StoredMemory, filter: MemoryReadFilter): boolean {
  if (m.scope === 'user') return m.ownerUserId === filter.ownerUserId;
  if (!filter.shared) return false;
  if (m.scope === 'org_unit') {
    return filter.orgUnitIds === 'all' || (!!m.orgUnitId && filter.orgUnitIds.has(m.orgUnitId));
  }
  return true;
}

function tokenize(text: string): string[] {
  return text
    .toLowerCase()
    .normalize('NFD')
    .replace(/[̀-ͯ]/g, '')
    .split(/[^a-z0-9]+/)
    .filter((t) => t.length > 2);
}

function score(terms: string[], text: string): number {
  const words = new Set(tokenize(text));
  return terms.filter((t) => words.has(t)).length;
}

export { ScriptedProvider, lastToolResults, type ScriptedStep } from '../llm/scripted';
