/**
 * CSRF protection: state-changing requests must carry a custom header (not
 * settable cross-site without CORS, which is disabled) and, when present, an
 * Origin matching the web application.
 */
import { DomainError } from '@mimorya/core';
import type { FastifyInstance } from 'fastify';

export const REQUEST_HEADER = 'x-mimorya-request';
const SAFE_METHODS = new Set(['GET', 'HEAD', 'OPTIONS']);

export function registerCsrfProtection(app: FastifyInstance, webOrigin: string): void {
  app.addHook('onRequest', async (request) => {
    if (SAFE_METHODS.has(request.method)) return;
    if (request.headers[REQUEST_HEADER] !== '1') {
      throw new DomainError('forbidden', 'En-tête de requête manquant');
    }
    const origin = request.headers.origin;
    if (origin && origin !== webOrigin) {
      throw new DomainError('forbidden', 'Origine non autorisée');
    }
  });
}
