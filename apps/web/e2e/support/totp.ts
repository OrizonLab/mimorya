// An authenticator app, as the end-to-end tests play it (RFC 6238, ADR-200):
// the key the enrolment shows, read in base32, and the six-digit codes of
// its 30-second steps.
import { createHmac } from 'node:crypto';

const ALPHABET = 'ABCDEFGHIJKLMNOPQRSTUVWXYZ234567';

/** The bytes of a base32 key, as an app reads it: spaces ignored. */
function keyBytes(key: string): Buffer {
  let bits = '';
  for (const letter of key.replace(/\s/g, '').toUpperCase()) {
    const value = ALPHABET.indexOf(letter);
    if (value < 0) throw new Error(`not base32: ${letter}`);
    bits += value.toString(2).padStart(5, '0');
  }
  const bytes = bits.match(/.{8}/g) ?? [];
  return Buffer.from(bytes.map((byte) => parseInt(byte, 2)));
}

/** The code of `key` at `step` (HMAC-SHA1, dynamic truncation, six digits). */
function codeAt(key: Buffer, step: number): string {
  const counter = Buffer.alloc(8);
  counter.writeBigUInt64BE(BigInt(step));
  const mac = createHmac('sha1', key).update(counter).digest();
  const offset = mac.readUInt8(mac.length - 1) & 0x0f;
  const truncated = mac.readUInt32BE(offset) & 0x7fffffff;
  return (truncated % 1_000_000).toString().padStart(6, '0');
}

/**
 * The app of a key: each code it gives is at the step after the last one,
 * starting with the current step, so that the server, which takes each
 * step once and one step of drift, accepts every code within the minute.
 */
export class App {
  private readonly key: Buffer;
  private step = Math.floor(Date.now() / 30_000) - 1;

  constructor(key: string) {
    this.key = keyBytes(key);
  }

  next(): string {
    this.step += 1;
    return codeAt(this.key, this.step);
  }
}
