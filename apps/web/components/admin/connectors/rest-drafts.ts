import type { HttpMethod, ParameterLocation, ParameterType, RestOperation } from '@/lib/api';

/**
 * A REST API's operations as the form edits them (ADR-112, ADR-113): each
 * operation and parameter keeps a key of its own, so that removing one
 * leaves the others' fields where they are.
 */
export interface ParameterDraft {
  key: number;
  name: string;
  location: ParameterLocation;
  kind: ParameterType;
  required: boolean;
  description: string;
}

export interface OperationDraft {
  key: number;
  name: string;
  title: string;
  description: string;
  method: HttpMethod;
  path: string;
  parameters: ParameterDraft[];
}

export const METHODS: HttpMethod[] = ['GET', 'POST', 'PUT', 'PATCH', 'DELETE'];

/** What one API may declare, as the platform checks it (ADR-112). */
export const MAX_OPERATIONS = 50;

export const LOCATIONS: Record<ParameterLocation, string> = {
  path: 'Chemin',
  query: 'Requête',
  body: 'Corps (JSON)',
};

export const KINDS: Record<ParameterType, string> = {
  string: 'Texte',
  integer: 'Entier',
  number: 'Nombre',
  boolean: 'Vrai ou faux',
};

let last = 0;
const nextKey = () => ++last;

export function emptyParameter(): ParameterDraft {
  return {
    key: nextKey(),
    name: '',
    location: 'query',
    kind: 'string',
    required: false,
    description: '',
  };
}

export function emptyOperation(): OperationDraft {
  return {
    key: nextKey(),
    name: '',
    title: '',
    description: '',
    method: 'GET',
    path: '/',
    parameters: [],
  };
}

/** Whether an operation is still as the form first showed it, nothing typed. */
export function untouched(draft: OperationDraft): boolean {
  return (
    draft.name === '' &&
    draft.title === '' &&
    draft.description === '' &&
    draft.method === 'GET' &&
    draft.path === '/' &&
    draft.parameters.length === 0
  );
}

/** The operations stored, as the form edits them. */
export function toDrafts(operations: RestOperation[]): OperationDraft[] {
  return operations.map((operation) => ({
    key: nextKey(),
    name: operation.name,
    title: operation.title,
    description: operation.description,
    method: operation.method,
    path: operation.path,
    parameters: operation.parameters.map((parameter) => ({
      key: nextKey(),
      name: parameter.name,
      location: parameter.in,
      kind: parameter.type,
      required: parameter.required ?? false,
      description: parameter.description ?? '',
    })),
  }));
}

/** The operations as the API reads them; it checks them as a whole. */
export function fromDrafts(drafts: OperationDraft[]): RestOperation[] {
  return drafts.map((draft) => ({
    name: draft.name.trim(),
    title: draft.title.trim(),
    description: draft.description.trim(),
    method: draft.method,
    path: draft.path.trim(),
    parameters: draft.parameters.map((parameter) => ({
      name: parameter.name.trim(),
      in: parameter.location,
      type: parameter.kind,
      // A path parameter always is.
      required: parameter.required || parameter.location === 'path',
      description: parameter.description.trim() || undefined,
    })),
  }));
}
