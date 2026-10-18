import { describe, expect, it } from 'vitest';
import { decideExecution, type AutonomyLevel, type ToolRisk } from './autonomy';

const tool = (risk: ToolRisk, requiresApproval = false) => ({
  name: `tool_${risk}`,
  risk,
  requiresApproval,
});

describe('decideExecution', () => {
  const matrix: Array<[AutonomyLevel, ToolRisk, 'allow' | 'require_approval' | 'deny']> = [
    ['read_only', 'read', 'allow'],
    ['read_only', 'draft', 'deny'],
    ['read_only', 'write', 'deny'],
    ['propose', 'draft', 'allow'],
    ['propose', 'write', 'deny'],
    ['propose', 'external', 'deny'],
    ['execute_with_approval', 'read', 'allow'],
    ['execute_with_approval', 'draft', 'allow'],
    ['execute_with_approval', 'write', 'require_approval'],
    ['execute_with_approval', 'external', 'require_approval'],
    ['execute_with_approval', 'irreversible', 'require_approval'],
    ['autonomous_limited', 'write', 'require_approval'],
    ['autonomous_limited', 'irreversible', 'require_approval'],
  ];

  it.each(matrix)('%s × %s → %s', (level, risk, expected) => {
    expect(decideExecution({ level }, tool(risk)).kind).toBe(expected);
  });

  it('runs allow-listed tools autonomously, except irreversible ones', () => {
    const policy = {
      level: 'autonomous_limited' as const,
      autonomousTools: new Set(['tool_write', 'tool_irreversible']),
    };
    expect(decideExecution(policy, tool('write')).kind).toBe('allow');
    expect(decideExecution(policy, tool('irreversible')).kind).toBe('require_approval');
  });

  it('honours tools that always require approval', () => {
    expect(decideExecution({ level: 'autonomous_limited' }, tool('draft', true)).kind).toBe(
      'require_approval',
    );
  });
});
