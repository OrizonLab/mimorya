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
  let json: string;
  try {
    // Aliases repeated beyond a hundred are refused, merge keys (`<<`)
    // included: a short document cannot unfold into a huge one. The reader
    // counts the anchor itself among them, hence one more.
    const read: unknown = parse(text, { maxAliasCount: 101, merge: true });
    if (typeof read !== 'object' || read === null || Array.isArray(read)) {
      throw unreadable('Description illisible : un document JSON ou YAML est attendu.');
    }
    json = JSON.stringify(read);
  } catch (error) {
    if (error instanceof ApiError) throw error;
    const line = error instanceof YAMLParseError ? error.linePos?.[0]?.line : undefined;
    if (line) throw unreadable(`Description YAML illisible (ligne ${line}) : vérifiez le fichier.`);
    // The reader's own refusals, each said for what it is: aliases repeated
    // too often (a map merging itself unfolds the same way), one without
    // its anchor, a merge of what is no table (a merged alias without its
    // anchor among them), or an alias holding itself, which no JSON can
    // write.
    const said = error instanceof Error ? error.message : '';
    if (error instanceof ReferenceError && said.startsWith('Excessive alias count')) {
      throw unreadable(
        'Description YAML refusée : des alias répétés plus de 100 fois, ou une fusion « << » qui se contient elle-même.',
      );
    }
    if (error instanceof ReferenceError && said.startsWith('Unresolved alias')) {
      throw unreadable('Description YAML illisible : un alias sans ancre définie avant lui.');
    }
    if (said.startsWith('Merge sources must be')) {
      throw unreadable(
        'Description YAML illisible : une clé « << » qui ne fusionne pas une table, ou dont l’alias n’a pas d’ancre définie avant lui.',
      );
    }
    if (error instanceof TypeError) {
      throw unreadable('Description YAML refusée : un alias qui se contient lui-même.');
    }
    throw unreadable('Description illisible : un document JSON ou YAML est attendu.');
  }
  if (new TextEncoder().encode(json).length > MAX_DESCRIPTION_BYTES) {
    throw unreadable('Une fois convertie en JSON, cette description dépasse 768 Kio.');
  }
  return json;
}
