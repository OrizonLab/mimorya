/** Access policies as the API lists them, and the simple form of a condition. */

export type ConditionJson = Record<string, unknown>;

export interface PolicyView {
  id: string;
  description: string;
  effect: 'allow' | 'deny';
  permissions: string[];
  condition: ConditionJson;
  enabled: boolean;
  /** Only deny policies are written from the administration (ADR-022). */
  editable: boolean;
  updatedAt: string;
}

export interface PoliciesView {
  policies: PolicyView[];
  /** The permissions a policy may restrict: business ones the platform checks. */
  permissions: Array<{ permission: string; label: string; perResource: boolean }>;
  /** Domains a policy may restrict as a whole (`domaine:*`). */
  domains: string[];
  attributes: Array<{ path: string; label: string }>;
}

export const OPERATORS = {
  eq: 'est égal à',
  neq: 'est différent de',
  in: 'fait partie de',
  not_in: 'ne fait pas partie de',
  contains: 'contient',
  gt: '>',
  gte: '≥',
  lt: '<',
  lte: '≤',
  exists: 'est renseigné',
} as const;

export type Operator = keyof typeof OPERATORS;

export interface Row {
  attr: string;
  op: Operator;
  /** As typed; lists are comma-separated. */
  value: string;
}

/** A flat condition: every row (`all`) or at least one (`any`). */
export interface SimpleCondition {
  mode: 'all' | 'any';
  rows: Row[];
}

const NUMBER = /^-?\d+(\.\d+)?$/;

/** A typed value: `12` is a number, `true` a boolean, `"12"` (quoted) a text. */
function parseScalar(text: string): unknown {
  const value = text.trim();
  if (value.length >= 2 && value.startsWith('"') && value.endsWith('"')) return value.slice(1, -1);
  if (NUMBER.test(value)) return Number(value);
  if (value === 'true' || value === 'false') return value === 'true';
  return value;
}

function formatScalar(value: unknown): string {
  if (typeof value !== 'string') return JSON.stringify(value);
  // Text that would read back as something else keeps its quotes.
  return parseScalar(value) === value ? value : `"${value}"`;
}

/** JSON with sorted keys, to compare two conditions. */
function canonical(value: unknown): string {
  if (Array.isArray(value)) return `[${value.map(canonical).join(',')}]`;
  if (value && typeof value === 'object') {
    const entries = Object.entries(value as Record<string, unknown>).sort(([a], [b]) =>
      a.localeCompare(b),
    );
    return `{${entries.map(([k, v]) => `${JSON.stringify(k)}:${canonical(v)}`).join(',')}}`;
  }
  return JSON.stringify(value);
}

/** The JSON condition the API stores, from the simple form. */
export function fromSimple(simple: SimpleCondition): ConditionJson {
  const comparisons = simple.rows.map((row) => {
    const left = { attr: row.attr.trim() };
    if (row.op === 'exists') return { op: row.op, left };
    const value =
      row.op === 'in' || row.op === 'not_in'
        ? row.value
            .split(',')
            .map((item) => item.trim())
            .filter(Boolean)
            .map(parseScalar)
        : parseScalar(row.value);
    return { op: row.op, left, right: { value } };
  });
  return { [simple.mode]: comparisons };
}

/**
 * The simple form of a condition, or `null` when it needs the JSON editor:
 * nested, or not given back exactly by the simple form (a list item holding a
 * comma, a `null` value…). Editing must never change a condition silently.
 */
export function toSimple(condition: ConditionJson): SimpleCondition | null {
  const simple = readSimple(condition);
  if (!simple) return null;
  const original = 'all' in condition || 'any' in condition ? condition : { all: [condition] };
  return canonical(fromSimple(simple)) === canonical(original) ? simple : null;
}

function readSimple(condition: ConditionJson): SimpleCondition | null {
  const mode = 'all' in condition ? 'all' : 'any' in condition ? 'any' : null;
  const items = mode ? condition[mode] : [condition];
  if (!Array.isArray(items)) return null;
  const rows: Row[] = [];
  for (const item of items as Array<Record<string, unknown>>) {
    const left = item.left as { attr?: unknown } | undefined;
    const right = item.right as { value?: unknown } | undefined;
    if (typeof item.op !== 'string' || !(item.op in OPERATORS)) return null;
    if (typeof left?.attr !== 'string') return null;
    if (right && !('value' in right)) return null;
    const value = right?.value;
    rows.push({
      attr: left.attr,
      op: item.op as Operator,
      value: Array.isArray(value)
        ? value.map(formatScalar).join(', ')
        : value === undefined
          ? ''
          : formatScalar(value),
    });
  }
  return { mode: mode ?? 'all', rows };
}
