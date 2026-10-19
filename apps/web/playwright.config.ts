import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { defineConfig, devices } from '@playwright/test';

/**
 * End-to-end tests (ADR-031): the real API, started with a scripted model
 * (`cargo run --example e2e_server`) on a freshly seeded database, behind the
 * production build of the web application. One worker and one pass per
 * start, without retry: the tests share the demo company and the model's
 * script, which a pass uses up.
 */
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
export const MAIL_DIR = path.join(root, 'target/e2e/mail');

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
      // e-mails of a former start).
      command:
        'cargo run --bin mimorya-seed -- --reset && cargo run -p mimorya-api --example e2e_server',
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
        LLM_PROVIDER: 'none',
        LOGIN_RATE_LIMIT_PER_MINUTE: '1000',
        LOG_LEVEL: 'warn',
        E2E_MAIL_DIR: MAIL_DIR,
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
