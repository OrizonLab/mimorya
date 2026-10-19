import { NextResponse, type NextRequest } from 'next/server';
import { contentSecurityPolicy, newNonce } from '@/lib/security/content-security-policy';

/**
 * Gives every page its Content Security Policy with a fresh nonce. Next.js
 * reads the policy from the request headers and puts the nonce on its own
 * scripts and styles; pages are therefore rendered per request.
 */
export function proxy(request: NextRequest) {
  const policy = contentSecurityPolicy(newNonce(), process.env.NODE_ENV === 'development');
  const headers = new Headers(request.headers);
  headers.set('Content-Security-Policy', policy);
  const response = NextResponse.next({ request: { headers } });
  response.headers.set('Content-Security-Policy', policy);
  return response;
}

export const config = {
  matcher: [
    {
      // Pages only: not the API relay, static files nor prefetches.
      source: '/((?!api|_next/static|_next/image|favicon.ico).*)',
      missing: [
        { type: 'header', key: 'next-router-prefetch' },
        { type: 'header', key: 'purpose', value: 'prefetch' },
      ],
    },
  ],
};
