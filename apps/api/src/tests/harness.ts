/**
 * Shared harness for the API integration tests: a fresh test database, one
 * app with a scripted model and one without any model, and helpers to sign
 * in as a demo user and send requests with the session cookie.
 */
import { ScriptedProvider, type ScriptedStep } from '@mimorya/agent/testing';
import { isoDate, mondayOf } from '@mimorya/connectors';
import { DEMO_USERS } from '@mimorya/core/demo';
import { resetTestDatabase } from '@mimorya/db/testing';
import type { FastifyInstance, LightMyRequestResponse } from 'fastify';
import { afterAll, beforeAll, expect } from 'vitest';
import { buildApp } from '../app';
import { hashPassword } from '../auth/credentials';
import { loadConfig } from '../config';

export const PASSWORD = 'mot-de-passe-de-test';
export const WEB_ORIGIN = 'http://localhost:3000';

export const EMAILS: Record<string, string> = {
  [DEMO_USERS.marc]: 'marc.lefevre@neo-maintenance.example',
  [DEMO_USERS.julien]: 'julien.moreau@neo-maintenance.example',
  [DEMO_USERS.lucas]: 'lucas.petit@neo-maintenance.example',
  [DEMO_USERS.paul]: 'paul.renaud@neo-maintenance.example',
};

const DAY_MS = 86_400_000;
const monday = mondayOf(new Date());
/** Current working week, where the demo interventions are dated. */
export const WEEK = { from: isoDate(monday), to: isoDate(new Date(monday.getTime() + 4 * DAY_MS)) };
export const NEXT_MONDAY = isoDate(new Date(monday.getTime() + 7 * DAY_MS));

export const json = <T = Record<string, unknown>>(res: LightMyRequestResponse) => res.json() as T;

export interface TestApi {
  /** App backed by the scripted model; push the model's answers to `steps`. */
  readonly app: FastifyInstance;
  /** App without any model provider configured. */
  readonly appWithoutLlm: FastifyInstance;
  readonly steps: ScriptedStep[];
  login(userId: string, target?: FastifyInstance): Promise<string>;
  as(session: string, target?: FastifyInstance): SessionClient;
}

export interface SessionClient {
  get(url: string): Promise<LightMyRequestResponse>;
  post(url: string, payload?: object): Promise<LightMyRequestResponse>;
  delete(url: string): Promise<LightMyRequestResponse>;
}

async function build(provider: ScriptedProvider | null): Promise<FastifyInstance> {
  const config = loadConfig({
    NODE_ENV: 'test',
    LOG_LEVEL: 'silent',
    DATABASE_APP_URL:
      process.env.TEST_DATABASE_APP_URL ??
      'postgres://mimorya_app:mimorya_app@localhost:5432/mimorya_test',
    WEB_ORIGIN,
    LLM_PROVIDER: 'none',
    LOGIN_RATE_LIMIT_PER_MINUTE: '1000',
  });
  return buildApp(config, { provider });
}

/** Registers the suite's setup and teardown, and returns the harness. */
export function useTestApi(): TestApi {
  const steps: ScriptedStep[] = [];
  let app: FastifyInstance | undefined;
  let appWithoutLlm: FastifyInstance | undefined;

  beforeAll(async () => {
    await resetTestDatabase({ passwordHash: await hashPassword(PASSWORD) });
    app = await build(new ScriptedProvider(steps));
    appWithoutLlm = await build(null);
  });
  afterAll(async () => {
    await app?.close();
    await appWithoutLlm?.close();
  });

  const api: TestApi = {
    get app() {
      if (!app) throw new Error('Test API not started');
      return app;
    },
    get appWithoutLlm() {
      if (!appWithoutLlm) throw new Error('Test API not started');
      return appWithoutLlm;
    },
    steps,
    async login(userId, target = api.app) {
      const res = await target.inject({
        method: 'POST',
        url: '/auth/login',
        headers: { 'x-mimorya-request': '1', origin: WEB_ORIGIN },
        payload: { email: EMAILS[userId], password: PASSWORD },
      });
      expect(res.statusCode).toBe(200);
      const cookie = res.cookies.find((c) => c.name === 'mimorya_session');
      expect(cookie?.httpOnly).toBe(true);
      return cookie!.value;
    },
    as(session, target = api.app) {
      const headers = { 'x-mimorya-request': '1' };
      const cookies = { mimorya_session: session };
      return {
        get: (url) => target.inject({ method: 'GET', url, cookies }),
        post: (url, payload) =>
          target.inject({ method: 'POST', url, payload: payload ?? {}, headers, cookies }),
        delete: (url) => target.inject({ method: 'DELETE', url, headers, cookies }),
      };
    },
  };
  return api;
}
