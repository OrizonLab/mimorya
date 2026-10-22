// Proving the company's domains from « Connexion » (ADR-069): each domain its
// providers cover is listed with the DNS record to publish. Read before it is
// published, the proof is refused with what is missing; published in the
// end-to-end server's DNS (`DNS_ADDRESS` of `crates/api/examples/e2e_server`),
// the domain is proved and dated, as the API records it. Whatever happens, the
// provider is removed; the proof, which the API keeps, stays with the demo
// company under a domain of this test's own.
import type { IdentityProvidersView, NewIdentityProvider } from '../lib/api/contract';
import { cardWith, openAdmin } from './support/admin';
import { expect, noError, test } from './support/fixtures';

const PROVIDER = 'Annuaire preuve e2e';
const DOMAIN = 'neo-preuve.example';
const DNS = 'http://127.0.0.1:4102';
const WRITE = { 'X-Mimorya-Request': '1' };

test.afterEach(async ({ page }) => {
  // With the test's session; 401: the test never signed in, so declared nothing.
  const listed = await page.request.get('/api/admin/identity-providers');
  if (listed.status() === 401) return;
  expect(listed.status()).toBe(200);
  const { providers } = (await listed.json()) as IdentityProvidersView;
  for (const provider of providers.filter((p) => p.name === PROVIDER)) {
    const deleted = await page.request.delete(`/api/admin/identity-providers/${provider.id}`, {
      headers: WRITE,
    });
    expect(deleted.status()).toBe(200);
  }
});

test('the company proves its domain from « Connexion »', async ({ page }) => {
  await openAdmin(page, 'Connexion');
  // Declared through the API: its screens are `single-sign-on.spec.ts`'s.
  const declared = await page.request.post('/api/admin/identity-providers', {
    headers: WRITE,
    data: {
      kind: 'oidc',
      name: PROVIDER,
      issuer: 'http://127.0.0.1:4100',
      clientId: 'mimorya-tests',
      clientSecret: 'secret-client-de-test-0123456789',
      domains: [DOMAIN],
    } satisfies NewIdentityProvider,
  });
  expect(declared.status()).toBe(201);
  const read = await page.request.get('/api/admin/identity-providers');
  const { domains } = (await read.json()) as IdentityProvidersView;
  const record = domains.find((d) => d.domain === DOMAIN);
  expect(record?.verifiedAt).toBeNull();

  // Listed with its record, not proved.
  await page.reload();
  await page.getByRole('tab', { name: 'Connexion' }).click();
  const unproved = cardWith(page, DOMAIN, 'Vérifier');
  await expect(unproved.getByText('Non prouvé')).toBeVisible();
  await expect(unproved.getByText(record?.recordName ?? '?', { exact: true })).toBeVisible();
  await expect(unproved.getByText(record?.recordValue ?? '?', { exact: true })).toBeVisible();

  // Read before it is published: refused, saying what is missing.
  await unproved.getByRole('button', { name: 'Vérifier', exact: true }).click();
  await expect(
    unproved.getByText(
      `aucun enregistrement TXT ${record?.recordName} ne porte la valeur attendue`,
    ),
  ).toBeVisible();

  // Published in its DNS, then read: proved, as the API records it.
  const published = await page.request.put(`${DNS}/records/${record?.recordName}`, {
    data: record?.recordValue,
  });
  expect(published.status()).toBe(204);
  await unproved.getByRole('button', { name: 'Vérifier', exact: true }).click();
  const proved = cardWith(page, DOMAIN, 'Vérifier de nouveau');
  await expect(proved.getByText(/^Prouvé le /)).toBeVisible();
  await page.reload();
  await page.getByRole('tab', { name: 'Connexion' }).click();
  await expect(proved.getByText(/^Prouvé le /)).toBeVisible();
  const reread = await page.request.get('/api/admin/identity-providers');
  const after = (await reread.json()) as IdentityProvidersView;
  expect(after.domains.find((d) => d.domain === DOMAIN)?.verifiedAt).toEqual(expect.any(String));
  await noError(page);
});
