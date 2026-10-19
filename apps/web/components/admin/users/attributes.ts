/** Collaborator attributes as the API stores them, and the rows of the editor. */

export type AttributeKind = 'text' | 'number' | 'boolean' | 'list';

export const ATTRIBUTE_KINDS: Record<AttributeKind, string> = {
  text: 'Texte',
  number: 'Nombre',
  boolean: 'Oui / non',
  list: 'Liste de textes',
};

export interface AttributeRow {
  key: string;
  kind: AttributeKind;
  /** As typed; a list holds one item per line. */
  value: string;
}

function toRow(key: string, value: unknown): AttributeRow | null {
  if (typeof value === 'string') return { key, kind: 'text', value };
  // An integer beyond 2^53 was already rounded when read: never written back.
  if (typeof value === 'number' && !(Number.isInteger(value) && !Number.isSafeInteger(value))) {
    return { key, kind: 'number', value: String(value) };
  }
  if (typeof value === 'boolean') return { key, kind: 'boolean', value: String(value) };
  if (Array.isArray(value) && value.every((item) => typeof item === 'string')) {
    return { key, kind: 'list', value: value.join('\n') };
  }
  return null;
}

/**
 * Editor rows from stored attributes, or `null` when a value has no row kind
 * (written outside the interface): editing must never change it silently.
 */
export function toRows(attributes: Record<string, unknown> | undefined): AttributeRow[] | null {
  const rows: AttributeRow[] = [];
  for (const [key, value] of Object.entries(attributes ?? {})) {
    const row = toRow(key, value);
    if (!row) return null;
    rows.push(row);
  }
  return rows;
}

/** The attributes to send, or why they cannot be sent yet. */
export function fromRows(rows: AttributeRow[]): {
  attributes?: Record<string, unknown>;
  error?: string;
} {
  const attributes: Record<string, unknown> = {};
  for (const row of rows) {
    const key = row.key.trim();
    if (!key) return { error: 'Chaque attribut porte un nom.' };
    if (Object.hasOwn(attributes, key)) {
      return { error: `L’attribut « ${key} » figure deux fois.` };
    }
    if (row.kind === 'number') {
      const number = Number(row.value.trim());
      if (row.value.trim() === '' || !Number.isFinite(number)) {
        return { error: `« ${key} » : nombre attendu.` };
      }
      attributes[key] = number;
    } else if (row.kind === 'boolean') {
      attributes[key] = row.value === 'true';
    } else if (row.kind === 'list') {
      attributes[key] = row.value
        .split('\n')
        .map((item) => item.trim())
        .filter(Boolean);
    } else {
      attributes[key] = row.value;
    }
  }
  return { attributes };
}
