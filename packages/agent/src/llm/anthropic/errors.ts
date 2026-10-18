/** Maps Anthropic SDK errors to the provider-neutral error taxonomy. */
import Anthropic from '@anthropic-ai/sdk';
import { LlmError } from '../types';

export function toLlmError(error: unknown): LlmError {
  if (error instanceof LlmError) return error;
  const opts = { cause: error };
  if (error instanceof Anthropic.APIUserAbortError) {
    return new LlmError('anthropic', 'aborted', 'Requête annulée', opts);
  }
  if (
    error instanceof Anthropic.AuthenticationError ||
    error instanceof Anthropic.PermissionDeniedError
  ) {
    return new LlmError('anthropic', 'auth', 'Authentification refusée par Anthropic', opts);
  }
  if (error instanceof Anthropic.RateLimitError) {
    return new LlmError('anthropic', 'rate_limit', 'Limite de débit Anthropic atteinte', opts);
  }
  if (error instanceof Anthropic.BadRequestError) {
    const overflow = /prompt is too long|context window/i.test(error.message);
    return new LlmError(
      'anthropic',
      overflow ? 'context_overflow' : 'invalid_request',
      error.message,
      opts,
    );
  }
  if (error instanceof Anthropic.InternalServerError) {
    return new LlmError('anthropic', 'overloaded', 'Service Anthropic indisponible', opts);
  }
  if (error instanceof Anthropic.APIConnectionError) {
    return new LlmError('anthropic', 'network', 'Connexion à Anthropic impossible', opts);
  }
  if (error instanceof Anthropic.APIError) {
    return new LlmError('anthropic', 'unknown', error.message, opts);
  }
  // Not an API error: the stream could not be parsed (e.g. malformed tool input JSON).
  return new LlmError('anthropic', 'malformed_output', 'Réponse du modèle illisible', opts);
}
