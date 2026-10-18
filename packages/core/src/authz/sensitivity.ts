/** Data classification levels, ordered from least to most sensitive. */
export const SENSITIVITY_LEVELS = ['public', 'internal', 'confidential', 'restricted'] as const;
export type Sensitivity = (typeof SENSITIVITY_LEVELS)[number];

export function sensitivityRank(level: Sensitivity): number {
  return SENSITIVITY_LEVELS.indexOf(level);
}

export function isSensitivity(value: unknown): value is Sensitivity {
  return typeof value === 'string' && (SENSITIVITY_LEVELS as readonly string[]).includes(value);
}
