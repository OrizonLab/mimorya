import { describe, expect, it } from 'vitest';
import { conditionSchema, evaluateCondition, type Condition } from './conditions';

const env = {
  subject: { userId: 'u1', orgUnitIds: ['t1', 't2'], clearanceRank: 1, attributes: {} },
  resource: { type: 'doc', sensitivityRank: 2, attributes: { status: 'open', tags: ['a'] } },
  context: { actorType: 'agent' },
};

describe('evaluateCondition', () => {
  it.each<[string, Condition, boolean]>([
    ['eq', { op: 'eq', left: { attr: 'resource.type' }, right: { value: 'doc' } }, true],
    [
      'neq',
      { op: 'neq', left: { attr: 'resource.attributes.status' }, right: { value: 'closed' } },
      true,
    ],
    ['in', { op: 'in', left: { value: 't2' }, right: { attr: 'subject.orgUnitIds' } }, true],
    [
      'not_in',
      { op: 'not_in', left: { value: 't9' }, right: { attr: 'subject.orgUnitIds' } },
      true,
    ],
    [
      'contains',
      { op: 'contains', left: { attr: 'resource.attributes.tags' }, right: { value: 'a' } },
      true,
    ],
    [
      'gt',
      {
        op: 'gt',
        left: { attr: 'resource.sensitivityRank' },
        right: { attr: 'subject.clearanceRank' },
      },
      true,
    ],
    ['lte', { op: 'lte', left: { attr: 'resource.sensitivityRank' }, right: { value: 1 } }, false],
    ['exists', { op: 'exists', left: { attr: 'context.actorType' } }, true],
  ])('supports %s', (_name, condition, expected) => {
    expect(evaluateCondition(condition, env)).toBe(expected);
  });

  it('combines with all / any / not', () => {
    const c: Condition = {
      all: [
        { op: 'eq', left: { attr: 'context.actorType' }, right: { value: 'agent' } },
        { not: { op: 'eq', left: { attr: 'resource.type' }, right: { value: 'secret' } } },
        {
          any: [
            { op: 'eq', left: { attr: 'subject.userId' }, right: { value: 'nobody' } },
            { op: 'exists', left: { attr: 'resource.attributes.status' } },
          ],
        },
      ],
    };
    expect(evaluateCondition(c, env)).toBe(true);
  });

  it('never matches unknown attributes and does not walk prototypes', () => {
    expect(
      evaluateCondition(
        { op: 'eq', left: { attr: 'subject.missing' }, right: { value: undefined } },
        env,
      ),
    ).toBe(false);
    expect(evaluateCondition({ op: 'exists', left: { attr: 'subject.constructor' } }, env)).toBe(
      false,
    );
    expect(evaluateCondition({ op: 'exists', left: { attr: 'resource.type.length' } }, env)).toBe(
      false,
    );
  });

  it('does not compare numbers with non numbers', () => {
    expect(evaluateCondition({ op: 'gt', left: { value: '3' }, right: { value: 1 } }, env)).toBe(
      false,
    );
  });

  it('validates the JSON representation', () => {
    expect(
      conditionSchema.safeParse({ op: 'eq', left: { attr: 'a' }, right: { value: 1 } }).success,
    ).toBe(true);
    expect(conditionSchema.safeParse({ op: 'regex', left: { attr: 'a' } }).success).toBe(false);
    expect(conditionSchema.safeParse({ all: [{ op: 'eq', left: { code: 'x' } }] }).success).toBe(
      false,
    );
  });
});
