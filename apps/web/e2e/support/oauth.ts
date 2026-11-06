import type { Page } from '@playwright/test';
import type { Connectors } from '../../lib/api/contract';
import { expect } from './fixtures';

/**
 * The MCP server protected by OAuth that the end-to-end API serves, its
 * authorization server, and the client that server knows, with its secret
 * (`e2e_server`, ADR-171).
 */
export const OAUTH_SERVER = 'http://127.0.0.1:4107/mcp';
export const OAUTH_ISSUER = 'http://127.0.0.1:4106';
export const OAUTH_CLIENT_ID = 'mimorya-test-client';
export const OAUTH_CLIENT_SECRET = 'secret-du-client-de-test';

/** The company's connectors, as the API lists them to its administrators. */
export async function listedConnectors(page: Page) {
  const response = await page.request.get('/api/admin/connectors');
  expect(response.status()).toBe(200);
  return ((await response.json()) as Connectors).instances;
}
