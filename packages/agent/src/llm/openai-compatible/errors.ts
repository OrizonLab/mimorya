/** Maps OpenAI SDK errors to the provider-neutral error taxonomy. */
import OpenAI from 'openai';
import { LlmError } from '../types';

export function toLlmError(provider: string, error: unknown): LlmError {
  if (error instanceof LlmError) return error;
  const opts = { cause: error };
  if (error instanceof OpenAI.APIUserAbortError) {
    return new LlmError(provider, 'aborted', 'Requête annulée', opts);
  }
  if (
    error instanceof OpenAI.AuthenticationError ||
    error instanceof OpenAI.PermissionDeniedError
  ) {
    return new LlmError(provider, 'auth', 'Authentification refusée par le fournisseur', opts);
  }
  if (error instanceof OpenAI.RateLimitError) {
    return new LlmError(provider, 'rate_limit', 'Limite de débit atteinte', opts);
  }
  if (error instanceof OpenAI.BadRequestError) {
    const overflow = /context length|too long|maximum context/i.test(error.message);
    return new LlmError(
      provider,
      overflow ? 'context_overflow' : 'invalid_request',
      error.message,
      opts,
    );
  }
  if (error instanceof OpenAI.InternalServerError) {
    return new LlmError(provider, 'overloaded', 'Fournisseur indisponible', opts);
  }
  if (error instanceof OpenAI.APIConnectionError) {
    return new LlmError(provider, 'network', 'Connexion au fournisseur impossible', opts);
  }
  if (error instanceof OpenAI.APIError)
    return new LlmError(provider, 'unknown', error.message, opts);
  return new LlmError(
    provider,
    'unknown',
    error instanceof Error ? error.message : String(error),
    opts,
  );
}
