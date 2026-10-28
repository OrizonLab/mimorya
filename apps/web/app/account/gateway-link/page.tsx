'use client';

import { GatewayLinkConfirmation, LinkPage, useLinkToken } from '@/components/account';
import type { GatewayLinkCode, GatewayLinkPreview } from '@/lib/api';

const asCode = (code: string) => ({ code }) satisfies GatewayLinkCode;

/**
 * Opened from the link the company's Slack application sends
 * (`/account/gateway-link#<code>`, ADR-126), in the person's own session:
 * outside the application's frame, which would send a signed-out visitor to
 * the sign-in page and lose the link.
 */
export default function GatewayLinkPage() {
  const state = useLinkToken<GatewayLinkPreview>(
    '/account/gateways/link/preview',
    'Lien impossible à vérifier : réessayez.',
    asCode,
  );
  return (
    <LinkPage state={state}>
      {(code, preview) => <GatewayLinkConfirmation code={code} preview={preview} />}
    </LinkPage>
  );
}
