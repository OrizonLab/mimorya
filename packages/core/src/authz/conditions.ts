/**
 * Minimal ABAC condition language, serialisable as JSON so that tenants can
 * store policies in the database. Deliberately small: no functions, no
 * regex, no arbitrary code — conditions must be auditable by an administrator.
 *
 * Attributes are addressed by dotted paths in an evaluation environment:
 *   subject.*  (userId, orgUnitIds, clearanceRank, attributes.*)
 *   resource.* (type, ownerUserId, orgUnitId, sensitivityRank, attributes.*)
 *   context.*  (actorType, channel, …)
 */
import { z } from 'zod';

export type Operand = { attr: string } | { value: unknown };

export type ComparisonOp =
  'eq' | 'neq' | 'in' | 'not_in' | 'contains' | 'gt' | 'gte' | 'lt' | 'lte' | 'exists';

export type Condition =
  | { all: Condition[] }
  | { any: Condition[] }
  | { not: Condition }
  | { op: ComparisonOp; left: Operand; right?: Operand };

const operandSchema: z.ZodType<Operand> = z.union([
  z.object({ attr: z.string().min(1) }).strict(),
  z.object({ value: z.unknown() }).strict(),
]);

export const conditionSchema: z.ZodType<Condition> = z.lazy(() =>
  z.union([
    z.object({ all: z.array(conditionSchema) }).strict(),
    z.object({ any: z.array(conditionSchema) }).strict(),
    z.object({ not: conditionSchema }).strict(),
    z
      .object({
        op: z.enum(['eq', 'neq', 'in', 'not_in', 'contains', 'gt', 'gte', 'lt', 'lte', 'exists']),
        left: operandSchema,
        right: operandSchema.optional(),
      })
      .strict(),
  ]),
);

export type EvaluationEnv = Record<string, unknown>;

function resolvePath(env: EvaluationEnv, path: string): unknown {
  let current: unknown = env;
  for (const key of path.split('.')) {
    if (current === null || typeof current !== 'object') return undefined;
    // Only own properties: never walk into prototypes.
    if (!Object.prototype.hasOwnProperty.call(current, key)) return undefined;
    current = (current as Record<string, unknown>)[key];
  }
  return current;
}

function resolve(env: EvaluationEnv, operand: Operand | undefined): unknown {
  if (operand === undefined) return undefined;
  return 'attr' in operand ? resolvePath(env, operand.attr) : operand.value;
}

function toArray(value: unknown): unknown[] | undefined {
  if (Array.isArray(value)) return value;
  if (value instanceof Set) return [...value];
  return undefined;
}

function compareNumbers(a: unknown, b: unknown, fn: (x: number, y: number) => boolean): boolean {
  return typeof a === 'number' && typeof b === 'number' && fn(a, b);
}

/** Evaluates a condition. Unknown attributes resolve to `undefined` and never match. */
export function evaluateCondition(condition: Condition, env: EvaluationEnv): boolean {
  if ('all' in condition) return condition.all.every((c) => evaluateCondition(c, env));
  if ('any' in condition) return condition.any.some((c) => evaluateCondition(c, env));
  if ('not' in condition) return !evaluateCondition(condition.not, env);

  const left = resolve(env, condition.left);
  const right = resolve(env, condition.right);

  switch (condition.op) {
    case 'exists':
      return left !== undefined && left !== null;
    case 'eq':
      return left !== undefined && left === right;
    case 'neq':
      return left !== undefined && left !== right;
    case 'in': {
      const list = toArray(right);
      return left !== undefined && list !== undefined && list.includes(left);
    }
    case 'not_in': {
      const list = toArray(right);
      return left !== undefined && list !== undefined && !list.includes(left);
    }
    case 'contains': {
      const list = toArray(left);
      return list !== undefined && right !== undefined && list.includes(right);
    }
    case 'gt':
      return compareNumbers(left, right, (x, y) => x > y);
    case 'gte':
      return compareNumbers(left, right, (x, y) => x >= y);
    case 'lt':
      return compareNumbers(left, right, (x, y) => x < y);
    case 'lte':
      return compareNumbers(left, right, (x, y) => x <= y);
  }
}
