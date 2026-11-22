import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { defineConfig, devices } from '@playwright/test';

/**
 * End-to-end tests (ADR-031): the real API, started with a scripted model
 * (`cargo run --example e2e_server`) on a freshly seeded database, behind the
 * production build of the web application. One worker and one pass per
 * start, without retry: the tests share the demo company, which a pass uses
 * up. A shard of the suite (`--shard`) has a start of its own, whatever ran
 * in the others (ADR-199). With `E2E_PREBUILT` set, the API's programs are
 * the ones already built (`cargo build`, as the command below builds them):
 * the CI builds them once for all its shards.
 */
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
export const MAIL_DIR = path.join(root, 'target/e2e/mail');

/** The seed, then the API: built here, or already built (`E2E_PREBUILT`). */
const apiCommand = process.env.E2E_PREBUILT
  ? 'target/debug/mimorya-seed --reset && target/debug/examples/e2e_server'
  : 'cargo build -p mimorya-api --bin mimorya-extract && cargo run --bin mimorya-seed -- --reset && cargo run -p mimorya-api --example e2e_server';

const databaseUrl =
  process.env.E2E_DATABASE_URL ?? 'postgres://mimorya:mimorya@localhost:5432/mimorya';
const databaseAppUrl =
  process.env.E2E_DATABASE_APP_URL ?? 'postgres://mimorya_app:mimorya_app@localhost:5432/mimorya';

export default defineConfig({
  testDir: './e2e',
  workers: 1,
  fullyParallel: false,
  forbidOnly: !!process.env.CI,
  retries: 0,
  timeout: 60_000,
  reporter: process.env.CI ? [['list'], ['github']] : 'list',
  use: {
    baseURL: 'http://localhost:3000',
    trace: 'retain-on-failure',
    screenshot: 'only-on-failure',
  },
  projects: [{ name: 'chromium', use: { ...devices['Desktop Chrome'] } }],
  webServer: [
    {
      // The demo company is recreated, then the API starts (it deletes the
      // e-mails of a former start), its documents read by the extraction
      // process (ADR-095).
      command: apiCommand,
      cwd: root,
      url: 'http://localhost:4000/health',
      timeout: 900_000,
      reuseExistingServer: false,
      stdout: 'pipe',
      env: {
        DATABASE_URL: databaseUrl,
        DATABASE_APP_URL: databaseAppUrl,
        DEMO_PASSWORD: 'demo-mimorya',
        WEB_ORIGIN: 'http://localhost:3000',
        // The platform's model as the operator configures it: the server
        // serves it scripted, and builds a supplied company's escalation from
        // this configuration (ADR-209), by a scripted factory too.
        LLM_PROVIDER: 'anthropic',
        ANTHROPIC_API_KEY: 'cle-exploitant-e2e',
        // The operator opens Anthropic's advisor to its companies (ADR-207).
        MIMORYA_ANTHROPIC_ADVISOR: 'true',
        // …and TypeSafe's Jev, asked at a local TypeSafe (ADR-213).
        MIMORYA_TYPESAFE_JEV: 'true',
        // A public test key (`PUBLIC_TEST_MASTER_KEYS`, refused in production),
        // so that the administration can store secrets (a token, an API key).
        MIMORYA_MASTER_KEY: 'AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8=',
        LOGIN_RATE_LIMIT_PER_MINUTE: '1000',
        // A brand of its own, as an installation sets it (ADR-222): every
        // page shows it, `brand.spec.ts` says where.
        MIMORYA_PRODUCT_NAME: 'Atelier Nova',
        MIMORYA_ASSISTANT_NAME: 'Nova',
        MIMORYA_ACCENT_LIGHT: '#6b2fa3',
        MIMORYA_ACCENT_DARK: '#c4a3e8',
        MIMORYA_BRAND_DIR: path.join(root, 'apps/web/e2e/brand'),
        LOG_LEVEL: 'warn',
        E2E_MAIL_DIR: MAIL_DIR,
        // Beside the API's binaries, not the example's (`target/debug/examples`).
        MIMORYA_EXTRACTOR: path.join(root, 'target/debug/mimorya-extract'),
      },
    },
    {
      // The production build (`next build`), as served to users.
      command: 'pnpm start',
      url: 'http://localhost:3000/login',
      timeout: 120_000,
      reuseExistingServer: false,
      env: { API_URL: 'http://localhost:4000', NEXT_TELEMETRY_DISABLED: '1' },
    },
  ],
});
