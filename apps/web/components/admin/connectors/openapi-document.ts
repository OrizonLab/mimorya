/**
 * What the platform reads of an OpenAPI description (ADR-114, ADR-115): a
 * JSON document as written. A YAML one is converted here, in the browser of
 * the administrator who chose it, so that no YAML reader runs on the server;
 * its reader loads only when one is chosen.
 */

import { ApiError } from '@/lib/api';

/** As the platform bounds a description: larger, it is not read. */
export const MAX_DESCRIPTION_BYTES = 768 * 1024;

/** A refusal said as the platform says its own. */
const unreadable = (message: string) => new ApiError(400, 'unreadable', message);

/** The description in JSON, or why it cannot be read (in French). */
export async function descriptionDocument(file: File): Promise<string> {
  if (file.size > MAX_DESCRIPTION_BYTES) {
    throw unreadable('Ce fichier dépasse 768 Kio : il n’est pas lu.');
  }
  const text = await file.text();
  if (text.trimStart().startsWith('{')) return text;
  const { parse, YAMLParseError } = await import('yaml');
  let read: unknown;
  try {
    // Aliases repeated beyond a hundred are refused: a short document
    // cannot unfold into a huge one.
    read = parse(text, { maxAliasCount: 100 });
  } catch (error) {
    const line = error instanceof YAMLParseError ? error.linePos?.[0]?.line : undefined;
    throw unreadable(
      line
        ? `Description YAML illisible (ligne ${line}) : vérifiez le fichier.`
        : 'Description illisible : un document JSON ou YAML est attendu.',
    );
  }
  if (typeof read !== 'object' || read === null || Array.isArray(read)) {
    throw unreadable('Description illisible : un document JSON ou YAML est attendu.');
  }
  const json = JSON.stringify(read);
  if (new TextEncoder().encode(json).length > MAX_DESCRIPTION_BYTES) {
    throw unreadable('Une fois convertie en JSON, cette description dépasse 768 Kio.');
  }
  return json;
}
