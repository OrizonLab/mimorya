/**
 * Tool outputs are data, never instructions. They are serialised inside an
 * explicit envelope that the system prompt tells the model to treat as
 * untrusted. Any attempt to close the envelope from inside the payload is
 * neutralised.
 */
const OPEN = '<donnees_outil';
const CLOSE = '</donnees_outil>';

export function wrapUntrusted(toolName: string, payload: unknown): string {
  const body = typeof payload === 'string' ? payload : JSON.stringify(payload, null, 1);
  const safe = body.replaceAll(CLOSE, '<\\/donnees_outil>').replaceAll(OPEN, '<\\donnees_outil');
  return `${OPEN} source="${toolName}" fiabilite="non_fiable">\n${safe}\n${CLOSE}`;
}

/** Cap a tool payload to keep the context bounded. */
export function truncate(text: string, maxChars = 24_000): string {
  if (text.length <= maxChars) return text;
  return `${text.slice(0, maxChars)}\n… [résultat tronqué : ${text.length - maxChars} caractères omis]`;
}
