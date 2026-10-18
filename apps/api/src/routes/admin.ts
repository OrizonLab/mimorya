/** Read-only administration endpoints (management screens come next). */
import { CONNECTOR_TYPES } from '@mimorya/connectors';
import { PLATFORM_PERMISSIONS } from '@mimorya/core';
import {
  listAudit,
  listConnectorInstances,
  listRoleAssignments,
  listSkills,
  listUsers,
  usageSummary,
  withTenant,
} from '@mimorya/db';
import type { FastifyInstance } from 'fastify';
import { z } from 'zod';
import { parse } from '../http';
import { requireAuth } from '../plugins/session';
import type { AppServices } from '../services';
import { describeScope } from '../services/access';

export function registerAdminRoutes(app: FastifyInstance, services: AppServices) {
  const { db, access } = services;

  app.get('/admin/organization', async (request) => {
    const ctx = await access.context(requireAuth(request));
    ctx.require('org:read');
    const users = await withTenant(db, ctx.subject.tenantId, (tx) => listUsers(tx));
    return {
      units: ctx.orgTree.all().map((u) => ({
        id: u.id,
        parentId: u.parentId,
        name: u.name,
        type: u.type,
        leadUserId: u.leadUserId ?? null,
        members: users.filter((m) => m.primaryUnitId === u.id).map((m) => m.id),
      })),
      users: users.map((u) => ({ id: u.id, displayName: u.displayName, jobTitle: u.jobTitle })),
    };
  });

  app.get('/admin/users', async (request) => {
    const ctx = await access.context(requireAuth(request));
    ctx.require('user:read');
    const canSeeRoles = ctx.engine.has(ctx.subject, 'role:manage');
    return withTenant(db, ctx.subject.tenantId, async (tx) => {
      const users = await listUsers(tx);
      const assignments = canSeeRoles ? await listRoleAssignments(tx) : [];
      return {
        users: users.map((u) => ({
          id: u.id,
          displayName: u.displayName,
          jobTitle: u.jobTitle,
          status: u.status,
          orgUnit: u.primaryUnitId ? ctx.orgTree.pathLabel(u.primaryUnitId) : null,
          ...(canSeeRoles
            ? {
                email: u.email,
                lastLoginAt: u.lastLoginAt,
                grants: assignments
                  .filter((a) => a.userId === u.id)
                  .map((a) => {
                    const scope =
                      a.scopeType === 'org_unit' && a.scopeOrgUnitId
                        ? {
                            type: 'org_unit' as const,
                            orgUnitId: a.scopeOrgUnitId,
                            includeDescendants: a.includeDescendants,
                          }
                        : a.scopeType === 'tenant'
                          ? { type: 'tenant' as const }
                          : { type: 'self' as const };
                    return `${a.roleName} — ${describeScope(scope, ctx.orgTree)}`;
                  }),
              }
            : {}),
        })),
      };
    });
  });

  app.get('/admin/roles', async (request) => {
    const ctx = await access.context(requireAuth(request));
    ctx.require('role:manage');
    return {
      roles: ctx.roles.map((r) => ({
        id: r.id,
        key: r.key,
        name: r.name,
        description: r.description,
        isSystem: r.isSystem,
        permissions: r.permissions,
      })),
      catalog: [
        ...PLATFORM_PERMISSIONS,
        ...[...CONNECTOR_TYPES.values()].flatMap((c) => c.permissions),
      ],
    };
  });

  app.get('/admin/audit', async (request) => {
    const ctx = await access.context(requireAuth(request));
    ctx.require('audit:read');
    const query = parse(
      z.object({
        limit: z.coerce.number().int().min(1).max(500).default(100),
        before: z.coerce.number().int().positive().optional(),
      }),
      request.query,
    );
    const rows = await withTenant(db, ctx.subject.tenantId, (tx) =>
      listAudit(tx, { limit: query.limit, ...(query.before ? { beforeId: query.before } : {}) }),
    );
    return { entries: rows };
  });

  app.get('/admin/usage', async (request) => {
    const ctx = await access.context(requireAuth(request));
    ctx.require('usage:read');
    const { days } = parse(
      z.object({ days: z.coerce.number().int().min(1).max(366).default(30) }),
      request.query,
    );
    const since = new Date(Date.now() - days * 24 * 3600 * 1000);
    const rows = await withTenant(db, ctx.subject.tenantId, (tx) => usageSummary(tx, since));
    return { since, rows };
  });

  app.get('/admin/connectors', async (request) => {
    const ctx = await access.context(requireAuth(request));
    ctx.require('connector:manage');
    const instances = await withTenant(db, ctx.subject.tenantId, (tx) =>
      listConnectorInstances(tx),
    );
    return {
      instances: instances.map((i) => ({
        id: i.id,
        type: i.type,
        name: i.name,
        enabled: i.enabled,
        status: i.status,
        toolPolicies: i.toolPolicies,
        hasSecret: i.secretId !== null,
        updatedAt: i.updatedAt,
      })),
      types: [...CONNECTOR_TYPES.values()].map((c) => ({
        type: c.type,
        name: c.name,
        description: c.description,
        demo: c.demo === true,
      })),
    };
  });

  // Every member reads skills through the agent; this screen is for their managers.
  app.get('/admin/skills', async (request) => {
    const ctx = await access.context(requireAuth(request));
    ctx.require('skill:manage');
    const skills = await withTenant(db, ctx.subject.tenantId, (tx) => listSkills(tx));
    return { skills };
  });
}
