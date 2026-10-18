import { createHash, randomBytes } from 'node:crypto';
import { hash, verify } from '@node-rs/argon2';

// OWASP-recommended Argon2id parameters (19 MiB, 2 iterations).
const ARGON2_OPTIONS = { memoryCost: 19_456, timeCost: 2, parallelism: 1 } as const;

export function hashPassword(password: string): Promise<string> {
  return hash(password, ARGON2_OPTIONS);
}

// Verified against when the account does not exist, so that response time does
// not reveal whether an email is registered.
let dummyHash: Promise<string> | undefined;

export async function verifyPassword(
  passwordHash: string | null,
  password: string,
): Promise<boolean> {
  if (!passwordHash) {
    dummyHash ??= hashPassword('dummy-password-for-timing');
    await verify(await dummyHash, password).catch(() => false);
    return false;
  }
  return verify(passwordHash, password).catch(() => false);
}

export const SESSION_COOKIE = 'mimorya_session';
export const SESSION_IDLE_MS = 12 * 60 * 60 * 1000;
export const SESSION_ABSOLUTE_MS = 7 * 24 * 60 * 60 * 1000;
/** Sliding expiry is persisted at most this often. */
export const SESSION_TOUCH_INTERVAL_MS = 5 * 60 * 1000;

/** Opaque 256-bit token; only its SHA-256 digest is stored. */
export function newSessionToken(): { token: string; tokenHash: string } {
  const token = randomBytes(32).toString('base64url');
  return { token, tokenHash: hashToken(token) };
}

export function hashToken(token: string): string {
  return createHash('sha256').update(token).digest('hex');
}
