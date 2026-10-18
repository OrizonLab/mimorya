import { OPERATORS, type ConditionJson, type Operator } from './types';

/** A condition in plain French, for the policy cards. */
export function describeCondition(condition: unknown, labels: Map<string, string>): string {
  const c = (condition ?? {}) as ConditionJson;
  const part = (items: unknown, joiner: string) =>
    Array.isArray(items) ? items.map((i) => describeCondition(i, labels)).join(joiner) : '';
  if ('all' in c) return part(c.all, ' et ');
  if ('any' in c) return `(${part(c.any, ' ou ')})`;
  if ('not' in c) return `non (${describeCondition(c.not, labels)})`;
  const operand = (o: unknown) => {
    const value = o as { attr?: string; value?: unknown } | undefined;
    if (value?.attr) return labels.get(value.attr) ?? value.attr;
    return JSON.stringify(value?.value);
  };
  const op = OPERATORS[c.op as Operator] ?? String(c.op);
  return c.op === 'exists'
    ? `${operand(c.left)} ${op}`
    : `${operand(c.left)} ${op} ${operand(c.right)}`;
}
