/**
 * Domain errors. They carry a stable `code` so transport layers (HTTP, gateways)
 * can map them without string matching, and a message safe to show end users.
 */
export type DomainErrorCode =
  | 'forbidden'
  | 'not_found'
  | 'validation'
  | 'conflict'
  | 'unauthenticated'
  | 'budget_exceeded'
  | 'unavailable';

export class DomainError extends Error {
  readonly code: DomainErrorCode;
  readonly details: Record<string, unknown> | undefined;

  constructor(code: DomainErrorCode, message: string, details?: Record<string, unknown>) {
    super(message);
    this.name = 'DomainError';
    this.code = code;
    this.details = details;
  }
}

export class ForbiddenError extends DomainError {
  constructor(message = 'Accès refusé', details?: Record<string, unknown>) {
    super('forbidden', message, details);
    this.name = 'ForbiddenError';
  }
}

export class NotFoundError extends DomainError {
  constructor(message = 'Ressource introuvable', details?: Record<string, unknown>) {
    super('not_found', message, details);
    this.name = 'NotFoundError';
  }
}

export class ValidationError extends DomainError {
  constructor(message: string, details?: Record<string, unknown>) {
    super('validation', message, details);
    this.name = 'ValidationError';
  }
}

export function isDomainError(err: unknown): err is DomainError {
  return err instanceof DomainError;
}
