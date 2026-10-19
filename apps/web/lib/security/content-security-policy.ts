/**
 * The Content Security Policy of every page (SECURITY.md §10): scripts only
 * from this origin and carrying the request's nonce, which Next.js adds to
 * its own; no plugin, no framing, no foreign image (a model's answer cannot
 * leak data through an image URL). Development allows what the dev server
 * needs (`eval` for React's debugging, inline styles for its overlay).
 */
export function contentSecurityPolicy(nonce: string, development: boolean): string {
  const directives = [
    "default-src 'self'",
    `script-src 'self' 'nonce-${nonce}' 'strict-dynamic'${development ? " 'unsafe-eval'" : ''}`,
    development ? "style-src 'self' 'unsafe-inline'" : `style-src 'self' 'nonce-${nonce}'`,
    "img-src 'self' data: blob:",
    "font-src 'self'",
    "connect-src 'self'",
    "object-src 'none'",
    "base-uri 'none'",
    "form-action 'self'",
    "frame-ancestors 'none'",
  ];
  return directives.join('; ');
}

/** 128 random bits, base64: unpredictable and new for every page. */
export function newNonce(): string {
  const bytes = crypto.getRandomValues(new Uint8Array(16));
  return btoa(String.fromCharCode(...bytes));
}
