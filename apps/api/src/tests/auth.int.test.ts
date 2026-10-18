/** Authentication, anti-CSRF protection and access to administration screens. */
import { DEMO_USERS } from '@mimorya/core/demo';
import { describe, expect, it } from 'vitest';
import { EMAILS, json, PASSWORD, useTestApi } from './harness';

const api = useTestApi();
const { as, login } = api;

describe('authentication', () => {
  it('rejects state-changing requests without the anti-CSRF header or from another origin', async () => {
    const noHeader = await api.app.inject({
      method: 'POST',
      url: '/auth/login',
      payload: { email: EMAILS[DEMO_USERS.marc], password: PASSWORD },
    });
    expect(noHeader.statusCode).toBe(403);
    const otherOrigin = await api.app.inject({
      method: 'POST',
      url: '/auth/login',
      headers: { 'x-mimorya-request': '1', origin: 'https://evil.example' },
      payload: { email: EMAILS[DEMO_USERS.marc], password: PASSWORD },
    });
    expect(otherOrigin.statusCode).toBe(403);
  });

  it('answers identically for unknown accounts and wrong passwords', async () => {
    const attempt = (email: string) =>
      api.app.inject({
        method: 'POST',
        url: '/auth/login',
        headers: { 'x-mimorya-request': '1' },
        payload: { email, password: 'incorrect' },
      });
    const wrong = await attempt(EMAILS[DEMO_USERS.marc]!);
    const unknown = await attempt('personne@inconnu.example');
    expect(wrong.statusCode).toBe(401);
    expect(unknown.statusCode).toBe(401);
    expect(wrong.json()).toEqual(unknown.json());
  });

  it('exposes effective permissions computed server-side', async () => {
    const marc = as(await login(DEMO_USERS.marc));
    const me = json<{ permissions: string[]; grants: string[] }>(await marc.get('/auth/me'));
    expect(me.permissions).toContain('intervention:read');
    expect(me.permissions).not.toContain('user:manage');
    expect(me.grants).toContain('Responsable d’équipe — Service technique et sous-unités');

    const anonymous = await api.app.inject({ method: 'GET', url: '/auth/me' });
    expect(anonymous.statusCode).toBe(401);
  });

  it('revokes the session on logout', async () => {
    const session = await login(DEMO_USERS.lucas);
    expect((await as(session).post('/auth/logout')).statusCode).toBe(200);
    expect((await as(session).get('/auth/me')).statusCode).toBe(401);
  });
});

describe('administration', () => {
  it('reserves administration screens to the matching permissions', async () => {
    const paul = as(await login(DEMO_USERS.paul));
    const marc = as(await login(DEMO_USERS.marc));
    expect((await paul.get('/admin/audit')).statusCode).toBe(200);
    expect((await marc.get('/admin/audit')).statusCode).toBe(403);
    // Reading skills is a member right; the administration screen is not.
    expect((await paul.get('/admin/skills')).statusCode).toBe(200);
    expect((await marc.get('/admin/skills')).statusCode).toBe(403);
    const users = json<{ users: Array<{ grants?: string[] }> }>(await paul.get('/admin/users'));
    expect(users.users).toHaveLength(10);
    expect(users.users[0]!.grants).toBeDefined();
  });
});
