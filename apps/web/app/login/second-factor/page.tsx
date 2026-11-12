'use client';

import { AuthFrame } from '@/components/account';
import { SignInSecondFactor } from '@/components/second-factor';

/** The second step of a sign-in with a password (ADR-200, ADR-201), before any session. */
export default function SecondFactorPage() {
  return (
    <AuthFrame>
      <SignInSecondFactor />
    </AuthFrame>
  );
}
