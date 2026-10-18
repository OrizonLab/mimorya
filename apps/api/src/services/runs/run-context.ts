/**
 * What a run is given to work with: the tools in reach of the user, the
 * system prompt, and the memories relevant to the request.
 */
import {
  buildSystemPrompt,
  createPlatformTools,
  ToolRegistry,
  type EnginePorts,
} from '@mimorya/agent';
import { memoryReadFilter, sensitivityRank } from '@mimorya/core';
import {
  listActiveSkills,
  markMemoryUsed,
  searchMemory,
  withTenant,
  type Database,
} from '@mimorya/db';
import type { AccessContext } from '../access';
import type { ConnectorService } from '../connectors';

const MEMORY_RECALL_LIMIT = 5;

export async function buildToolRegistry(
  access: AccessContext,
  ports: EnginePorts,
  connectors: ConnectorService,
): Promise<ToolRegistry> {
  const connectorTools = await connectors.toolsFor(access.subject.tenantId);
  return new ToolRegistry(
    [
      ...createPlatformTools({
        engine: access.engine,
        orgTree: access.orgTree,
        memory: ports.memory,
        skills: ports.skills,
      }),
      ...connectorTools.tools,
    ],
    connectorTools.overrides,
  );
}

export async function buildRunSystemPrompt(
  db: Database,
  access: AccessContext,
  assistantName: string,
  now: Date,
): Promise<string> {
  const skills = await withTenant(db, access.subject.tenantId, (tx) => listActiveSkills(tx));
  const primaryUnit = access.profile.primaryUnitId;
  return buildSystemPrompt(
    {
      assistantName,
      tenantName: access.tenant.name,
      displayName: access.profile.displayName,
      jobTitle: access.profile.jobTitle,
      orgUnitPath: primaryUnit ? access.orgTree.pathLabel(primaryUnit) : null,
      roles: access.grants,
    },
    skills,
    now,
  );
}

/** Memories the agent may read for this user, ranked against the request. */
export async function recallMemories(db: Database, access: AccessContext, text: string) {
  const filter = memoryReadFilter(access.engine, access.orgTree, access.subject, {
    type: 'agent',
  });
  return withTenant(db, access.subject.tenantId, async (tx) => {
    const hits = await searchMemory(tx, filter, text, {
      limit: MEMORY_RECALL_LIMIT,
      clearanceRank: sensitivityRank(access.subject.clearance ?? 'internal'),
    });
    await markMemoryUsed(
      tx,
      hits.map((h) => h.id),
    );
    return hits;
  });
}
