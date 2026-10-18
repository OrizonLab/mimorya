import { PROVIDER_KINDS, type ProviderConfig } from '@mimorya/agent';
import { z } from 'zod';

const booleanString = z
  .enum(['true', 'false', '1', '0'])
  .transform((v) => v === 'true' || v === '1');

const envSchema = z.object({
  NODE_ENV: z.enum(['development', 'test', 'production']).default('development'),
  HOST: z.string().default('0.0.0.0'),
  PORT: z.coerce.number().int().positive().default(4000),
  LOG_LEVEL: z.enum(['fatal', 'error', 'warn', 'info', 'debug', 'trace', 'silent']).default('info'),
  /** Runtime connection (role mimorya_app, subject to row-level security). */
  DATABASE_APP_URL: z.string().url(),
  /** Public origin of the web app, used to check the Origin header. */
  WEB_ORIGIN: z.string().url().default('http://localhost:3000'),
  SESSION_SECURE_COOKIE: booleanString.optional(),
  MIMORYA_ASSISTANT_NAME: z.string().min(1).default('Mimorya'),
  LLM_PROVIDER: z.enum([...PROVIDER_KINDS, 'none']).optional(),
  LLM_MODEL: z.string().optional(),
  LLM_API_KEY: z.string().optional(),
  LLM_BASE_URL: z.string().url().optional(),
  ANTHROPIC_API_KEY: z.string().optional(),
  /** Login attempts allowed per IP and per minute. */
  LOGIN_RATE_LIMIT_PER_MINUTE: z.coerce.number().int().positive().default(10),
  /** Run a model-based review when success criteria are given. */
  MIMORYA_LLM_JUDGE: booleanString.default(false),
});

export interface AppConfig {
  env: 'development' | 'test' | 'production';
  host: string;
  port: number;
  logLevel: string;
  databaseAppUrl: string;
  webOrigin: string;
  secureCookies: boolean;
  assistantName: string;
  llm: ProviderConfig | null;
  llmJudge: boolean;
  loginRateLimitPerMinute: number;
}

export function loadConfig(env: NodeJS.ProcessEnv = process.env): AppConfig {
  const parsed = envSchema.safeParse(env);
  if (!parsed.success) {
    const issues = parsed.error.issues.map((i) => `${i.path.join('.')}: ${i.message}`).join('\n');
    throw new Error(`Invalid configuration:\n${issues}`);
  }
  const e = parsed.data;
  const kind = e.LLM_PROVIDER ?? (e.ANTHROPIC_API_KEY ? 'anthropic' : 'none');
  const apiKey = e.LLM_API_KEY ?? (kind === 'anthropic' ? e.ANTHROPIC_API_KEY : undefined);
  return {
    env: e.NODE_ENV,
    host: e.HOST,
    port: e.PORT,
    logLevel: e.LOG_LEVEL,
    databaseAppUrl: e.DATABASE_APP_URL,
    webOrigin: e.WEB_ORIGIN,
    secureCookies: e.SESSION_SECURE_COOKIE ?? e.NODE_ENV === 'production',
    assistantName: e.MIMORYA_ASSISTANT_NAME,
    llm:
      kind === 'none'
        ? null
        : {
            kind,
            ...(e.LLM_MODEL ? { model: e.LLM_MODEL } : {}),
            ...(apiKey ? { apiKey } : {}),
            ...(e.LLM_BASE_URL ? { baseURL: e.LLM_BASE_URL } : {}),
          },
    llmJudge: e.MIMORYA_LLM_JUDGE,
    loginRateLimitPerMinute: e.LOGIN_RATE_LIMIT_PER_MINUTE,
  };
}
