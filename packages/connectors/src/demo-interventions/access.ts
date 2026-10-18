/** Authorization view of an intervention: every read goes through `ctx.authz`. */
import type { ToolContext } from '@mimorya/core';
import type { Intervention } from './dataset';

export function asResource(i: Intervention) {
  return {
    type: 'intervention',
    id: i.id,
    ownerUserId: i.technicianId,
    orgUnitId: i.teamUnitId,
    sensitivity: i.sensitivity,
    attributes: { status: i.status },
  };
}

export function canRead(ctx: ToolContext, i: Intervention): boolean {
  return ctx.authz.check('intervention:read', asResource(i)).allowed;
}
