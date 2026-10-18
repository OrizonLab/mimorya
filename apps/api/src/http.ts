import { DomainError, ValidationError } from '@mimorya/core';
import type { FastifyError, FastifyReply, FastifyRequest } from 'fastify';
import { z } from 'zod';

const STATUS: Record<DomainError['code'], number> = {
  validation: 400,
  unauthenticated: 401,
  forbidden: 403,
  not_found: 404,
  conflict: 409,
  budget_exceeded: 429,
  unavailable: 503,
};

export function parse<T>(schema: z.ZodType<T>, data: unknown): T {
  const result = schema.safeParse(data);
  if (!result.success) {
    throw new ValidationError('Requête invalide', {
      issues: result.error.issues.map((i) => ({ path: i.path.join('.'), message: i.message })),
    });
  }
  return result.data;
}

export function errorHandler(
  error: FastifyError | Error,
  request: FastifyRequest,
  reply: FastifyReply,
) {
  if (error instanceof DomainError) {
    return reply.status(STATUS[error.code]).send({
      error: {
        code: error.code,
        message: error.message,
        ...(error.details ? { details: error.details } : {}),
      },
    });
  }
  const statusCode = (error as FastifyError).statusCode;
  if (statusCode && statusCode < 500) {
    return reply.status(statusCode).send({ error: { code: 'request', message: error.message } });
  }
  request.log.error({ err: error }, 'Unhandled error');
  return reply.status(500).send({ error: { code: 'internal', message: 'Erreur interne' } });
}

/** Route parameters of the form `/:id`. */
export const idParams = z.object({ id: z.uuid() });
